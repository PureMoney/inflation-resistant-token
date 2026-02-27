import { StrategyType } from "@meteora-ag/dlmm";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { BN } from "@coral-xyz/anchor";
// import { POOL_ADDRESS, RESERVE_SYMBOL } from "./config.js";
// import { getActiveBins, updateActiveBins, Logger, logRebalancingEvent } from "./d1_logs.js";
// import IDL from "../../target/idl/irma.json";
// import { min } from "bn.js";

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
 * Execute counter swap in the on-chain program
 */
export async function cSwap(program, statePda, corePda, adminKeyPair, quoteToken, amount, exact_out, isMint) {
    
  // Hardcoded config keys for devUSDT (following IRMA conventions)
  const config_keys = [
    "147jRQy4cyE3jCuiCYoKwvfYjLkkoPxhaJPZovU9oSNS", // host fee account
    "HYeXEBUxLM4aFYSBmHRhMLwMP5wGDXMtEHTtx3VevkTD", // DLMM pair
    "9rMc8GnMfbq233ZhqjguRt37iXcKrt35LNgxj4ZVChZs", // position 1
    "BjE6syL6oswibYwzhVFFWWmPGYuBDKuRgjfjGFQu5HAt", // position 2
    "2GPfbE3E972LCqiBSsujyUvziAq1z5NvsBTWdVX8VTR9", // bin array 1
    "3eEiY1mqyka1E6WsZKLZnC7mDBT5Pn8zUkz1nqg2MEoA", // bin array 2
    "ADqpCiuXTnhDsXVaeZMbTpuriotmjGZUh4sptzzzmFmm", // IRMA mint
    "J2JAep9untmdaQXXRYB1bxT2eFNWWeR8ApuRdAiY9gni", // devUSDT mint
    "3QghBFXLYT2cJWG2b6HpNwoE2qDyRxvRCsbjaWwZwdH6",
    "8q6mdAFNQTqgJdUxFQTYyzAAsnwRstgVKchTdAjxbnPT",
    "3GbsvBADXgJufc9g5BnWnu1mbeUxPq9SukLeryyfSgir", // devUSDT account owned by the fed
    "Gjbk2AcwthyHgVSVbPb3US3MB5UM5FXE6z3m1WkaHb95", // "the fed" wallet account
    "9ZEqmbBp3QaT4z25xnQqdLLeRqb7Vej59vdgvHmVhwrk",
    "L93d6igVFXZKhcujZNWKeM1rH1XyqWmHttRoy5J3vg6",
    "5kgnXrzjgLAxcaYJZ4qvHZw4qZqYCoQm2L5pWdAACdZ5", // IRMA account owned by the USDT pool
    "9vtyTe9WhHSZgcN6dKhkh2cgzY9njyUQn4pNvjkwVzuj", // devUSDT account owned the USDT pool
    "D1ZN9Wj1fRSUQfCjhvnu1hqDMT7hzjzBBpi12nVniYD6", // authority
    "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo",  // DLMM program ID
    "GbsgfkY8aUq9c2kBE7aA5GG7HxATqnitdakJJBpp1qaa", // IRMA token account owned by the-fed
    "6NnDoJeGdo5vdMwc9eMpJyNSbbz7xMnH8eVqascPCXR1", // Oracle
    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",  // token program ID
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",  // token program ID
    "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr",
    "11111111111111111111111111111111", // System program ID
    "SysvarRent111111111111111111111111111111111", // Rent sysvar
    "SysvarC1ock11111111111111111111111111111111", // Clock sysvar
  ];

  try {
    console.log(`Simulating cSwap with quoteToken=${quoteToken}, amount=${amount}, exact_out=${exact_out}, isMint=${isMint}...`);
    const simulation = await program.methods
      .cSwap(quoteToken, amount, exact_out, isMint)
      .accounts({
        state: statePda,
        irmaAdmin: adminKeyPair.publicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(config_keys.map((key, index) => ({
        pubkey: new PublicKey(key),
        isSigner: index === 11, // Only the fed wallet should be a signer
        isWritable: // !key.startsWith('6NnDoJeGd') && 
                    !key.startsWith('TokenkegQ') && 
                    !key.startsWith('LBUZKhRx') &&
                    !key.startsWith('11111111') &&
                    !key.startsWith('TokenzQdB') &&
                    !key.startsWith('MemoSq4gq'), // Programs are read-only
      })))
      .simulate();
    
    if (simulation.err) {
      console.error("Simulation returned error:", JSON.stringify(simulation.err, null, 2));
      console.error("Simulation logs:", simulation.logs);
      return null;
    }
    console.log("Counter swap simulation successful:", simulation);
    const swapTx = await program.methods
      .cSwap(quoteToken, amount, exact_out, isMint)
      .accounts({
        state: statePda,
        irmaAdmin: adminKeyPair.publicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(config_keys.map((key, index) => ({
        pubkey: new PublicKey(key),
        isSigner: index === 11, // Only the fed wallet should be a signer
        isWritable: // !key.startsWith('6NnDoJeGd') && 
                    !key.startsWith('TokenkegQ') && 
                    !key.startsWith('11111111') &&
                    !key.startsWith('LBUZKhRx') &&
                    !key.startsWith('TokenzQdB') &&
                    !key.startsWith('MemoSq4gq'), // Programs are read-only
      })))
      .transaction();

    logger.log("DEBUG: Sending cSwap transaction...");
    swapTx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
    swapTx.feePayer = adminKeyPair.publicKey;
    swapTx.sign(adminKeyPair);
    const swapSig = await connection.sendRawTransaction(swapTx.serialize(), { skipPreflight: false });
    logger.log(`✅ Counter-swap sent: ${swapSig}`);
    await logger.flush();
    
    return swapSig;

  } catch (err) {
    // Extract all possible error information
    console.error("=== cSwap Full Error Details ===");
    console.error("Error type:", typeof err);
    console.error("Error constructor:", err.constructor.name);
    console.error("Error toString:", err.toString());
    
    // Log all enumerable properties
    console.error("Enumerable properties:", Object.keys(err));
    
    // Log all properties including non-enumerable
    console.error("All properties:", Object.getOwnPropertyNames(err));
    
    // Serialize with all properties
    console.error("Full error object:", JSON.stringify(err, Object.getOwnPropertyNames(err), 2));
    
    // Check for specific Anchor/Solana error properties
    if (err.error) {
      console.error("err.error:", err.error);
    }
    if (err.code) {
      console.error("err.code:", err.code);
    }
    if (err.logs) {
      console.error("Transaction logs:");
      err.logs.forEach((log, idx) => console.error(`  [${idx}] ${log}`));
    }
    if (err.errorLogs) {
      console.error("Error logs:", err.errorLogs);
    }
    if (err.simulationResponse) {
      console.error("Simulation response:", JSON.stringify(err.simulationResponse, null, 2));
    }
    
    // Try to access the inner cause
    if (err.cause) {
      console.error("Error cause:", err.cause);
    }
    
    // Stack trace
    if (err.stack) {
      console.error("Stack trace:", err.stack);
    }
    return null;
  }
}

/** Get current active position bins
 * for this user and liquidity pool
 */
export async function getCurrentPriceBins(dlmmPool, adminKeypair, logger, mintBinId, redemptionBinId) {
    // --- GET EXISTING POSITIONS ---
    const { userPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
    logger.log(`📍 Found ${userPositions.length} position(s)`);

    if (userPositions.length === 0) {
        logger.log(`⚠️ No positions found for user ${adminKeypair.publicKey.toBase58()}`);
        await logger.flush();
        return {};
    }
    else if (userPositions.length === 1 || 
      (userPositions.length === 2 && userPositions[0].publicKey === userPositions[1].publicKey)) {
        const pos = userPositions[0];
        logger.log(`ℹ️ Single position found, mint and redeem bins in same position, mint_bin_id = ${mintBinId}, redemption_bin_id = ${redemptionBinId}`);

        const mPositionBinData = pos.positionData.positionBinData.find(b => b.binId === mintBinId);
        const rPositionBinData = pos.positionData.positionBinData.find(b => b.binId === redemptionBinId);
        await logger.flush();
        return {
            mPositionBinData,
            rPositionBinData
        };
    }
    else if (userPositions.length === 2) {
        const pos1 = userPositions[0];
        const pos2 = userPositions[1];

        let mPositionBinData = null;
        let rPositionBinData = null;
        if (pos1.positionData.lowerBinId > pos2.positionData.lowerBinId) {
            mPositionBinData = pos1.positionData.positionBinData.find(b => b.binId === mintBinId);
            rPositionBinData = pos2.positionData.positionBinData.find(b => b.binId === redemptionBinId);
        } else {
            mPositionBinData = pos2.positionData.positionBinData.find(b => b.binId === mintBinId);
            rPositionBinData = pos1.positionData.positionBinData.find(b => b.binId === redemptionBinId);
        }
        logger.log(`ℹ️ Two positions found, mint_bin_id = ${mintBinId}, redemption_bin_id = ${redemptionBinId}`);
        await logger.flush();
        return {
            mPositionBinData,
            rPositionBinData
        };
    }
    else {
        logger.log(`⚠️ More than 2 positions found, remove extra positions to avoid issues`);
        await logger.flush();
        return {};
    }
}
