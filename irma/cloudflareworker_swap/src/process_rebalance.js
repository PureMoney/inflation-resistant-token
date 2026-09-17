import { 
  Connection, 
  Keypair, 
  PublicKey, 
  SystemProgram, 
  TransactionInstruction, 
  ComputeBudgetProgram 
} from "@solana/web3.js";
import { AnchorProvider, Program } from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import DLMM from "@meteora-ag/dlmm";
import { Logger, logSwapEvent, updateActiveBins } from "./d1_logs.js";
import { TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, getAssociatedTokenAddressSync } from "@solana/spl-token";
import { CustomWallet, getPrices, buildRemainingAccounts, placeLimitOrder, cancelLimitOrder, closeLimitOrderIfEmpty } from "./dlmm.js";
import irma from "../../target/idl/irma.json";


// ==================================================================
// REBALANCING FUNCTION for Swaps
// ==================================================================

const MEMO_PROGRAM_ID = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

export const WORKER_MEMO_STRING = "IRMA_WORKER_SWAP";

/**
 * Setup common Solana connection, wallet, and program
 */
export async function setupSolanaConnection(env) {
  const HELIUS_API_KEY = env.HELIUS_API_KEY;
  const HELIUS_RPC_URL = `https://devnet.helius-rpc.com/?api-key=${HELIUS_API_KEY}`;
  
  const secretString = env.ADMIN_PRIVATE_KEY;
  const secretKey = new Uint8Array(JSON.parse(secretString));
  const connection = new Connection(HELIUS_RPC_URL, "confirmed");
  const adminKeypair = Keypair.fromSecretKey(secretKey);
  
  const wallet = new CustomWallet(adminKeypair);
  const provider = new AnchorProvider(connection, wallet, { commitment: "confirmed" });
  const program = new Program(irma, provider);
  
  const programId = new PublicKey(irma.address);
  const [statePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("state_v5")],
    programId
  );
  const [corePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("core_v5")],
    programId
  );
  
  return { connection, adminKeypair, wallet, provider, program, statePda, corePda };
}


