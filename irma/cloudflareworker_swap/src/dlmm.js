import { StrategyType } from "@meteora-ag/dlmm";
import { Keypair, PublicKey, SystemProgram, SYSVAR_RENT_PUBKEY, SYSVAR_CLOCK_PUBKEY } from "@solana/web3.js";
import { BN } from "@coral-xyz/anchor";
import { getAssociatedTokenAddressSync, TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID } from "@solana/spl-token";

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

const DLMM_PROGRAM_ID = new PublicKey("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");
const MEMO_PROGRAM = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

/**
 * Build the remainingAccounts array for checkShiftPriceRanges / cSwap by
 * fetching live pool state rather than relying on hardcoded addresses.
 *
 * Accounts included:
 *  - DLMM LbPair (pool)
 *  - All admin positions in the pool (fetched on-chain)
 *  - Bin arrays covering both swap directions (fetched on-chain)
 *  - IRMA and reserve mints
 *  - Admin ATAs for IRMA and reserve
 *  - Pool vault accounts (tokenX.reserve, tokenY.reserve)
 *  - Pool authority (vault owner PDA)
 *  - Oracle account
 *  - Admin wallet (signer)
 *  - Event authority PDA, program IDs, sysvars
 */
export async function buildRemainingAccounts(dlmmPool, adminKeypair, env) {
  const irmaMint    = new PublicKey(env.IRMA_MINT_STR);
  const reserveMint = new PublicKey(env.RESERVE_MINT_STR);

  // Fetch live positions and bin arrays concurrently
  const [{ userPositions }, binArraysX2Y, binArraysY2X] = await Promise.all([
    dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey),
    dlmmPool.getBinArrayForSwap(false),
    dlmmPool.getBinArrayForSwap(true),
  ]);

  const binArrayKeySet = new Set([
    ...binArraysX2Y.map(b => b.publicKey.toBase58()),
    ...binArraysY2X.map(b => b.publicKey.toBase58()),
  ]);

  // Include nearby bin-array + position PDAs so on-chain shifts can initialize/replace
  // positions (and initialize bin arrays) without requiring hardcoded addresses.
  const BIN_ARRAY_SIZE = 70;
  const i32le = (n) => {
    const b = Buffer.alloc(4);
    b.writeInt32LE(n, 0);
    return b;
  };
  const i64le = (n) => {
    const b = Buffer.alloc(8);
    b.writeBigInt64LE(BigInt(n), 0);
    return b;
  };

  const nearbyIndices = new Set();
  for (const p of userPositions) {
    if (p?.positionData?.lowerBinId === undefined) continue;
    const idx = Math.floor(p.positionData.lowerBinId / BIN_ARRAY_SIZE);
    for (const d of [-1, 0, 1]) nearbyIndices.add(idx + d);
  }

  const derivedMetas = [...nearbyIndices].flatMap((idx) => {
    const lowerBinId = idx * BIN_ARRAY_SIZE;
    const [positionPda] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("position"),
        dlmmPool.pubkey.toBuffer(),
        adminKeypair.publicKey.toBuffer(),
        i32le(lowerBinId),
        i32le(BIN_ARRAY_SIZE),
      ],
      DLMM_PROGRAM_ID
    );

    const [binArrayPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("bin_array"), dlmmPool.pubkey.toBuffer(), i64le(idx)],
      DLMM_PROGRAM_ID
    );

    return [
      { pubkey: positionPda, isSigner: false, isWritable: true },
      { pubkey: binArrayPda, isSigner: false, isWritable: true },
    ];
  });

  // Admin ATAs (synchronous derivation)
  const adminIrmaAta    = getAssociatedTokenAddressSync(irmaMint,    adminKeypair.publicKey, false, TOKEN_2022_PROGRAM_ID);
  const adminReserveAta = getAssociatedTokenAddressSync(reserveMint, adminKeypair.publicKey, false, TOKEN_PROGRAM_ID);

  // Pool vault addresses and authority come from the loaded pool state
  const poolIrmaVault    = dlmmPool.tokenX.reserve;
  const poolReserveVault = dlmmPool.tokenY.reserve;
  const poolAuthority    = dlmmPool.tokenX.owner;
  const oracle           = dlmmPool.lbPair.oracle;

  // Anchor event authority PDA (standard for all Anchor programs)
  const [eventAuthority] = PublicKey.findProgramAddressSync(
    [Buffer.from("__event_authority")],
    DLMM_PROGRAM_ID
  );

  const entries = [
    { pubkey: dlmmPool.pubkey,            isSigner: false, isWritable: true  },
    ...derivedMetas,
    ...userPositions.map(p => ({ pubkey: p.publicKey,      isSigner: false, isWritable: true  })),
    ...[...binArrayKeySet].map(k => ({ pubkey: new PublicKey(k), isSigner: false, isWritable: true })),
    { pubkey: irmaMint,                   isSigner: false, isWritable: false },
    { pubkey: reserveMint,                isSigner: false, isWritable: false },
    { pubkey: adminIrmaAta,               isSigner: false, isWritable: true  },
    { pubkey: adminReserveAta,            isSigner: false, isWritable: true  },
    { pubkey: poolIrmaVault,              isSigner: false, isWritable: true  },
    { pubkey: poolReserveVault,           isSigner: false, isWritable: true  },
    { pubkey: poolAuthority,              isSigner: false, isWritable: false },
    { pubkey: oracle,                     isSigner: false, isWritable: true  },
    { pubkey: adminKeypair.publicKey,     isSigner: true,  isWritable: true  },
    { pubkey: eventAuthority,             isSigner: false, isWritable: false },
    ...(dlmmPool.tokenX.transferHookAccountMetas || []),
    { pubkey: DLMM_PROGRAM_ID,            isSigner: false, isWritable: false },
    { pubkey: TOKEN_2022_PROGRAM_ID,      isSigner: false, isWritable: false },
    { pubkey: TOKEN_PROGRAM_ID,           isSigner: false, isWritable: false },
    { pubkey: SystemProgram.programId,    isSigner: false, isWritable: false },
    { pubkey: SYSVAR_RENT_PUBKEY,         isSigner: false, isWritable: false },
    { pubkey: SYSVAR_CLOCK_PUBKEY,        isSigner: false, isWritable: false },
    { pubkey: MEMO_PROGRAM,               isSigner: false, isWritable: false },
  ];

  // Deduplicate by pubkey (keep first occurrence)
  const seen = new Set();
  return entries.filter(({ pubkey }) => {
    const key = pubkey.toBase58();
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

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
 * Derive the bin array PDA for a given bin ID.
 */
export function deriveBinArrayPda(lbPairPubkey, binId) {
  const binArrayIndex = Math.floor(binId / 70);
  const binArrayIndexBuffer = Buffer.alloc(8);
  binArrayIndexBuffer.writeBigInt64LE(BigInt(binArrayIndex), 0);
  
  const [pubkey] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("bin_array"),
      lbPairPubkey.toBuffer(),
      binArrayIndexBuffer
    ],
    DLMM_PROGRAM_ID
  );
  return pubkey;
}

