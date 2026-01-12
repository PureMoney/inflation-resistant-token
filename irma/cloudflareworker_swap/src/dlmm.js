import { AnchorProvider, Program } from "@coral-xyz/anchor";
import DLMM, { StrategyType } from "@meteora-ag/dlmm";
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { BN } from "@coral-xyz/anchor";
import { POOL_ADDRESS, RESERVE_SYMBOL } from "./config.js";
import { getActiveBins, updateActiveBins, Logger, logRebalancingEvent } from "./d1_logs.js";
import IDL from "../../target/idl/irma.json";
import { min } from "bn.js";

// --- HELPER FUNCTIONS ---

/**
 * Remove all liquidity from a specific bin in a position
 * Returns the amounts withdrawn
 */
async function removeLiquidityFromBin(dlmmPool, connection, adminKeypair, position, binId, logger) {
  await logger.log(`📤 Removing liquidity from bin ${binId} in position ${position.publicKey.toBase58()}...`);
  
  try {
    // Get bin data to determine amounts
    const binData = position.positionData.positionBinData.find(b => b.binId === binId);
    if (!binData) {
      await logger.log(`⚠️ No liquidity found in bin ${binId}`);
      return { xAmount: new BN(0), yAmount: new BN(0), signature: null };
    }
    
    // Calculate the BPS to remove (100% = 10000 BPS)
    const binIdsToRemove = [binId];
    const bpsToRemove = new BN(10000); // 100%
    
    const removeLiquidityTx = await dlmmPool.removeLiquidity({
      position: position.publicKey,
      user: adminKeypair.publicKey,
      binIds: binIdsToRemove,
      bps: bpsToRemove,
      shouldClaimAndClose: false, // Don't close the position, just remove from this bin
    });
    
    // Sign and send
    for (const tx of Array.isArray(removeLiquidityTx) ? removeLiquidityTx : [removeLiquidityTx]) {
      tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
      tx.feePayer = adminKeypair.publicKey;
      tx.sign(adminKeypair);
      const sig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
      await logger.log(`✅ Removed liquidity from bin ${binId}: ${sig}`);
      
      // Parse withdrawn amounts from binData
      const xAmount = new BN(binData.positionXAmount || 0);
      const yAmount = new BN(binData.positionYAmount || 0);
      
      return { xAmount, yAmount, signature: sig };
    }
  } catch (err) {
    logger.error(`❌ Failed to remove liquidity from bin ${binId}: ${err.message}`);
    // throw err;
  }
  
  return { xAmount: new BN(0), yAmount: new BN(0), signature: null };
}

/**
 * Close an empty position
 */
async function closePosition(dlmmPool, connection, adminKeypair, position, logger) {
  await logger.log(`🗑️ Closing position ${position.publicKey.toBase58()}...`);
  
  try {
    // Close position - need to provide the position account info
    const closePositionTx = await dlmmPool.closePosition({
      position: position.publicKey,
      user: adminKeypair.publicKey,
    });
    
    for (const tx of Array.isArray(closePositionTx) ? closePositionTx : [closePositionTx]) {
      tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
      tx.feePayer = adminKeypair.publicKey;
      tx.sign(adminKeypair);
      const sig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
      await logger.log(`✅ Position closed: ${sig}`);
      return sig;
    }
  } catch (err) {
    await logger.error(`❌ Failed to close position: ${err.message}`);
    throw err;
  }
  
  return null;
}

/**
 * Add liquidity to a bin (creates position if needed)
 * Returns transaction signature
 */