/// Main function to process a rebalance based on a swap event
export async function processRebalance(tx, env, ctx) {
  const logger = new Logger();
  
  // Track swap event data for logging
  const RESERVE_SYMBOL = env.RESERVE_SYMBOL;
  const RESERVE_MINT_STR = env.RESERVE_MINT_STR;
  const POOL_ADDRESS = env.POOL_ADDRESS;

  let swapEventData = {
    timestamp: Date.now(),
    eventType: null,
    reserveSymbol: RESERVE_SYMBOL,
    mintPrice: null,
    redemptionPrice: null,
    mintBinId: null,
    redemptionBinId: null,
    amountAtomic: '0',
    amountUi: 0,
    tradeEventSignature: null, // signature of the on-chain instruction that notifies IRMA of the swap event
    counterSwapSignature: null, // signature of the counter-swap transaction sent to the DLMM (if executed)
    txSignature: null, // signature of the original transaction that triggered the rebalance
    success: false,
  };

  async function notifyIRMAofTradeEvent(connection, program, statePda, corePda, adminKeypair, poolKey, amountAtomic) {
    logger.log(`📝 Building {${swapEventData.eventType}TradeEvent} transaction...`);
    const tradeTxPromise = 
      swapEventData.eventType === 'MINT' ?
        program.methods
          .saleTradeEvent(RESERVE_SYMBOL, amountAtomic)
          .accounts({
            state: statePda,
            irmaAdmin: adminKeypair.publicKey,
            core: corePda,
            systemProgram: SystemProgram.programId,
          })
          .remainingAccounts([
            { pubkey: poolKey, isSigner: false, isWritable: false }
          ])
          .transaction()
        :
        program.methods
          .buyTradeEvent(RESERVE_SYMBOL, amountAtomic)
          .accounts({
            state: statePda,
            irmaAdmin: adminKeypair.publicKey,
            core: corePda,
            systemProgram: SystemProgram.programId,
          })
          .remainingAccounts([
            { pubkey: poolKey, isSigner: false, isWritable: false }
          ])
          .transaction();

    const tradeTxInstruction = await tradeTxPromise;
    logger.log(`📝 Sending {${swapEventData.eventType}TradeEvent} transaction...`);
    tradeTxInstruction.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
    tradeTxInstruction.feePayer = adminKeypair.publicKey;
    tradeTxInstruction.sign(adminKeypair);
    
    const signature = await connection.sendRawTransaction(tradeTxInstruction.serialize(), { 
      skipPreflight: false,
      maxRetries: 0 // Don't wait for confirmation
    });
    logger.log(`✅ {${swapEventData.eventType}TradeEvent} transaction sent: ${signature}`);
    await logger.flush();
    return signature;
  }

  try {
    // --- DELTA CALC ---
    const preBalanceEntry = tx.meta.preTokenBalances.find(b => b.mint === RESERVE_MINT_STR && b.owner === POOL_ADDRESS);
    const postBalanceEntry = tx.meta.postTokenBalances.find(b => b.mint === RESERVE_MINT_STR && b.owner === POOL_ADDRESS);
    // const irmaOnTheWrongSide = tx.meta.postTokenBalances.find(b => b.mint === IRMA_MINT_STR && b.owner === POOL_ADDRESS);

    if (!preBalanceEntry || !postBalanceEntry) {
        logger.log("Ignored (No Pool Balance)");
        await logger.flush();
        return;
    }

    const preAmount = parseFloat(preBalanceEntry.uiTokenAmount.uiAmount);
    const postAmount = parseFloat(postBalanceEntry.uiTokenAmount.uiAmount);
    const delta = postAmount - preAmount;
    const decimals = preBalanceEntry.uiTokenAmount.decimals;

    // --- SETUP ---
    const { connection, adminKeypair, wallet, provider, program, statePda, corePda } = await setupSolanaConnection(env);

    const atomicValueString = Math.floor(Math.abs(delta) * (10 ** decimals)).toString();
    const amountAtomic = new BN(atomicValueString);
    swapEventData.amountAtomic = atomicValueString;

    swapEventData.txSignature = tx.transaction.signatures[0];

    // --- INITIALIZE DLMM POOL ---
    logger.log(`📊 Initializing DLMM pool...`);
    const poolKey = new PublicKey(POOL_ADDRESS);
    let dlmmPool;
    try {
      dlmmPool = await DLMM.create(connection, poolKey);
      logger.log(`✅ DLMM pool initialized`);
    }
    catch (dlmmError) {
      logger.error(`❌ Failed to initialize DLMM: ${dlmmError.message}`);
      throw dlmmError;
    }
    finally {
      await logger.flush();
    }

    // --- GET PRICES FROM IRMA PROGRAM ---
    logger.log(`📊 Fetching prices from IRMA program...`);
    let mintPrice, redemptionPrice;
    try {
      logger.log(`📊 Calling getPrices with symbol: ${RESERVE_SYMBOL}`);
      const prices = await getPrices(
        program,
        statePda,
        corePda,
        adminKeypair.publicKey,
        RESERVE_SYMBOL
      );
      logger.log(`📊 getPrices returned successfully`);
      mintPrice = prices.mintPrice;
      redemptionPrice = prices.redemptionPrice;
      swapEventData.mintPrice = mintPrice;
      swapEventData.redemptionPrice = redemptionPrice;
      logger.log(`📊 Mint Price: ${mintPrice}, Redemption Price: ${redemptionPrice}`);
    }
    catch (priceError) {
      logger.error(`❌ Failed to get prices: ${priceError.message}`);
      logger.error(`Stack: ${priceError.stack}`);
      throw priceError;
    }
    finally {
      await logger.flush();
    }

    // --- CONVERT PRICES TO BIN IDs ---
    const mintBinId = dlmmPool.getBinIdFromPrice(mintPrice.toString(), true);
    const redemptionBinId = dlmmPool.getBinIdFromPrice(redemptionPrice.toString(), false);
    const binDistance = Math.abs(mintBinId - redemptionBinId);
    
    swapEventData.mintBinId = mintBinId;
    swapEventData.redemptionBinId = redemptionBinId;

    logger.log(`📍 Mint Bin: ${mintBinId}, Redemption Bin: ${redemptionBinId}, Distance: ${binDistance}`);

    // The kind of swap it is can be reliably detected by whether it increased the reserve or not.
    // A trade that increases the reserve can only be a mint tx because the buyer of IRMA pays
    // using a reserve token. A trade that decreases the reserve can only be a redemption tx.
    swapEventData.eventType = delta > 0 ? 'MINT' : 'REDEMPTION';
    swapEventData.amountUi = Math.abs(delta);
    
    logger.log(`🚨 TRIGGER: ${delta > 0 ? "MINT" : "REDEMPTION"} Detected. Delta: ${delta}`);

    // all swapEventData necessary fields should be populated at this point, 
    // so we can log the event even if any of the subsequent steps fail
    let signature = await notifyIRMAofTradeEvent(
                    connection, program, statePda, corePda, adminKeypair, poolKey, amountAtomic);
    swapEventData.tradeEventSignature = signature;

    // TODO: get current price bins and determine which one is active
    // the one that is active must have both quote and IRMA tokens in it
    // check that it does, and do the counter-swap against that bin.
    // If neither bin has both tokens, this means there has been no recent swap: do nothing.
    
    // --- 1. Fetch active limit orders for this user/pool ---
    logger.log("🔍 Fetching active limit orders from DLMM pool...");
    const activeLimitOrders = await dlmmPool.getLimitOrderByUserAndLbPair(adminKeypair.publicKey);
    logger.log(`Found ${activeLimitOrders.length} active limit orders.`);

    // --- 2. Check if we need to cancel/claim existing limit orders ---
    for (const lo of activeLimitOrders) {
      const data = lo.limitOrderData;
      const binId = data.limitOrderBinData[0]?.binId;
      const isAskSide = data.limitOrderBinData[0]?.isAskSide;

      if (binId === undefined) {
        logger.log(`⚠️ Limit order ${lo.publicKey.toBase58()} has no bin data. Skipping.`);
        continue;
      }

      const targetBinId = isAskSide ? mintBinId : redemptionBinId;
      const totalFilledAmountX = new BN(data.totalFilledAmountX);
      const totalFilledAmountY = new BN(data.totalFilledAmountY);
      const totalSwappedAmountX = new BN(data.totalSwappedAmountX);
      const totalSwappedAmountY = new BN(data.totalSwappedAmountY);
      const isFilled = totalFilledAmountX.gt(new BN(0)) || totalFilledAmountY.gt(new BN(0)) ||
                       totalSwappedAmountX.gt(new BN(0)) || totalSwappedAmountY.gt(new BN(0));

      const isOutdatedBin = binId !== targetBinId;

      if (isFilled || isOutdatedBin) {
        logger.log(`🧹 Canceling/claiming limit order ${lo.publicKey.toBase58()} (binId=${binId}, target=${targetBinId}, isAskSide=${isAskSide}, isFilled=${isFilled}, isOutdatedBin=${isOutdatedBin})...`);
        try {
          const cancelSig = await cancelLimitOrder(
            connection,
            program,
            logger,
            statePda,
            corePda,
            adminKeypair,
            RESERVE_SYMBOL,
            lo.publicKey,
            [binId],
            dlmmPool,
            env
          );
          logger.log(`✅ Cancel transaction: ${cancelSig}`);

          logger.log(`👉 Closing empty limit order account ${lo.publicKey.toBase58()}...`);
          const closeSig = await closeLimitOrderIfEmpty(
            connection,
            program,
            logger,
            statePda,
            corePda,
            adminKeypair,
            lo.publicKey,
            dlmmPool,
            env
          );
          logger.log(`✅ Close transaction: ${closeSig}`);
        } catch (err) {
          logger.error(`❌ Failed to cancel/close limit order ${lo.publicKey.toBase58()}: ${err.message}`);
        }
      } else {
        logger.log(`ℹ️ Limit order ${lo.publicKey.toBase58()} is up-to-date and unfilled at binId=${binId}.`);
      }
    }

    // --- 3. Refresh active limit orders after cancellations ---
    const remainingLimitOrders = await dlmmPool.getLimitOrderByUserAndLbPair(adminKeypair.publicKey);
    const hasActiveAsk = remainingLimitOrders.some(lo => lo.limitOrderData.limitOrderBinData[0]?.isAskSide && lo.limitOrderData.limitOrderBinData[0]?.binId === mintBinId);
    const hasActiveBid = remainingLimitOrders.some(lo => !lo.limitOrderData.limitOrderBinData[0]?.isAskSide && lo.limitOrderData.limitOrderBinData[0]?.binId === redemptionBinId);

    // --- 4. Place new limit orders if missing ---
    if (!hasActiveAsk) {
      logger.log(`➕ No active Ask limit order at mintBinId=${mintBinId}. Placing new Ask limit order...`);
      const adminIrmaAta = getAssociatedTokenAddressSync(
        new PublicKey(env.IRMA_MINT_STR),
        adminKeypair.publicKey,
        false,
        TOKEN_2022_PROGRAM_ID
      );
      
      let irmaAmount = new BN("1000000000000000"); // Default target 1 billion IRMA
      try {
        const balanceRes = await connection.getTokenAccountBalance(adminIrmaAta);
        const irmaBalance = new BN(balanceRes.value.amount);
        logger.log(`IRMA wallet balance: ${irmaBalance.toString()} base units.`);
        if (irmaBalance.lt(irmaAmount)) {
          irmaAmount = irmaBalance;
        }
      } catch (balErr) {
        logger.warn(`Could not fetch IRMA balance, defaulting to target amount: ${balErr.message}`);
      }

      if (irmaAmount.gt(new BN(0))) {
        const askKeypair = Keypair.generate();
        logger.log(`Placing Ask limit order of ${irmaAmount.toString()} IRMA with limit_order pubkey=${askKeypair.publicKey.toBase58()} at binId=${mintBinId}...`);
        try {
          const sig = await placeLimitOrder(
            connection,
            program,
            logger,
            statePda,
            corePda,
            adminKeypair,
            RESERVE_SYMBOL,
            askKeypair,
            true, // isAskSide
            mintBinId,
            irmaAmount,
            dlmmPool,
            env
          );
          logger.log(`✅ Ask limit order placed successfully. Sig: ${sig}`);
        } catch (placeErr) {
          logger.error(`❌ Failed to place Ask limit order: ${placeErr.message}`);
        }
      } else {
        logger.warn("⚠️ IRMA balance is 0. Cannot place Ask limit order.");
      }
    } else {
      logger.log(`ℹ️ Ask limit order already exists at mintBinId=${mintBinId}.`);
    }

    if (!hasActiveBid) {
      logger.log(`➕ No active Bid limit order at redemptionBinId=${redemptionBinId}. Placing new Bid limit order...`);
      const adminReserveAta = getAssociatedTokenAddressSync(
        new PublicKey(RESERVE_MINT_STR),
        adminKeypair.publicKey,
        false,
        TOKEN_PROGRAM_ID
      );

      let reserveAmount = new BN("100000000000"); // Default target 100k stablecoin
      try {
        const balanceRes = await connection.getTokenAccountBalance(adminReserveAta);
        const reserveBalance = new BN(balanceRes.value.amount);
        logger.log(`Reserve wallet balance: ${reserveBalance.toString()} base units.`);
        if (reserveBalance.lt(reserveAmount)) {
          reserveAmount = reserveBalance;
        }
      } catch (balErr) {
        logger.warn(`Could not fetch Reserve balance, defaulting to target amount: ${balErr.message}`);
      }

      if (reserveAmount.gt(new BN(0))) {
        const bidKeypair = Keypair.generate();
        logger.log(`Placing Bid limit order of ${reserveAmount.toString()} reserve with limit_order pubkey=${bidKeypair.publicKey.toBase58()} at binId=${redemptionBinId}...`);
        try {
          const sig = await placeLimitOrder(
            connection,
            program,
            logger,
            statePda,
            corePda,
            adminKeypair,
            RESERVE_SYMBOL,
            bidKeypair,
            false, // isAskSide = false (Bid)
            redemptionBinId,
            reserveAmount,
            dlmmPool,
            env
          );
          logger.log(`✅ Bid limit order placed successfully. Sig: ${sig}`);
        } catch (placeErr) {
          logger.error(`❌ Failed to place Bid limit order: ${placeErr.message}`);
        }
      } else {
        logger.warn("⚠️ Reserve balance is 0. Cannot place Bid limit order.");
      }
    } else {
      logger.log(`ℹ️ Bid limit order already exists at redemptionBinId=${redemptionBinId}.`);
    }

    await logger.flush();

    // Update stored active bins in DB (don't await - fire and forget)
    await updateActiveBins(env.DB, {
      mintBinId,
      redemptionBinId,
      mintPrice,
      redemptionPrice,
    });

    swapEventData.success = true;
    logger.log(`✅ Workflow Complete`);
    await logger.flush();

  } catch (err) {
    logger.error(`Worker Error: ${err.message}`);
    console.error("Worker Error:", err);
    console.error("Error message:", err.message);
    if (err.logs) {
      logger.error(`Program logs: ${JSON.stringify(err.logs)}`);
      console.error("Program logs:", err.logs);
    }
    
    // Log failed swap event with waitUntil to ensure completion
    swapEventData.success = false;
    swapEventData.errorMessage = err.message;

  } finally {
    logger.log(`👉 Call on-chain function check_shift_price_range`);
    // TODO: pass necessary parameters to remove hardwired vars in check_shift_price_range()
    await check_shift_price_range_worker(env, logger);

    // Log the swap event and flush with waitUntil to ensure completion
    if (ctx) {
      ctx.waitUntil(Promise.all([
        logSwapEvent(env.DB, swapEventData),
        logger.flush()
      ]));
    } else {
      await logSwapEvent(env.DB, swapEventData);
      await logger.flush();
    }
  }
}