/**
 * Place a limit order.
 */
export async function placeLimitOrder(
  connection,
  program,
  logger,
  statePda,
  corePda,
  adminKeyPair,
  symbol,
  limitOrderKeypair,
  isAskSide,
  binId,
  amount,
  dlmmPool,
  env
) {
  const binArrayPda = deriveBinArrayPda(dlmmPool.pubkey, binId);
  const remainingAccounts = await buildRemainingAccounts(dlmmPool, adminKeyPair, env);

  if (!remainingAccounts.some(acc => acc.pubkey.equals(limitOrderKeypair.publicKey))) {
    remainingAccounts.push({ pubkey: limitOrderKeypair.publicKey, isSigner: true, isWritable: true });
  }
  if (!remainingAccounts.some(acc => acc.pubkey.equals(binArrayPda))) {
    remainingAccounts.push({ pubkey: binArrayPda, isSigner: false, isWritable: true });
  }

  try {
    console.log(`Simulating placeLimitOrder for symbol=${symbol}, binId=${binId}, amount=${amount}, isAskSide=${isAskSide}...`);
    const simulation = await program.methods
      .placeLimitOrder(symbol, limitOrderKeypair.publicKey, isAskSide, binId, new BN(amount))
      .accounts({
        state: statePda,
        irmaAdmin: adminKeyPair.publicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(remainingAccounts)
      .signers([limitOrderKeypair])
      .simulate();

    if (simulation.err) {
      console.error("placeLimitOrder simulation returned error:", JSON.stringify(simulation.err, null, 2));
      console.error("Simulation logs:", simulation.logs);
      return null;
    }
    console.log("placeLimitOrder simulation successful");

    const tx = await program.methods
      .placeLimitOrder(symbol, limitOrderKeypair.publicKey, isAskSide, binId, new BN(amount))
      .accounts({
        state: statePda,
        irmaAdmin: adminKeyPair.publicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(remainingAccounts)
      .transaction();

    tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
    tx.feePayer = adminKeyPair.publicKey;
    tx.partialSign(adminKeyPair, limitOrderKeypair);

    logger.log("DEBUG: Sending placeLimitOrder transaction...");
    const sig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
    logger.log(`✅ Limit order placed: ${sig}`);
    await logger.flush();
    return sig;
  } catch (err) {
    console.error("Failed to place limit order:", err);
    throw err;
  }
}

/**
 * Cancel/claim a limit order.
 */
export async function cancelLimitOrder(
  connection,
  program,
  logger,
  statePda,
  corePda,
  adminKeyPair,
  symbol,
  limitOrderPubkey,
  binIds,
  dlmmPool,
  env
) {
  const remainingAccounts = await buildRemainingAccounts(dlmmPool, adminKeyPair, env);

  if (!remainingAccounts.some(acc => acc.pubkey.equals(limitOrderPubkey))) {
    remainingAccounts.push({ pubkey: limitOrderPubkey, isSigner: false, isWritable: true });
  }

  for (const binId of binIds) {
    const binArrayPda = deriveBinArrayPda(dlmmPool.pubkey, binId);
    if (!remainingAccounts.some(acc => acc.pubkey.equals(binArrayPda))) {
      remainingAccounts.push({ pubkey: binArrayPda, isSigner: false, isWritable: true });
    }
  }

  try {
    console.log(`Simulating cancelLimitOrder for symbol=${symbol}, limitOrder=${limitOrderPubkey.toBase58()}...`);
    const simulation = await program.methods
      .cancelLimitOrder(symbol, limitOrderPubkey, binIds)
      .accounts({
        state: statePda,
        irmaAdmin: adminKeyPair.publicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(remainingAccounts)
      .simulate();

    if (simulation.err) {
      console.error("cancelLimitOrder simulation returned error:", JSON.stringify(simulation.err, null, 2));
      console.error("Simulation logs:", simulation.logs);
      return null;
    }
    console.log("cancelLimitOrder simulation successful");

    const tx = await program.methods
      .cancelLimitOrder(symbol, limitOrderPubkey, binIds)
      .accounts({
        state: statePda,
        irmaAdmin: adminKeyPair.publicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(remainingAccounts)
      .transaction();

    tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
    tx.feePayer = adminKeyPair.publicKey;
    tx.sign(adminKeyPair);

    logger.log("DEBUG: Sending cancelLimitOrder transaction...");
    const sig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
    logger.log(`✅ Limit order canceled/claimed: ${sig}`);
    await logger.flush();
    return sig;
  } catch (err) {
    console.error("Failed to cancel limit order:", err);
    throw err;
  }
}

/**
 * Close an empty limit order.
 */
export async function closeLimitOrderIfEmpty(
  connection,
  program,
  logger,
  statePda,
  corePda,
  adminKeyPair,
  limitOrderPubkey,
  dlmmPool,
  env
) {
  const remainingAccounts = await buildRemainingAccounts(dlmmPool, adminKeyPair, env);

  if (!remainingAccounts.some(acc => acc.pubkey.equals(limitOrderPubkey))) {
    remainingAccounts.push({ pubkey: limitOrderPubkey, isSigner: false, isWritable: true });
  }

  try {
    console.log(`Simulating closeLimitOrderIfEmpty for limitOrder=${limitOrderPubkey.toBase58()}...`);
    const simulation = await program.methods
      .closeLimitOrderIfEmpty(limitOrderPubkey)
      .accounts({
        state: statePda,
        irmaAdmin: adminKeyPair.publicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(remainingAccounts)
      .simulate();

    if (simulation.err) {
      console.error("closeLimitOrderIfEmpty simulation returned error:", JSON.stringify(simulation.err, null, 2));
      console.error("Simulation logs:", simulation.logs);
      return null;
    }
    console.log("closeLimitOrderIfEmpty simulation successful");

    const tx = await program.methods
      .closeLimitOrderIfEmpty(limitOrderPubkey)
      .accounts({
        state: statePda,
        irmaAdmin: adminKeyPair.publicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(remainingAccounts)
      .transaction();

    tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
    tx.feePayer = adminKeyPair.publicKey;
    tx.sign(adminKeyPair);

    logger.log("DEBUG: Sending closeLimitOrderIfEmpty transaction...");
    const sig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
    logger.log(`✅ Limit order closed: ${sig}`);
    await logger.flush();
    return sig;
  } catch (err) {
    console.error("Failed to close limit order:", err);
    throw err;
  }
}