async function addLiquidityToBin(dlmmPool, connection, adminKeypair, userPositions, binId, xAmount, yAmount, logger) {
  await logger.log(`📥 Adding liquidity to bin ${binId} (X: ${xAmount.toString()}, Y: ${yAmount.toString()})...`);
  
  try {
    // Find existing position that covers this bin
    let targetPosition = userPositions.find(pos => 
      pos.positionData.lowerBinId <= binId && pos.positionData.upperBinId >= binId
    );
    
    if (!targetPosition) {
      await logger.log(`📍 No existing position covers bin ${binId}, creating new position...`);
      
      const newPositionKeypair = Keypair.generate();
      const createPositionTx = await dlmmPool.initializePositionAndAddLiquidityByStrategy({
        positionPubKey: newPositionKeypair.publicKey,
        user: adminKeypair.publicKey,
        totalXAmount: xAmount,
        totalYAmount: yAmount,
        strategy: {
          minBinId: binId,
          maxBinId: binId,
          strategyType: StrategyType.Spot,
        },
      });
      
      for (const tx of Array.isArray(createPositionTx) ? createPositionTx : [createPositionTx]) {
        tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
        tx.feePayer = adminKeypair.publicKey;
        tx.partialSign(adminKeypair, newPositionKeypair);
        const sig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
        await logger.log(`✅ Created position and added liquidity to bin ${binId}: ${sig}`);
        return { signature: sig, positionPubkey: newPositionKeypair.publicKey };
      }
    } else {
      await logger.log(`📍 Using existing position: ${targetPosition.publicKey.toBase58()}`);
      
      const addLiquidityTx = await dlmmPool.addLiquidityByStrategy({
        positionPubKey: targetPosition.publicKey,
        user: adminKeypair.publicKey,
        totalXAmount: xAmount,
        totalYAmount: yAmount,
        strategy: {
          minBinId: binId,
          maxBinId: binId,
          strategyType: StrategyType.Spot,
        },
      });

      for (const tx of Array.isArray(addLiquidityTx) ? addLiquidityTx : [addLiquidityTx]) {
        tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
        tx.feePayer = adminKeypair.publicKey;
        tx.sign(adminKeypair);
        const sig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
        await logger.log(`✅ Added liquidity to bin ${binId}: ${sig}`);
        return { signature: sig, positionPubkey: targetPosition.publicKey };
      }
    }
  } catch (err) {
    await logger.error(`❌ Failed to add liquidity to bin ${binId}: ${err.message}`);
    throw err;
  }
  
  return { signature: null, positionPubkey: null };
}

// --- START EXPORTS ---

// --- CUSTOM WALLET ---
// Used as wallet adapter for AnchorProvider in Cloudflare Workers environment
export class CustomWallet {
  constructor(payer) {
    this.payer = payer;
  }
  async signTransaction(tx) {
    tx.partialSign(this.payer);
    return tx;
  }
  async signAllTransactions(txs) {
    return txs.map((t) => {
      t.partialSign(this.payer);
      return t;
    });
  }
  get publicKey() {
    return this.payer.publicKey;
  }
}

/**
 * Get both mint and redemption prices from IRMA program
 */