/**
 * Encapsulated check_shift_price_range function for Workers environment
 * Calls the on-chain instruction that determines whether any change in price
 * (either mint or redemption price) has caused a shift in any of the bins.
 * If so, it shifts the bins accordingly.
 * TODO: this should be in dlmm.js
 */
export async function check_shift_price_range_worker(env, logger) {
  logger.log("🚀 Worker version: Test integration with Meteora DLMM");
  
  try {
    const { connection, adminKeypair, wallet, provider, program, statePda, corePda } = await setupSolanaConnection(env);

    const reserve_token = env.RESERVE_SYMBOL;

    // Load the pool and derive all required accounts live from chain state
    const dlmmPool = await DLMM.create(connection, new PublicKey(env.POOL_ADDRESS));
    const remainingAccounts = await buildRemainingAccounts(dlmmPool, adminKeypair, env);

    // Call check_shift_price_ranges transaction
    logger.log("🔄 Calling check_shift_price_ranges() transaction...");
    const tx_sell = await program.methods
      .checkShiftPriceRanges(reserve_token)
      .accounts({
        state: statePda,
        irmaAdmin: adminKeypair.publicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(remainingAccounts)
      .transaction();

    // Add compute budget instructions to increase CU limit
    const computeLimitIx = ComputeBudgetProgram.setComputeUnitLimit({
        units: 1_000_000, // Request 1M compute units (5x the default)
    });
    
    const computePriceIx = ComputeBudgetProgram.setComputeUnitPrice({
        microLamports: 1000, // Set higher priority fee for faster processing
    });

    // Add compute budget instructions at the beginning
    tx_sell.instructions.unshift(computeLimitIx, computePriceIx);

    const signature = await connection.sendTransaction(tx_sell, [adminKeypair]);
    logger.log("🚀 On-chain checkShiftPriceRanges transaction sent:", signature);

    // Wait for confirmation
    await connection.confirmTransaction(signature, "confirmed");
    logger.log("✅ On-chain checkShiftPriceRanges transaction confirmed:", signature);
    await logger.flush();

    return { success: true, signature };
  } catch (error) {
    logger.error("❌ Error during check_shift_price_range:", error);
    await logger.flush();
    throw error;
  }
}
