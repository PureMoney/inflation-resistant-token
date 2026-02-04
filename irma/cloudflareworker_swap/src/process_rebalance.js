const MEMO_PROGRAM_ID = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
import { Connection, Keypair, PublicKey, SystemProgram, TransactionInstruction } from "@solana/web3.js";
import { AnchorProvider, Program } from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import DLMM, { StrategyType } from "@meteora-ag/dlmm";
import { check_shift_price_range } from "./check_shift_price_ranges.js";
import { POOL_ADDRESS, RESERVE_MINT_STR, RESERVE_SYMBOL } from "./config.js";
import { Logger, logSwapEvent, updateActiveBins } from "./d1_logs.js";
import { CustomWallet, getPrices } from "./dlmm.js";
import IDL from "../../target/idl/irma.json";


// ==================================================================
// REBALANCING FUNCTION for Swaps
// ==================================================================


export async function processRebalance(tx, env, ctx) {
  const logger = new Logger(env.DB);
  const HELIUS_API_KEY = env.HELIUS_API_KEY;
  const HELIUS_RPC_URL = `https://devnet.helius-rpc.com/?api-key=${HELIUS_API_KEY}`;
  
  // Track swap event data for logging
  let swapEventData = {
    timestamp: Date.now(),
    eventType: null,
    reserveSymbol: RESERVE_SYMBOL,
    amountAtomic: '0',
    amountUi: 0,
    success: false,
  };
  
  try {
    // --- DELTA CALC ---
    const preBalanceEntry = tx.meta.preTokenBalances.find(b => b.mint === RESERVE_MINT_STR && b.owner === POOL_ADDRESS);
    const postBalanceEntry = tx.meta.postTokenBalances.find(b => b.mint === RESERVE_MINT_STR && b.owner === POOL_ADDRESS);

    if (!preBalanceEntry || !postBalanceEntry) {
        logger.log("Ignored (No Pool Balance)");
        await logger.flush();
        return;
    }

    const preAmount = parseFloat(preBalanceEntry.uiTokenAmount.uiAmount);
    const postAmount = parseFloat(postBalanceEntry.uiTokenAmount.uiAmount);
    const delta = postAmount - preAmount;
    const decimals = preBalanceEntry.uiTokenAmount.decimals;

    if (delta !== 0) {
      // The kind of swap it is can be reliably detected by whether it increased the reserve or not.
      // A trade that increases the reserve can only be a mint tx because the buyer of IRMA pays
      // using a reserve token. A trade that decreases the reserve can only be a redemption tx.
      swapEventData.eventType = delta > 0 ? 'MINT' : 'REDEMPTION';
      swapEventData.amountUi = Math.abs(delta);
      
      logger.log(`🚨 TRIGGER: ${delta > 0 ? "MINT" : "REDEMPTION"} Detected. Delta: ${delta}`);

      // --- SETUP ---
      const secretString = env.ADMIN_PRIVATE_KEY;
      const secretKey = new Uint8Array(JSON.parse(secretString));
      const connection = new Connection(HELIUS_RPC_URL, "confirmed");
      const adminKeypair = Keypair.fromSecretKey(secretKey);
      logger.log(`🔑 Admin Public Key: ${adminKeypair.publicKey.toBase58()}`);
      
      const wallet = new CustomWallet(adminKeypair);
      const provider = new AnchorProvider(connection, wallet, { commitment: "confirmed" });
      const program = new Program(IDL, provider);

      // Derive PDAs with v5 seeds
      const programId = new PublicKey(IDL.address);
      const [statePda] = PublicKey.findProgramAddressSync(
        [Buffer.from("state_v5")],
        programId
      );
      
      const [corePda] = PublicKey.findProgramAddressSync(
        [Buffer.from("core_v5")],
        programId
      );

      const atomicValueString = Math.floor(Math.abs(delta) * (10 ** decimals)).toString();
      const amountAtomic = new BN(atomicValueString);
      swapEventData.amountAtomic = atomicValueString;

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
      } catch (priceError) {
        logger.error(`❌ Failed to get prices: ${priceError.message}`);
        logger.error(`Stack: ${priceError.stack}`);
        throw priceError;
      }

      // --- INITIALIZE DLMM POOL ---
      await logger.log(`📊 Initializing DLMM pool...`);
      const poolKey = new PublicKey(POOL_ADDRESS);
      let dlmmPool;
      try {
        dlmmPool = await DLMM.create(connection, poolKey);
        logger.log(`✅ DLMM pool initialized`);
      } catch (dlmmError) {
        logger.error(`❌ Failed to initialize DLMM: ${dlmmError.message}`);
        throw dlmmError;
      }

      // --- CONVERT PRICES TO BIN IDs ---
      const mintBinId = dlmmPool.getBinIdFromPrice(mintPrice.toString(), true);
      const redemptionBinId = dlmmPool.getBinIdFromPrice(redemptionPrice.toString(), false);
      const binDistance = Math.abs(mintBinId - redemptionBinId);
      
      swapEventData.mintBinId = mintBinId;
      swapEventData.redemptionBinId = redemptionBinId;

      logger.log(`📍 Mint Bin: ${mintBinId}, Redemption Bin: ${redemptionBinId}, Distance: ${binDistance}`);

      // Check if rebalancing is needed (skip if same bin or adjacent bins)
      if (binDistance < 2) {
        logger.log(`⏭️ Bins too close (distance: ${binDistance}), skipping rebalancing`);
        // Still call trade event but no counter-swap
        if (delta > 0) {
          logger.log(`📝 Building saleTradeEvent transaction...`);
          const tradeTxInstruction = await program.methods
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
            .transaction();

          logger.log(`📝 Sending saleTradeEvent transaction...`);
          tradeTxInstruction.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
          tradeTxInstruction.feePayer = adminKeypair.publicKey;
          tradeTxInstruction.sign(adminKeypair);
          
          const tradeTx = await connection.sendRawTransaction(tradeTxInstruction.serialize(), { 
            skipPreflight: false,
            maxRetries: 0 // Don't wait for confirmation
          });
          logger.log(`✅ Sale trade event sent (no rebalancing): ${tradeTx}`);
          swapEventData.txSignature = tradeTx;
        } else {
          logger.log(`📝 Building buyTradeEvent transaction...`);
          const tradeTxInstruction = await program.methods
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

          logger.log(`📝 Sending buyTradeEvent transaction...`);
          tradeTxInstruction.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
          tradeTxInstruction.feePayer = adminKeypair.publicKey;
          tradeTxInstruction.sign(adminKeypair);
          
          const tradeTx = await connection.sendRawTransaction(tradeTxInstruction.serialize(), { 
            skipPreflight: false,
            maxRetries: 0 // Don't wait for confirmation
          });
          logger.log(`✅ Buy trade event sent (no rebalancing): ${tradeTx}`);
          swapEventData.txSignature = tradeTx;
        }
        
        logger.log(`📝 Trade event complete, preparing to log and return`);
        swapEventData.success = true;
        
        // Use waitUntil for logging operations to ensure they complete
        if (ctx) {
          ctx.waitUntil(Promise.all([
            logSwapEvent(env.DB, swapEventData),
            logger.flush()
          ]));
        } else {
          await logSwapEvent(env.DB, swapEventData);
          await logger.flush();
        }
        logger.log(`📝 Returning from early exit`);
        return;
      }

      // --- GET EXISTING POSITIONS ---
      const { userPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
      logger.log(`📍 Found ${userPositions.length} position(s)`);
      
      for (const pos of userPositions) {
        logger.log(`   Position ${pos.publicKey.toBase58()}: bins ${pos.positionData.lowerBinId} to ${pos.positionData.upperBinId}`);
      }
      
      // --- GET STORED ACTIVE BINS FOR REBALANCING ---
      // const storedActiveBins = await getActiveBins(env.DB);
      // const currentActiveBins = await getActivePositionBins(dlmmPool, adminKeypair, logger);

      // =========================================================
      // LOGIC: MINT EVENT (User bought IRMA using reserve token or quote token)
      // =========================================================
      if (delta > 0) {
        logger.log(`👉 Step 1: Performing counter-swap (IRMA -> ${RESERVE_SYMBOL})...`);

        const swapForY = true; 
        
        logger.log("DEBUG: Fetching bin arrays...");
        const binArrays = await dlmmPool.getBinArrayForSwap(swapForY);
        
        logger.log("DEBUG: Getting swap quote...");
        const irmaSwapAmount = amountAtomic.mul(new BN(95)).div(new BN(100));
        logger.log(`💰 User delta: ${amountAtomic.toString()}, Counter-swap IRMA amount: ${irmaSwapAmount.toString()}`);
        
        const swapQuote = await dlmmPool.swapQuote(
          irmaSwapAmount, 
          swapForY,
          new BN(1500),
          binArrays
        );
        
        const reserveOutputAmount = swapQuote.minOutAmount;
        logger.log(`💰 Expected ${RESERVE_SYMBOL} output: ${reserveOutputAmount.toString()}`);

        logger.log("DEBUG: Creating swap transaction...");
        const swapTx = await dlmmPool.swap({
          inToken: dlmmPool.tokenX.publicKey,
          outToken: dlmmPool.tokenY.publicKey,
          inAmount: irmaSwapAmount,
          binArraysPubkey: swapQuote.binArraysPubkey,
          lbPair: poolKey,
          user: adminKeypair.publicKey,
          minOutAmount: reserveOutputAmount,
        });

        if (swapTx.add) {
            swapTx.add(new TransactionInstruction({
                keys: [],
                programId: MEMO_PROGRAM_ID,
                data: Buffer.from(WORKER_MEMO_STRING, "utf-8"),
            }));
        }

        logger.log("DEBUG: Sending transaction...");
        swapTx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
        swapTx.feePayer = adminKeypair.publicKey;
        swapTx.sign(adminKeypair);
        const swapSig = await connection.sendRawTransaction(swapTx.serialize(), { skipPreflight: false });
        logger.log(`✅ Counter-swap sent: ${swapSig}`);
        swapEventData.counterSwapSignature = swapSig;

        logger.log(`👉 Step 2: Call on-chain function check_shift_price_range`);
        // TODO: pass necessary parameters to remove hardwired vars in check_shift_price_range()
        await check_shift_price_range();
        // const rebalanceResult = await checkAndRebalanceBins(
        //   env, 
        //   prices.mintPrice, 
        //   prices.redemptionPrice, 
        //   'auto'
        // );
        
        swapEventData.liquiditySignature = liquiditySig;

        await logger.log(`👉 Step 3: Recording sale trade event...`);
        await logger.log(`💰 Recording sale with token: "${RESERVE_SYMBOL}", amount: ${amountAtomic.toString()}`);
        
        const tradeTxInstruction = await program.methods
          .saleTradeEvent(RESERVE_SYMBOL, amountAtomic)
          .accounts({
            state: statePda,
            irmaAdmin: adminKeypair.publicKey,
            core: corePda,
            systemProgram: SystemProgram.programId,
          })
          .remainingAccounts([{
            pubkey: new PublicKey(POOL_ADDRESS),
            isSigner: false,
            isWritable: false,
          }])
          .transaction();
        
        tradeTxInstruction.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
        tradeTxInstruction.feePayer = adminKeypair.publicKey;
        tradeTxInstruction.sign(adminKeypair);
        
        const tradeTx = await connection.sendRawTransaction(tradeTxInstruction.serialize(), { 
          skipPreflight: false,
          maxRetries: 0
        });

        await logger.log(`✅ Sale trade event sent: ${tradeTx}`);
        swapEventData.txSignature = tradeTx;

      } else {
        // =========================================================
        // LOGIC: REDEMPTION EVENT (User sold IRMA for reserve token or quote token)
        // =========================================================
        await logger.log(`👉 Step 1: Performing counter-swap (${RESERVE_SYMBOL} -> IRMA)...`);

        const swapForY = false; 
        
        await logger.log("DEBUG: Fetching bin arrays...");
        const binArrays = await dlmmPool.getBinArrayForSwap(swapForY);
        
        await logger.log("DEBUG: Getting swap quote...");
        const reserveSwapAmount = amountAtomic.abs().mul(new BN(95)).div(new BN(100));
        await logger.log(`💰 User delta: ${amountAtomic.toString()}, Counter-swap ${RESERVE_SYMBOL} amount: ${reserveSwapAmount.toString()}`);

        const swapQuote = await dlmmPool.swapQuote(
          reserveSwapAmount,
          swapForY,
          new BN(1500),
          binArrays
        );

        const irmaOutputAmount = swapQuote.minOutAmount;
        await logger.log(`💰 Expected IRMA output: ${irmaOutputAmount.toString()}`);
        
        await logger.log("DEBUG: Creating swap transaction...");
        const swapTx = await dlmmPool.swap({
          inToken: dlmmPool.tokenY.publicKey,
          outToken: dlmmPool.tokenX.publicKey,
          inAmount: reserveSwapAmount,
          binArraysPubkey: swapQuote.binArraysPubkey,
          lbPair: poolKey,
          user: adminKeypair.publicKey,
          minOutAmount: irmaOutputAmount,
        });

        if (swapTx.add) {
            swapTx.add(new TransactionInstruction({
                keys: [],
                programId: MEMO_PROGRAM_ID,
                data: Buffer.from(WORKER_MEMO_STRING, "utf-8"),
            }));
        }

        await logger.log("DEBUG: Sending transaction...");
        swapTx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
        swapTx.feePayer = adminKeypair.publicKey;
        swapTx.sign(adminKeypair);
        const swapSig = await connection.sendRawTransaction(swapTx.serialize(), { skipPreflight: false });
        await logger.log(`✅ Counter-swap sent: ${swapSig}`);
        swapEventData.counterSwapSignature = swapSig;

        await logger.log(`👉 Step 2: Call check_shift_price_ranges ...`);
        // TODO: pass necessary parameters to remove hardwired vars in check_shift_price_range()
        await check_shift_price_range();
        // const rebalanceResult = await checkAndRebalanceBins(
        //   env, 
        //   prices.mintPrice, 
        //   prices.redemptionPrice, 
        //   'auto'
        // );
        
        swapEventData.liquiditySignature = liquiditySig;

        await logger.log(`👉 Step 3: Recording buy trade event...`);

        const tradeTxInstruction = await program.methods
          .buyTradeEvent(RESERVE_SYMBOL, totalIrmaToAdd)
          .accounts({
            state: statePda,
            irmaAdmin: adminKeypair.publicKey,
            core: corePda,
            systemProgram: SystemProgram.programId,
          })
          .remainingAccounts([{
            pubkey: new PublicKey(POOL_ADDRESS),
            isSigner: false,
            isWritable: false,
          }])
          .transaction();
        
        tradeTxInstruction.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
        tradeTxInstruction.feePayer = adminKeypair.publicKey;
        tradeTxInstruction.sign(adminKeypair);
        
        const tradeTx = await connection.sendRawTransaction(tradeTxInstruction.serialize(), { 
          skipPreflight: false,
          maxRetries: 0
        });

        await logger.log(`✅ Buy trade event sent: ${tradeTx}`);
        swapEventData.txSignature = tradeTx;
      }

      // Update stored active bins (don't await - fire and forget)
      await updateActiveBins(env.DB, {
        mintBinId,
        redemptionBinId,
        mintPrice,
        redemptionPrice,
      });

      swapEventData.success = true;
      logger.log(`✅ Workflow Complete`);
    }
    
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
