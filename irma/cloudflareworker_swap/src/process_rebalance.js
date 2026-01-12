const MEMO_PROGRAM_ID = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
import { Connection, Keypair, PublicKey, SystemProgram, TransactionInstruction } from "@solana/web3.js";
import { AnchorProvider, Program } from "@coral-xyz/anchor";
import { BN } from "@coral-xyz/anchor";
import DLMM, { StrategyType } from "@meteora-ag/dlmm";
import { POOL_ADDRESS, RESERVE_MINT_STR, RESERVE_SYMBOL } from "./config.js";
import { Logger, /* getActiveBins, */  logSwapEvent, logRebalancingEvent, updateActiveBins } from "./d1_logs.js";
import { CustomWallet, getPrices, rebalanceMintBin, rebalanceRedemptionBin, getActivePositionBins } from "./dlmm.js";
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
      const currentActiveBins = await getActivePositionBins(dlmmPool, adminKeypair, logger);

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

        logger.log(`👉 Step 2: Adding ${RESERVE_SYMBOL} to redemption bin ${redemptionBinId}...`);

        // Check if redemption bin has changed and we need to rebalance
        const oldRedemptionBinId = currentActiveBins?.redemption_bin_id;
        let totalReserveToAdd = reserveOutputAmount;
        
        if (oldRedemptionBinId && oldRedemptionBinId !== redemptionBinId) {
          logger.log(`🔄 Redemption bin changed: ${oldRedemptionBinId} → ${redemptionBinId}, rebalancing...`);
          
          // Remove liquidity from old redemption bin first
          try {
            const rebalanceResult = await rebalanceRedemptionBin(
              env, oldRedemptionBinId, redemptionBinId,
              dlmmPool, connection, adminKeypair, userPositions, logger
            );
            
            // Add the recovered reserve to our total
            if (rebalanceResult.usdcAmountMoved && rebalanceResult.usdcAmountMoved !== '0') {
              totalReserveToAdd = totalReserveToAdd.add(new BN(rebalanceResult.usdcAmountMoved));
              logger.log(`📦 Added ${rebalanceResult.usdcAmountMoved} ${RESERVE_SYMBOL} from old bin to deposit`);
            }
            
            await logRebalancingEvent(env.DB, {
              rebalanceType: 'redemption_bin',
              oldRedemptionBinId,
              newRedemptionBinId: redemptionBinId,
              usdcAmountMoved: rebalanceResult.usdcAmountMoved,
              removeLiquiditySignature: rebalanceResult.removeLiquiditySignature,
              triggerType: 'auto_swap',
              success: true,
            });
          } catch (rebalanceErr) {
            logger.error(`❌ Redemption bin rebalancing failed: ${rebalanceErr.message}`);
            console.error(`❌ Redemption bin rebalancing failed: ${rebalanceErr.message}`);
            // Continue with original amount
          }
        }
        
        // Refresh positions after potential rebalancing
        const { userPositions: refreshedPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);

        // Find or create position for redemption bin
        let redemptionPosition = refreshedPositions.find(pos => 
          pos.positionData.lowerBinId <= redemptionBinId && pos.positionData.upperBinId >= redemptionBinId
        );
        
        let liquiditySig = null;
        if (!redemptionPosition) {
          await logger.log(`📍 No position covers redemption bin ${redemptionBinId}, creating new position...`);
          
          const newPositionKeypair = Keypair.generate();
          const createPositionTx = await dlmmPool.initializePositionAndAddLiquidityByStrategy({
            positionPubKey: newPositionKeypair.publicKey,
            user: adminKeypair.publicKey,
            totalXAmount: new BN(0),
            totalYAmount: totalReserveToAdd,
            strategy: {
              minBinId: redemptionBinId,
              maxBinId: redemptionBinId,
              strategyType: StrategyType.Spot,
            },
          });
          
          for (const tx of Array.isArray(createPositionTx) ? createPositionTx : [createPositionTx]) {
            tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
            tx.feePayer = adminKeypair.publicKey;
            tx.partialSign(adminKeypair, newPositionKeypair);
            liquiditySig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
            await logger.log(`✅ Created position and added liquidity: ${liquiditySig}`);
          }
        } else {
          await logger.log(`📍 Using existing position: ${redemptionPosition.publicKey.toBase58()}`);
          
          const addLiquidityTx = await dlmmPool.addLiquidityByStrategy({
            positionPubKey: redemptionPosition.publicKey,
            user: adminKeypair.publicKey,
            totalXAmount: new BN(0),
            totalYAmount: totalReserveToAdd,
            strategy: {
              minBinId: redemptionBinId,
              maxBinId: redemptionBinId,
              strategyType: StrategyType.Spot,
            },
          });

          for (const tx of Array.isArray(addLiquidityTx) ? addLiquidityTx : [addLiquidityTx]) {
            tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
            tx.feePayer = adminKeypair.publicKey;
            tx.sign(adminKeypair);
            liquiditySig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
            await logger.log(`✅ Liquidity addition sent to redemption bin: ${liquiditySig}`);
          }
        }
        
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

        await logger.log(`👉 Step 2: Adding IRMA to mint bin ${mintBinId}...`);

        // Check if mint bin has changed and we need to rebalance
        const oldMintBinId = currentActiveBins?.mint_bin_id;
        let totalIrmaToAdd = irmaOutputAmount;
        
        if (oldMintBinId && oldMintBinId !== mintBinId) {
          await logger.log(`🔄 Mint bin changed: ${oldMintBinId} → ${mintBinId}, rebalancing...`);
          
          // Remove liquidity from old mint bin first
          try {
            const rebalanceResult = await rebalanceMintBin(
              oldMintBinId, mintBinId,
              dlmmPool, connection, adminKeypair, userPositions, logger
            );
            
            // Add the recovered IRMA to our total
            if (rebalanceResult.irmaAmountMoved && rebalanceResult.irmaAmountMoved !== '0') {
              totalIrmaToAdd = totalIrmaToAdd.add(new BN(rebalanceResult.irmaAmountMoved));
              await logger.log(`📦 Added ${rebalanceResult.irmaAmountMoved} IRMA from old bin to deposit`);
            }
            
            await logRebalancingEvent(env.DB, {
              rebalanceType: 'mint_bin',
              oldMintBinId,
              newMintBinId: mintBinId,
              irmaAmountMoved: rebalanceResult.irmaAmountMoved,
              removeLiquiditySignature: rebalanceResult.removeLiquiditySignature,
              triggerType: 'auto_swap',
              success: true,
            });
          } catch (rebalanceErr) {
            await logger.error(`❌ Mint bin rebalancing failed: ${rebalanceErr.message}`);
            console.error(`❌ Mint bin rebalancing failed: ${rebalanceErr.message}`);
            // Continue with original amount
          }
        }

        await logger.log(`💰 IRMA amount to add: ${totalIrmaToAdd.toString()}`);
        
        // Refresh positions after potential rebalancing
        const { userPositions: refreshedPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
        
        // Find or create position for mint bin
        let mintPosition = refreshedPositions.find(pos => 
          pos.positionData.lowerBinId <= mintBinId && pos.positionData.upperBinId >= mintBinId
        );
        
        let liquiditySig = null;
        if (!mintPosition) {
          await logger.log(`📍 No position covers mint bin ${mintBinId}, creating new position...`);
          
          const newPositionKeypair = Keypair.generate();
          const createPositionTx = await dlmmPool.initializePositionAndAddLiquidityByStrategy({
            positionPubKey: newPositionKeypair.publicKey,
            user: adminKeypair.publicKey,
            totalXAmount: totalIrmaToAdd,
            totalYAmount: new BN(0),
            strategy: {
              minBinId: mintBinId,
              maxBinId: mintBinId,
              strategyType: StrategyType.Spot,
            },
          });
          
          for (const tx of Array.isArray(createPositionTx) ? createPositionTx : [createPositionTx]) {
            tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
            tx.feePayer = adminKeypair.publicKey;
            tx.partialSign(adminKeypair, newPositionKeypair);
            liquiditySig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
            await logger.log(`✅ Created position and added liquidity: ${liquiditySig}`);
          }
        } else {
          await logger.log(`📍 Using existing position: ${mintPosition.publicKey.toBase58()}`);
          
          const addLiquidityTx = await dlmmPool.addLiquidityByStrategy({
            positionPubKey: mintPosition.publicKey,
            user: adminKeypair.publicKey,
            totalXAmount: totalIrmaToAdd,
            totalYAmount: new BN(0),
            strategy: {
              minBinId: mintBinId,
              maxBinId: mintBinId,
              strategyType: StrategyType.Spot,
            },
          });

          for (const tx of Array.isArray(addLiquidityTx) ? addLiquidityTx : [addLiquidityTx]) {
            tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
            tx.feePayer = adminKeypair.publicKey;
            tx.sign(adminKeypair);
            liquiditySig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
            await logger.log(`✅ Liquidity addition sent to mint bin: ${liquiditySig}`);
          }
        }
        
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
        await logger.flush()
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