export async function getPrices(program, statePda, corePda, adminPublicKey, quoteToken) {
  try {
    const pricesResult = await program.methods
      .getPrices(quoteToken)
      .accounts({
        state: statePda,
        irmaAdmin: adminPublicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .simulate();
    
    // Look for the "Program return" line in the raw logs
    const returnLine = pricesResult.raw.find((line) => 
      line.includes("Program return:")
    );
    
    if (returnLine) {
      // Extract the base64 data from the return line
      const base64Data = returnLine.split(' ').pop();
      if (base64Data) {
        // Decode the base64 data
        const decodedData = Buffer.from(base64Data, 'base64');
        
        // Read two f64 values (8 bytes each, little-endian)
        if (decodedData.length >= 16) {
          const mintPrice = decodedData.readDoubleLE(0);
          const redemptionPrice = decodedData.readDoubleLE(8);
          return { mintPrice, redemptionPrice };
        }
      }
    }
    
    throw new Error("Failed to parse prices from simulation");
  } catch (err) {
    console.error("Simulation failed:", err);
    console.error("Simulation error message:", err.message);
    if (err.logs) {
      console.error("Simulation logs:", err.logs);
    } else if (err.simulationResponse && err.simulationResponse.logs) {
       console.error("Simulation logs (from response):", err.simulationResponse.logs);
    }
    throw err;
  }
}

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
  const program = new Program(IDL, provider);
  
  const programId = new PublicKey(IDL.address);
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

/**
 * Rebalance mint bin - move all IRMA from old mint bin to new mint bin
 */
export async function rebalanceMintBin(oldMintBinId, newMintBinId, dlmmPool, connection, adminKeypair, userPositions, logger) {
  await logger.log(`🔄 Rebalancing MINT bin: ${oldMintBinId} → ${newMintBinId}`);
  
  let totalIrmaRemoved = new BN(0);
  let removeSig = null;
  let addSig = null;
  let closeSig = null;
  
  // Find positions with liquidity in old mint bin
  for (const pos of userPositions) {
    logger.log(`🔍 Checking position: ${pos.publicKey.toBase58()}...`);
    if (pos.positionData.lowerBinId <= oldMintBinId && pos.positionData.upperBinId >= oldMintBinId) {
    //   const binData = pos.positionData.positionBinData.find(b => b.binId === oldMintBinId);
    //   if (binData && (binData.positionXAmount > 0 || binData.positionYAmount > 0)) {
    //     logger.log(`📍 Found liquidity in position ${pos.publicKey.toBase58()} at bin ${oldMintBinId}`);
        
        const { xAmount, yAmount, signature } = await removeLiquidityFromBin(
          dlmmPool, connection, adminKeypair, pos, oldMintBinId, logger
        );
        logger.log(`❗ Removed X amount: ${xAmount.toString()}, Y amount: ${yAmount.toString()}`);
        
        if (signature) removeSig = signature;
        totalIrmaRemoved = totalIrmaRemoved.add(xAmount);
        
        // Check if position is now empty and should be closed
        const remainingBins = pos.positionData.positionBinData.filter(
          b => b.binId !== oldMintBinId && (b.positionXAmount > 0 || b.positionYAmount > 0)
        );
        if (remainingBins.length === 0) {
          closeSig = await closePosition(dlmmPool, connection, adminKeypair, pos, logger);
        }
    //  }
    }
  }
  await logger.flush();
  
  // Add IRMA to new mint bin
  if (totalIrmaRemoved.gt(new BN(0))) {
    logger.log(`📦 Total IRMA removed: ${totalIrmaRemoved.toString()}`);

    // Refresh positions after removal
    const { userPositions: refreshedPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
    
    const result = await addLiquidityToBin(
      dlmmPool, connection, adminKeypair, refreshedPositions, 
      newMintBinId, totalIrmaRemoved, new BN(0), logger
    );
    addSig = result.signature;
  } else {
    logger.log(`ℹ️ No IRMA liquidity found in old mint bin ${oldMintBinId}`);
  }
  await logger.flush();
  
  return {
    irmaAmountMoved: totalIrmaRemoved.toString(),
    removeLiquiditySignature: removeSig,
    addLiquiditySignature: addSig,
    closePositionSignature: closeSig,
  };
}

/**
 * Rebalance redemption bin - move all reserve tokens from old redemption bin to new redemption bin
 */
export async function rebalanceRedemptionBin(env, oldRedemptionBinId, newRedemptionBinId, dlmmPool, connection, adminKeypair, userPositions, logger) {
  await logger.log(`🔄 Rebalancing REDEMPTION bin: ${oldRedemptionBinId} → ${newRedemptionBinId}`);
  
  let totalUsdcRemoved = new BN(0);
  let removeSig = null;
  let addSig = null;
  let closeSig = null;
  
  // Find positions with liquidity in old redemption bin
  for (const pos of userPositions) {
    if (pos.positionData.lowerBinId <= oldRedemptionBinId && pos.positionData.upperBinId >= oldRedemptionBinId) {
      const binData = pos.positionData.positionBinData.find(b => b.binId === oldRedemptionBinId);
      if (binData && (binData.positionXAmount > 0 || binData.positionYAmount > 0)) {
        await logger.log(`📍 Found liquidity in position ${pos.publicKey.toBase58()} at bin ${oldRedemptionBinId}`);
        
        const { xAmount, yAmount, signature } = await removeLiquidityFromBin(
          dlmmPool, connection, adminKeypair, pos, oldRedemptionBinId, logger
        );
        
        if (signature) removeSig = signature;
        totalUsdcRemoved = totalUsdcRemoved.add(yAmount);
        
        // Check if position is now empty and should be closed
        const remainingBins = pos.positionData.positionBinData.filter(
          b => b.binId !== oldRedemptionBinId && (b.positionXAmount > 0 || b.positionYAmount > 0)
        );
        if (remainingBins.length === 0) {
          closeSig = await closePosition(dlmmPool, connection, adminKeypair, pos, logger);
        }
      }
    }
  }
  
  // Add reserve token to new redemption bin
  if (totalUsdcRemoved.gt(new BN(0))) {
    await logger.log(`📦 Total ${RESERVE_SYMBOL} removed: ${totalUsdcRemoved.toString()}`);
    
    // Refresh positions after removal
    const { userPositions: refreshedPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
    
    const result = await addLiquidityToBin(
      dlmmPool, connection, adminKeypair, refreshedPositions, 
      newRedemptionBinId, new BN(0), totalUsdcRemoved, logger
    );
    addSig = result.signature;
  } else {
    await logger.log(`ℹ️ No ${RESERVE_SYMBOL} liquidity found in old redemption bin ${oldRedemptionBinId}`);
  }
  
  return {
    usdcAmountMoved: totalUsdcRemoved.toString(),
    removeLiquiditySignature: removeSig,
    addLiquiditySignature: addSig,
    closePositionSignature: closeSig,
  };
}

/**
 * Check and rebalance bins after price update
 * Compares stored active bins with new calculated bins
 */
export async function checkAndRebalanceBins(env, newMintPrice, newRedemptionPrice, triggerType = 'auto') {
  const logger = new Logger(env.DB);
  
  try {
    logger.log(`🔍 Checking if bin rebalancing is needed...`);
    
    const { connection, adminKeypair } = await setupSolanaConnection(env);
    
    // Initialize DLMM pool
    const poolKey = new PublicKey(POOL_ADDRESS);
    const dlmmPool = await DLMM.create(connection, poolKey);
    
    // Calculate new bin IDs from prices
    const newMintBinId = dlmmPool.getBinIdFromPrice(newMintPrice.toString(), true);
    const newRedemptionBinId = dlmmPool.getBinIdFromPrice(newRedemptionPrice.toString(), false);
    
    logger.log(`📊 New prices → Mint Bin: ${newMintBinId}, Redemption Bin: ${newRedemptionBinId}`);
    
    // Get stored active bins
    // const activeBins = await getActiveBins(env.DB);
    const activeBins = await getActivePositionBins(dlmmPool, adminKeypair, logger);
    
    let oldMintBinId = activeBins.mint_bin_id;
    let oldRedemptionBinId = activeBins.redemption_bin_id;
    // if (!activeBins) {
    //   logger.log(`ℹ️ No active bins stored yet, initializing...`);
    //   await updateActiveBins(env.DB, {
    //     mintBinId: newMintBinId,
    //     redemptionBinId: newRedemptionBinId,
    //     mintPrice: newMintPrice,
    //     redemptionPrice: newRedemptionPrice,
    //   });
    //   await logger.flush();
    //   // assume rebalancing is needed on first run
    //   oldMintBinId = newMintBinId - 1;
    //   oldRedemptionBinId = newRedemptionBinId + 1;
    //   // return { success: true, message: 'Active bins initialized', rebalanced: true };
    // }
    // else {

    //   oldMintBinId = activeBins.mint_bin_id;
    //   oldRedemptionBinId = activeBins.redemption_bin_id;
    // }

    logger.log(`📊 DLMM bins → Mint Bin: ${oldMintBinId}, Redemption Bin: ${oldRedemptionBinId}`);
    
    const mintBinChanged = Math.abs(newMintBinId - oldMintBinId) >= 1;
    const redemptionBinChanged = Math.abs(newRedemptionBinId - oldRedemptionBinId) >= 1;
    
    if (!mintBinChanged && !redemptionBinChanged) {
      logger.log(`✅ Bins are in sync, no rebalancing needed`);
      await logger.flush();
      return { success: true, message: 'Bins in sync', rebalanced: false };
    }
    
    // Get user positions
    const { userPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
    logger.log(`📍 Found ${userPositions.length} position(s) to check`);
    await logger.flush();
    
    let mintRebalanceResult = null;
    let redemptionRebalanceResult = null;
    
    // Rebalance mint bin if changed
    if (mintBinChanged) {
      logger.log(`🔄 Mint bin changed: ${oldMintBinId} → ${newMintBinId}`);
      try {
        mintRebalanceResult = await rebalanceMintBin(
          oldMintBinId, newMintBinId, 
          dlmmPool, connection, adminKeypair, userPositions, logger
        );
        
        await logRebalancingEvent(env.DB, {
          rebalanceType: 'mint_bin',
          oldMintBinId,
          newMintBinId,
          irmaAmountMoved: mintRebalanceResult.irmaAmountMoved,
          removeLiquiditySignature: mintRebalanceResult.removeLiquiditySignature,
          addLiquiditySignature: mintRebalanceResult.addLiquiditySignature,
          closePositionSignature: mintRebalanceResult.closePositionSignature,
          triggerType,
          success: true,
        });
      } catch (err) {
        logger.error(`❌ Mint bin rebalancing failed: ${err.message}`);
        console.error(`❌ Mint bin rebalancing failed: ${err.message}`);
        
        await logRebalancingEvent(env.DB, {
          rebalanceType: 'mint_bin',
          oldMintBinId,
          newMintBinId,
          triggerType,
          success: false,
          errorMessage: err.message,
        });
      }
    }
    
    // Rebalance redemption bin if changed
    if (redemptionBinChanged) {
      logger.log(`🔄 Redemption bin changed: ${oldRedemptionBinId} → ${newRedemptionBinId}`);
      
      // Refresh positions if mint rebalancing happened
      let currentPositions = userPositions;
      if (mintRebalanceResult) {
        const { userPositions: refreshed } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
        currentPositions = refreshed;
      }
      
      try {
        redemptionRebalanceResult = await rebalanceRedemptionBin(
          env, oldRedemptionBinId, newRedemptionBinId, 
          dlmmPool, connection, adminKeypair, currentPositions, logger
        );
        
        await logRebalancingEvent(env.DB, {
          rebalanceType: 'redemption_bin',
          oldRedemptionBinId,
          newRedemptionBinId,
          usdcAmountMoved: redemptionRebalanceResult.usdcAmountMoved,
          removeLiquiditySignature: redemptionRebalanceResult.removeLiquiditySignature,
          addLiquiditySignature: redemptionRebalanceResult.addLiquiditySignature,
          closePositionSignature: redemptionRebalanceResult.closePositionSignature,
          triggerType,
          success: true,
        });
      } catch (err) {
        logger.error(`❌ Redemption bin rebalancing failed: ${err.message}`);
        console.error(`❌ Redemption bin rebalancing failed: ${err.message}`);
        
        await logRebalancingEvent(env.DB, {
          rebalanceType: 'redemption_bin',
          oldRedemptionBinId,
          newRedemptionBinId,
          triggerType,
          success: false,
          errorMessage: err.message,
        });
      }
    }
    
    // Update stored active bins
    await updateActiveBins(env.DB, {
      mintBinId: newMintBinId,
      redemptionBinId: newRedemptionBinId,
      mintPrice: newMintPrice,
      redemptionPrice: newRedemptionPrice,
    });
    
    logger.log(`✅ Rebalancing complete, active bins updated`);
    await logger.flush();
    
    return {
      success: true,
      rebalanced: true,
      mintBinChanged,
      redemptionBinChanged,
      oldMintBinId,
      newMintBinId,
      oldRedemptionBinId,
      newRedemptionBinId,
      mintRebalanceResult,
      redemptionRebalanceResult,
    };
  } catch (err) {
    logger.error(`❌ Bin rebalancing check failed: ${err.message}`);
    console.error(`❌ Bin rebalancing check failed: ${err.message}`);
    await logger.flush();
    throw err;
  }
}

/**
 * Manual rebalancing endpoint - forces rebalance based on current prices
 */
export async function manualRebalanceBins(env) {
  const logger = new Logger(env.DB);
  
  try {
    logger.log(`🔧 Manual rebalancing triggered...`);
    
    const { connection, adminKeypair, program, statePda, corePda } = await setupSolanaConnection(env);
    logger.log(`🔑 Admin: ${adminKeypair.publicKey.toBase58()}`);
    
    // Get current prices from IRMA program
    const prices = await getPrices(program, statePda, corePda, adminKeypair.publicKey, RESERVE_SYMBOL);
    logger.log(`📊 Current prices - Mint: ${prices.mintPrice}, Redemption: ${prices.redemptionPrice}`);
    
    // Force rebalance by temporarily clearing active bins
    const poolKey = new PublicKey(POOL_ADDRESS);
    const dlmmPool = await DLMM.create(connection, poolKey);
    
    const newMintBinId = dlmmPool.getBinIdFromPrice(prices.mintPrice.toString(), true);
    const newRedemptionBinId = dlmmPool.getBinIdFromPrice(prices.redemptionPrice.toString(), false);
    
    // Get stored bins
    // const activeBins = await getActiveBins(env.DB);
    const activeBins = await getActivePositionBins(dlmmPool, adminKeypair, logger);

    let oldMintBinId = activeBins.mint_bin_id;
    let oldRedemptionBinId = activeBins.redemption_bin_id;
    
    // if (!activeBins) {
    //   // No bins stored, just initialize
    //   await updateActiveBins(env.DB, {
    //     mintBinId: newMintBinId,
    //     redemptionBinId: newRedemptionBinId,
    //     mintPrice: prices.mintPrice,
    //     redemptionPrice: prices.redemptionPrice,
    //   });
    //   logger.log(`ℹ️ Active bins initialized (first time)`);
    //   await logger.flush();
    //   // assume rebalancing is needed on first run
    //   oldMintBinId = newMintBinId - 1;
    //   oldRedemptionBinId = newRedemptionBinId + 1;
    //   // return { success: true, message: 'Active bins initialized', rebalanced: false };
    // }
    // else {

    //   oldMintBinId = activeBins.mint_bin_id;
    //   oldRedemptionBinId = activeBins.redemption_bin_id;
    // }
    
    // Get user positions
    const { userPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
    
    let results = { mintRebalanced: false, redemptionRebalanced: false };
    
    // Always attempt rebalancing if bins differ
    if (oldMintBinId !== newMintBinId) {
      try {
        const mintResult = await rebalanceMintBin(
          oldMintBinId, newMintBinId, 
          dlmmPool, connection, adminKeypair, userPositions, logger
        );
        results.mintRebalanced = true;
        results.mintResult = mintResult;
        
        await logRebalancingEvent(env.DB, {
          rebalanceType: 'mint_bin',
          oldMintBinId,
          newMintBinId,
          irmaAmountMoved: mintResult.irmaAmountMoved,
          removeLiquiditySignature: mintResult.removeLiquiditySignature,
          addLiquiditySignature: mintResult.addLiquiditySignature,
          closePositionSignature: mintResult.closePositionSignature,
          triggerType: 'manual',
          success: true,
        });
      } catch (err) {
        results.mintError = err.message;
        logger.error(`❌ Manual mint bin rebalancing failed: ${err.message}`);
        console.error(`❌ Manual mint bin rebalancing failed: ${err.message}`);
      }
    }
    await logger.flush();
    
    if (oldRedemptionBinId !== newRedemptionBinId) {
      // Refresh positions
      const { userPositions: refreshed } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
      
      try {
        const redemptionResult = await rebalanceRedemptionBin(
          env, oldRedemptionBinId, newRedemptionBinId, 
          dlmmPool, connection, adminKeypair, refreshed, logger
        );
        results.redemptionRebalanced = true;
        results.redemptionResult = redemptionResult;
        
        await logRebalancingEvent(env.DB, {
          rebalanceType: 'redemption_bin',
          oldRedemptionBinId,
          newRedemptionBinId,
          usdcAmountMoved: redemptionResult.usdcAmountMoved,
          removeLiquiditySignature: redemptionResult.removeLiquiditySignature,
          addLiquiditySignature: redemptionResult.addLiquiditySignature,
          closePositionSignature: redemptionResult.closePositionSignature,
          triggerType: 'manual',
          success: true,
        });
      } catch (err) {
        results.redemptionError = err.message;
        logger.error(`❌ Manual redemption bin rebalancing failed: ${err.message}`);
        console.error(`❌ Manual redemption bin rebalancing failed: ${err.message}`);
      }
    }
    await logger.flush();
    
    // Update active bins
    await updateActiveBins(env.DB, {
      mintBinId: newMintBinId,
      redemptionBinId: newRedemptionBinId,
      mintPrice: prices.mintPrice,
      redemptionPrice: prices.redemptionPrice,
    });
    
    await logger.log(`✅ Manual rebalancing complete`);
    await logger.flush();
    
    return {
      success: true,
      ...results,
      currentBins: { mintBinId: newMintBinId, redemptionBinId: newRedemptionBinId },
      previousBins: { mintBinId: oldMintBinId, redemptionBinId: oldRedemptionBinId },
    };
  } catch (err) {
    logger.error(`❌ Manual rebalancing failed: ${err.message}`);
    console.error(`❌ Manual rebalancing failed: ${err.message}`);
    await logger.flush();
    throw err;
  }
}

/** Get current active position bins
 * for this user and liquidity pool
 */
export async function getActivePositionBins(dlmmPool, adminKeypair, logger) {
    // const { connection, adminKeypair, wallet, provider, program, statePda, corePda } = await setupSolanaConnection(env);
    // --- GET EXISTING POSITIONS ---
    const { userPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
    logger.log(`📍 Found ${userPositions.length} position(s)`);

    if (userPositions.length === 0) {
        logger.log(`⚠️ No positions found for user ${adminKeypair.publicKey.toBase58()}`);
        await logger.flush();
        return {};
    }
    else if (userPositions.length === 1) {
        const pos = userPositions[0];
        const mint_bin_id = pos.positionData.lowerBinId;
        logger.log(`ℹ️ Single position found, assume it is the mint position, mint_bin_id = ${mint_bin_id}`);
        await logger.flush();
        return {
            // publicKey: pos.publicKey.toBase58(),
            mint_bin_id,
            redemption_bin_id: null
        };
    }
    else if (userPositions.length === 2) {
        const pos1 = userPositions[0];
        const pos2 = userPositions[1];

        let mint_bin_id = null;
        let redemption_bin_id = null;

        if (pos1.positionData.lowerBinId > pos2.positionData.lowerBinId) {
            mint_bin_id = pos1.positionData.lowerBinId;
            redemption_bin_id = pos2.positionData.lowerBinId;
        } else {
            mint_bin_id = pos2.positionData.lowerBinId;
            redemption_bin_id = pos1.positionData.lowerBinId;
        }
        logger.log(`ℹ️ Two positions found, mint_bin_id = ${mint_bin_id}, redemption_bin_id = ${redemption_bin_id}`);
        await logger.flush();
        return {
            mint_bin_id,
            redemption_bin_id
        };
    }
    else {
        logger.log(`⚠️ More than 2 positions found, remove extra positions to avoid issues`);
        await logger.flush();
        return {};
    }
}
