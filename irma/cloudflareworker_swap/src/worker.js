// 1. IMPORTS
import { Connection, Keypair, PublicKey, SystemProgram, TransactionInstruction } from "@solana/web3.js";
import { Program, AnchorProvider, BN } from "@coral-xyz/anchor";
import { Buffer } from "buffer";
import DLMM, { StrategyType } from "@meteora-ag/dlmm";
import IDL from "../../target/idl/irma.json";

const MEMO_PROGRAM_ID = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
const WORKER_MEMO_STRING = "IRMA_WORKER_SWAP";

// ==================================================================
// LOGGING UTILITIES (D1 Database)
// ==================================================================

// Generate unique request ID for log correlation
function generateRequestId() {
  return `${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
}

// Logger class that writes to both console and D1
class Logger {
  constructor(db, requestId = null) {
    this.db = db;
    this.requestId = requestId || generateRequestId();
    this.buffer = [];
  }

  log(message, context = null) {
    const timestamp = Date.now();
    console.log(message);
    this.buffer.push({ timestamp, level: 'log', message: String(message), context, requestId: this.requestId });
  }

  warn(message, context = null) {
    const timestamp = Date.now();
    console.warn(message);
    this.buffer.push({ timestamp, level: 'warn', message: String(message), context, requestId: this.requestId });
  }

  error(message, context = null) {
    const timestamp = Date.now();
    console.error(message);
    this.buffer.push({ timestamp, level: 'error', message: String(message), context, requestId: this.requestId });
  }

  debug(message, context = null) {
    const timestamp = Date.now();
    console.log(`[DEBUG] ${message}`);
    this.buffer.push({ timestamp, level: 'debug', message: String(message), context, requestId: this.requestId });
  }

  // Flush all buffered logs to D1 (non-blocking, fire-and-forget)
  // Stores all messages from this request as a single row with JSON array
  flush() {
    if (!this.db || this.buffer.length === 0) return Promise.resolve();
    
    const logsToFlush = [...this.buffer];
    this.buffer = []; // Clear buffer immediately
    
    // Aggregate all logs into a single row with messages as JSON array
    const firstLog = logsToFlush[0];
    const lastLog = logsToFlush[logsToFlush.length - 1];
    
    // Create messages array with timestamp, level, and message for each log
    const messages = logsToFlush.map(log => ({
      timestamp: log.timestamp,
      level: log.level,
      message: log.message,
      context: log.context
    }));
    
    // Insert single row with all messages
    return this.db.prepare(
      'INSERT INTO console_logs (timestamp, level, message, context, request_id) VALUES (?, ?, ?, ?, ?)'
    ).bind(
      firstLog.timestamp, // Start timestamp
      'batch', // Special level indicating this is a batch
      `${logsToFlush.length} log entries`, // Summary message
      JSON.stringify({ messages, duration: lastLog.timestamp - firstLog.timestamp }), // All messages in context
      firstLog.requestId
    ).run().catch(err => {
      console.error('Failed to flush logs to D1:', err.message);
    });
  }
}

// Event logging functions (non-blocking)
function logSwapEvent(db, eventData) {
  if (!db) return Promise.resolve();
  
  return db.prepare(`
    INSERT INTO swap_events (
      timestamp, event_type, reserve_symbol, amount_atomic, amount_ui,
      tx_signature, counter_swap_signature, liquidity_signature,
      mint_bin_id, redemption_bin_id, mint_price, redemption_price,
      success, error_message
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).bind(
    eventData.timestamp || Date.now(),
    eventData.eventType,
    eventData.reserveSymbol,
    eventData.amountAtomic,
    eventData.amountUi,
    eventData.txSignature || null,
    eventData.counterSwapSignature || null,
    eventData.liquiditySignature || null,
    eventData.mintBinId || null,
    eventData.redemptionBinId || null,
    eventData.mintPrice || null,
    eventData.redemptionPrice || null,
    eventData.success ? 1 : 0,
    eventData.errorMessage || null
  ).run().catch(err => {
    console.error('Failed to log swap event:', err.message);
  });
}

function logPriceUpdate(db, eventData) {
  if (!db) return Promise.resolve();
  
  return db.prepare(`
      INSERT INTO price_updates (
        timestamp, inflation_rate, quote_token_price_usd,
        old_mint_price, new_mint_price,
        old_mint_bin_id, new_mint_bin_id,
        old_redemption_bin_id, new_redemption_bin_id,
        tx_signature, trigger_type, success, error_message
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).bind(
      eventData.timestamp || Date.now(),
      eventData.inflationRate,
      eventData.quoteTokenPriceUsd,
      eventData.oldMintPrice || null,
      eventData.newMintPrice,
      eventData.oldMintBinId || null,
      eventData.newMintBinId || null,
      eventData.oldRedemptionBinId || null,
      eventData.newRedemptionBinId || null,
      eventData.txSignature || null,
      eventData.triggerType,
      eventData.success ? 1 : 0,
      eventData.errorMessage || null
    ).run().catch(err => {
      console.error('Failed to log price update:', err.message);
    });
}

function logRebalancingEvent(db, eventData) {
  if (!db) return Promise.resolve();
  
  return db.prepare(`
      INSERT INTO rebalancing_events (
        timestamp, rebalance_type,
        old_mint_bin_id, new_mint_bin_id,
        old_redemption_bin_id, new_redemption_bin_id,
        irma_amount_moved, usdc_amount_moved,
        remove_liquidity_signature, add_liquidity_signature, close_position_signature,
        trigger_type, success, error_message
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `).bind(
      eventData.timestamp || Date.now(),
      eventData.rebalanceType,
      eventData.oldMintBinId || null,
      eventData.newMintBinId || null,
      eventData.oldRedemptionBinId || null,
      eventData.newRedemptionBinId || null,
      eventData.irmaAmountMoved || null,
      eventData.usdcAmountMoved || null,
      eventData.removeLiquiditySignature || null,
      eventData.addLiquiditySignature || null,
      eventData.closePositionSignature || null,
      eventData.triggerType,
      eventData.success ? 1 : 0,
      eventData.errorMessage || null
    ).run().catch(err => {
      console.error('Failed to log rebalancing event:', err.message);
    });
}

// Active bins management
async function getActiveBins(db) {
  if (!db) return null;
  try {
    const result = await db.prepare('SELECT * FROM active_bins WHERE id = 1').first();
    return result;
  } catch (err) {
    console.error('Failed to get active bins:', err.message);
    return null;
  }
}

function updateActiveBins(db, binsData) {
  if (!db) return Promise.resolve();
  
  // Validate that all required fields are present
  if (binsData.mintBinId === undefined || binsData.mintBinId === null ||
      binsData.redemptionBinId === undefined || binsData.redemptionBinId === null ||
      binsData.mintPrice === undefined || binsData.mintPrice === null ||
      binsData.redemptionPrice === undefined || binsData.redemptionPrice === null) {
    console.error('Failed to update active bins: missing required fields', {
      mintBinId: binsData.mintBinId,
      redemptionBinId: binsData.redemptionBinId,
      mintPrice: binsData.mintPrice,
      redemptionPrice: binsData.redemptionPrice
    });
    return Promise.resolve();
  }
  
  return db.prepare(`
      INSERT INTO active_bins (id, mint_bin_id, redemption_bin_id, mint_price, redemption_price, updated_at)
      VALUES (1, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        mint_bin_id = excluded.mint_bin_id,
        redemption_bin_id = excluded.redemption_bin_id,
        mint_price = excluded.mint_price,
        redemption_price = excluded.redemption_price,
        updated_at = excluded.updated_at
    `).bind(
      binsData.mintBinId,
      binsData.redemptionBinId,
      binsData.mintPrice,
      binsData.redemptionPrice,
      Date.now()
    ).run().catch(err => {
      console.error('Failed to update active bins:', err.message);
    });
}

// Query logs endpoint helper
async function queryLogs(db, type, limit = 100, offset = 0, expand = false) {
  if (!db) return { error: 'Database not configured' };
  
  try {
    let query, results;
    
    switch (type) {
      case 'console':
        results = await db.prepare(
          'SELECT * FROM console_logs ORDER BY timestamp DESC LIMIT ? OFFSET ?'
        ).bind(limit, offset).all();
        
        // Expand batched messages if requested
        if (expand && results.results) {
          results.results = results.results.flatMap(row => {
            if (row.level === 'batch' && row.context) {
              try {
                const parsed = JSON.parse(row.context);
                return parsed.messages.map(msg => ({
                  ...row,
                  timestamp: msg.timestamp,
                  level: msg.level,
                  message: msg.message,
                  context: msg.context ? JSON.stringify(msg.context) : null,
                  _batch_id: row.request_id
                }));
              } catch (e) {
                return [row];
              }
            }
            return [row];
          });
        }
        break;
      case 'swaps':
        results = await db.prepare(
          'SELECT * FROM swap_events ORDER BY timestamp DESC LIMIT ? OFFSET ?'
        ).bind(limit, offset).all();
        break;
      case 'prices':
        results = await db.prepare(
          'SELECT * FROM price_updates ORDER BY timestamp DESC LIMIT ? OFFSET ?'
        ).bind(limit, offset).all();
        break;
      case 'rebalancing':
        results = await db.prepare(
          'SELECT * FROM rebalancing_events ORDER BY timestamp DESC LIMIT ? OFFSET ?'
        ).bind(limit, offset).all();
        break;
      case 'bins':
        results = await db.prepare('SELECT * FROM active_bins').all();
        break;
      default:
        return { error: 'Unknown log type. Use: console, swaps, prices, rebalancing, bins' };
    }
    
    return { success: true, data: results.results, meta: results.meta };
  } catch (err) {
    return { error: err.message };
  }
}

// --- CUSTOM WALLET ---
// Used as wallet adapter for AnchorProvider in Cloudflare Workers environment
class CustomWallet {
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

// ==================================================================
// CONFIGURATION
// ==================================================================

// CONSTANTS
const RESERVE_MINT_STR = "J2JAep9untmdaQXXRYB1bxT2eFNWWeR8ApuRdAiY9gni"; // devUSDT mint on Devnet
const RESERVE_SYMBOL = "devUSDT";
const POOL_ADDRESS = "HYeXEBUxLM4aFYSBmHRhMLwMP5wGDXMtEHTtx3VevkTD"; // Meteora DLMM pool for IRMA/devUSDT

// TRUFLATION CONFIGURATION
// Inflation data is fetched via our Vercel proxy (truflation-proxy)
// The proxy URL is set in wrangler.jsonc vars.TRUFLATION_PROXY_URL
// Deploy the proxy first: cd truflation-proxy && ./deploy.sh

// Target inflation rate (below this, mint price = 1.0 / quote_token_price)
const TARGET_INFLATION_RATE = 2.0;

// ==================================================================
// HELPER FUNCTIONS
// ==================================================================

/**
 * Fetch current inflation rate from our Truflation Vercel proxy
 * The proxy handles the SDK/axios complexity in a Node.js environment
 * Returns inflation rate as a percentage (e.g., 2.169 for 2.169%)
 */
async function fetchTruflationRate(env) {
  console.log("📊 Fetching inflation data from Truflation proxy...");

  const proxyUrl = env.TRUFLATION_PROXY_URL;
  if (!proxyUrl) {
    throw new Error("TRUFLATION_PROXY_URL not configured. Deploy truflation-proxy first.");
  }

  const apiUrl = `${proxyUrl}/api/inflation`;
  console.log(`📡 Querying: ${apiUrl}`);

  try {
    const res = await fetch(apiUrl, {
      method: "GET",
      headers: {
        "Accept": "application/json",
      },
    });

    if (!res.ok) {
      const errorText = await res.text();
      throw new Error(`HTTP ${res.status}: ${errorText}`);
    }

    const data = await res.json();

    if (!data.success || !data.data || typeof data.data.inflationRate !== "number") {
      throw new Error(`Invalid response from proxy: ${JSON.stringify(data)}`);
    }

    const inflationRate = data.data.inflationRate;
    console.log(`📈 Truflation US Inflation Index: ${inflationRate}%`);
    console.log(`📅 Data timestamp: ${new Date(data.data.timestamp).toISOString()}`);
    
    return inflationRate;
  } catch (error) {
    console.error("❌ Failed to fetch Truflation data:", error.message);
    throw error;
  }
}


/**
 * Calculate the IRMA mint price based on Truflation inflation rate
 * Formula:
 *   if (truflation > 2.0) {
 *     mint_price = (1.00 + (truflation - 2.0) / 100.0) / quote_token_price_in_usd;
 *   } else {
 *     mint_price = 1.00 / quote_token_price_in_usd;
 *   }
 */
function calculateMintPrice(inflationRate, quoteTokenPriceUsd) {
  let mintPrice;
  
  if (inflationRate > TARGET_INFLATION_RATE) {
    // Inflation above target: adjust mint price upward
    const inflationAdjustment = (inflationRate - TARGET_INFLATION_RATE) / 100.0;
    mintPrice = (1.00 + inflationAdjustment) / quoteTokenPriceUsd;
    console.log(`📊 Inflation ${inflationRate}% > ${TARGET_INFLATION_RATE}%: adjustment = ${inflationAdjustment}`);
  } else {
    // Inflation at or below target: mint price = 1.0 / quote token price
    mintPrice = 1.00 / quoteTokenPriceUsd;
    console.log(`📊 Inflation ${inflationRate}% <= ${TARGET_INFLATION_RATE}%: no adjustment`);
  }
  
  console.log(`💰 Calculated mint price: ${mintPrice} (quote token price: ${quoteTokenPriceUsd} USD)`);
  return mintPrice;
}

/**
 * Fetch the quote token (stablecoin) price in USD
 * For now, we assume it's ~1.0 USD for stablecoins
 * TODO: Integrate with Meteora pool or oracle for real price
 */
async function getQuoteTokenPriceUsd(connection, poolAddress) {
  // For stablecoins like USDC, the price is typically very close to $1
  // In production, you might want to:
  // 1. Query Meteora pool for the actual price
  // 2. Use an oracle like Pyth or Switchboard
  // 3. Average across multiple DEXs
  
  // For now, return 1.0 as a reasonable assumption for devUSDC
  console.log("📊 Assuming quote token price = $1.00 USD (stablecoin)");
  return 1.0;
}

/**
 * Update the IRMA mint price on-chain using Truflation data
 */
async function updateMintPriceFromTruflation(env) {
  console.log("🔄 Starting mint price update from Truflation...");
  
  const HELIUS_API_KEY = env.HELIUS_API_KEY;
  const HELIUS_RPC_URL = `https://devnet.helius-rpc.com/?api-key=${HELIUS_API_KEY}`;

  try {
    // 1. Fetch inflation rate from Truflation
    const inflationRate = await fetchTruflationRate(env);
    
    // 2. Setup Solana connection and program
    const secretString = env.ADMIN_PRIVATE_KEY;
    const secretKey = new Uint8Array(JSON.parse(secretString));
    const connection = new Connection(HELIUS_RPC_URL, "confirmed");
    const adminKeypair = Keypair.fromSecretKey(secretKey);
    
    const wallet = new CustomWallet(adminKeypair);
    const provider = new AnchorProvider(connection, wallet, { commitment: "confirmed" });
    const program = new Program(IDL, provider);
    
    // Derive PDAs
    const programId = new PublicKey(IDL.address);
    const [statePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("state_v5")],
      programId
    );
    const [corePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("core_v5")],
      programId
    );
    
    // 3. Get quote token price (USD price of the reserve stablecoin)
    const quoteTokenPriceUsd = await getQuoteTokenPriceUsd(connection, POOL_ADDRESS);
    
    // 4. Calculate new mint price
    const newMintPrice = calculateMintPrice(inflationRate, quoteTokenPriceUsd);
    
    // 5. Convert to the format expected by the on-chain program (f64 as fixed-point)
    // The program expects price as f64, we'll pass it directly
    const priceAsNumber = newMintPrice;
    
    console.log(`📝 Setting mint price for ${RESERVE_SYMBOL} to ${priceAsNumber}...`);
    
    // 6. Call setMintPrice on the IRMA program
    const txInstruction = await program.methods
      .setMintPrice(RESERVE_SYMBOL, priceAsNumber)
      .accounts({
        state: statePda,
        irmaAdmin: wallet.publicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .transaction();
    
    txInstruction.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
    txInstruction.feePayer = adminKeypair.publicKey;
    txInstruction.sign(adminKeypair);
    
    const tx = await connection.sendRawTransaction(txInstruction.serialize(), { 
      skipPreflight: false,
      maxRetries: 0
    });
    console.log(`✅ Mint price updated! Transaction: ${tx}`);
    
    return {
      success: true,
      inflationRate,
      quoteTokenPriceUsd,
      newMintPrice,
      transaction: tx,
    };
  } catch (error) {
    console.error("❌ Failed to update mint price:", error.message);
    throw error;
  }
}

/**
 * Get both mint and redemption prices from IRMA program
 */
/**
 * Get both mint and redemption prices from IRMA program
 */
async function getPrices(program, statePda, corePda, adminPublicKey, quoteToken) {
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

// ==================================================================
// REBALANCING FUNCTIONS
// ==================================================================

/**
 * Setup common Solana connection, wallet, and program
 */
async function setupSolanaConnection(env) {
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
    await logger.error(`❌ Failed to remove liquidity from bin ${binId}: ${err.message}`);
    throw err;
  }
  
  return { xAmount: new BN(0), yAmount: new BN(0), signature: null };
}

/**
 * Close an empty position
 */
async function closePosition(dlmmPool, connection, adminKeypair, position, logger) {
  await logger.log(`🗑️ Closing position ${position.publicKey.toBase58()}...`);
  
  try {
    const closePositionTx = await dlmmPool.closePosition({
      position: position.publicKey,
      owner: adminKeypair.publicKey,
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

/**
 * Rebalance mint bin - move all IRMA from old mint bin to new mint bin
 */
async function rebalanceMintBin(env, oldMintBinId, newMintBinId, dlmmPool, connection, adminKeypair, userPositions, logger) {
  await logger.log(`🔄 Rebalancing MINT bin: ${oldMintBinId} → ${newMintBinId}`);
  
  let totalIrmaRemoved = new BN(0);
  let removeSig = null;
  let addSig = null;
  let closeSig = null;
  
  // Find positions with liquidity in old mint bin
  for (const pos of userPositions) {
    if (pos.positionData.lowerBinId <= oldMintBinId && pos.positionData.upperBinId >= oldMintBinId) {
      const binData = pos.positionData.positionBinData.find(b => b.binId === oldMintBinId);
      if (binData && (binData.positionXAmount > 0 || binData.positionYAmount > 0)) {
        await logger.log(`📍 Found liquidity in position ${pos.publicKey.toBase58()} at bin ${oldMintBinId}`);
        
        const { xAmount, yAmount, signature } = await removeLiquidityFromBin(
          dlmmPool, connection, adminKeypair, pos, oldMintBinId, logger
        );
        
        if (signature) removeSig = signature;
        totalIrmaRemoved = totalIrmaRemoved.add(xAmount);
        
        // Check if position is now empty and should be closed
        const remainingBins = pos.positionData.positionBinData.filter(
          b => b.binId !== oldMintBinId && (b.positionXAmount > 0 || b.positionYAmount > 0)
        );
        if (remainingBins.length === 0) {
          closeSig = await closePosition(dlmmPool, connection, adminKeypair, pos, logger);
        }
      }
    }
  }
  
  // Add IRMA to new mint bin
  if (totalIrmaRemoved.gt(new BN(0))) {
    await logger.log(`📦 Total IRMA removed: ${totalIrmaRemoved.toString()}`);
    
    // Refresh positions after removal
    const { userPositions: refreshedPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
    
    const result = await addLiquidityToBin(
      dlmmPool, connection, adminKeypair, refreshedPositions, 
      newMintBinId, totalIrmaRemoved, new BN(0), logger
    );
    addSig = result.signature;
  } else {
    await logger.log(`ℹ️ No IRMA liquidity found in old mint bin ${oldMintBinId}`);
  }
  
  return {
    irmaAmountMoved: totalIrmaRemoved.toString(),
    removeLiquiditySignature: removeSig,
    addLiquiditySignature: addSig,
    closePositionSignature: closeSig,
  };
}

/**
 * Rebalance redemption bin - move all USDC from old redemption bin to new redemption bin
 */
async function rebalanceRedemptionBin(env, oldRedemptionBinId, newRedemptionBinId, dlmmPool, connection, adminKeypair, userPositions, logger) {
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
  
  // Add USDC to new redemption bin
  if (totalUsdcRemoved.gt(new BN(0))) {
    await logger.log(`📦 Total USDC removed: ${totalUsdcRemoved.toString()}`);
    
    // Refresh positions after removal
    const { userPositions: refreshedPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
    
    const result = await addLiquidityToBin(
      dlmmPool, connection, adminKeypair, refreshedPositions, 
      newRedemptionBinId, new BN(0), totalUsdcRemoved, logger
    );
    addSig = result.signature;
  } else {
    await logger.log(`ℹ️ No USDC liquidity found in old redemption bin ${oldRedemptionBinId}`);
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
async function checkAndRebalanceBins(env, newMintPrice, newRedemptionPrice, triggerType = 'auto') {
  const logger = new Logger(env.irma_logs);
  
  try {
    await logger.log(`🔍 Checking if bin rebalancing is needed...`);
    
    const { connection, adminKeypair } = await setupSolanaConnection(env);
    
    // Initialize DLMM pool
    const poolKey = new PublicKey(POOL_ADDRESS);
    const dlmmPool = await DLMM.create(connection, poolKey);
    
    // Calculate new bin IDs from prices
    const newMintBinId = dlmmPool.getBinIdFromPrice(newMintPrice.toString(), true);
    const newRedemptionBinId = dlmmPool.getBinIdFromPrice(newRedemptionPrice.toString(), false);
    
    await logger.log(`📊 New prices → Mint Bin: ${newMintBinId}, Redemption Bin: ${newRedemptionBinId}`);
    
    // Get stored active bins
    const activeBins = await getActiveBins(env.irma_logs);
    
    if (!activeBins) {
      await logger.log(`ℹ️ No active bins stored yet, initializing...`);
      await updateActiveBins(env.irma_logs, {
        mintBinId: newMintBinId,
        redemptionBinId: newRedemptionBinId,
        mintPrice: newMintPrice,
        redemptionPrice: newRedemptionPrice,
      });
      await logger.flush();
      return { success: true, message: 'Active bins initialized', rebalanced: false };
    }
    
    const oldMintBinId = activeBins.mint_bin_id;
    const oldRedemptionBinId = activeBins.redemption_bin_id;
    
    await logger.log(`📊 Stored bins → Mint Bin: ${oldMintBinId}, Redemption Bin: ${oldRedemptionBinId}`);
    
    const mintBinChanged = Math.abs(newMintBinId - oldMintBinId) >= 1;
    const redemptionBinChanged = Math.abs(newRedemptionBinId - oldRedemptionBinId) >= 1;
    
    if (!mintBinChanged && !redemptionBinChanged) {
      await logger.log(`✅ Bins are in sync, no rebalancing needed`);
      await logger.flush();
      return { success: true, message: 'Bins in sync', rebalanced: false };
    }
    
    // Get user positions
    const { userPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
    await logger.log(`📍 Found ${userPositions.length} position(s) to check`);
    
    let mintRebalanceResult = null;
    let redemptionRebalanceResult = null;
    
    // Rebalance mint bin if changed
    if (mintBinChanged) {
      await logger.log(`🔄 Mint bin changed: ${oldMintBinId} → ${newMintBinId}`);
      try {
        mintRebalanceResult = await rebalanceMintBin(
          env, oldMintBinId, newMintBinId, 
          dlmmPool, connection, adminKeypair, userPositions, logger
        );
        
        logRebalancingEvent(env.irma_logs, {
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
        await logger.error(`❌ Mint bin rebalancing failed: ${err.message}`);
        console.error(`❌ Mint bin rebalancing failed: ${err.message}`);
        
        logRebalancingEvent(env.irma_logs, {
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
      await logger.log(`🔄 Redemption bin changed: ${oldRedemptionBinId} → ${newRedemptionBinId}`);
      
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
        
        logRebalancingEvent(env.irma_logs, {
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
        await logger.error(`❌ Redemption bin rebalancing failed: ${err.message}`);
        console.error(`❌ Redemption bin rebalancing failed: ${err.message}`);
        
        logRebalancingEvent(env.irma_logs, {
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
    await updateActiveBins(env.irma_logs, {
      mintBinId: newMintBinId,
      redemptionBinId: newRedemptionBinId,
      mintPrice: newMintPrice,
      redemptionPrice: newRedemptionPrice,
    });
    
    await logger.log(`✅ Rebalancing complete, active bins updated`);
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
    await logger.error(`❌ Bin rebalancing check failed: ${err.message}`);
    console.error(`❌ Bin rebalancing check failed: ${err.message}`);
    await logger.flush();
    throw err;
  }
}

/**
 * Manual rebalancing endpoint - forces rebalance based on current prices
 */
async function manualRebalanceBins(env) {
  const logger = new Logger(env.irma_logs);
  
  try {
    await logger.log(`🔧 Manual rebalancing triggered...`);
    
    const { connection, adminKeypair, wallet, provider, program, statePda, corePda } = await setupSolanaConnection(env);
    
    // Get current prices from IRMA program
    const prices = await getPrices(program, statePda, corePda, adminKeypair.publicKey, RESERVE_SYMBOL);
    await logger.log(`📊 Current prices - Mint: ${prices.mintPrice}, Redemption: ${prices.redemptionPrice}`);
    
    // Force rebalance by temporarily clearing active bins
    const poolKey = new PublicKey(POOL_ADDRESS);
    const dlmmPool = await DLMM.create(connection, poolKey);
    
    const newMintBinId = dlmmPool.getBinIdFromPrice(prices.mintPrice.toString(), true);
    const newRedemptionBinId = dlmmPool.getBinIdFromPrice(prices.redemptionPrice.toString(), false);
    
    // Get stored bins
    const activeBins = await getActiveBins(env.irma_logs);
    
    if (!activeBins) {
      // No bins stored, just initialize
      await updateActiveBins(env.irma_logs, {
        mintBinId: newMintBinId,
        redemptionBinId: newRedemptionBinId,
        mintPrice: prices.mintPrice,
        redemptionPrice: prices.redemptionPrice,
      });
      await logger.log(`ℹ️ Active bins initialized (first time)`);
      await logger.flush();
      return { success: true, message: 'Active bins initialized', rebalanced: false };
    }
    
    const oldMintBinId = activeBins.mint_bin_id;
    const oldRedemptionBinId = activeBins.redemption_bin_id;
    
    // Get user positions
    const { userPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
    
    let results = { mintRebalanced: false, redemptionRebalanced: false };
    
    // Always attempt rebalancing if bins differ
    if (oldMintBinId !== newMintBinId) {
      try {
        const mintResult = await rebalanceMintBin(
          env, oldMintBinId, newMintBinId, 
          dlmmPool, connection, adminKeypair, userPositions, logger
        );
        results.mintRebalanced = true;
        results.mintResult = mintResult;
        
        logRebalancingEvent(env.irma_logs, {
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
        await logger.error(`❌ Manual mint bin rebalancing failed: ${err.message}`);
        console.error(`❌ Manual mint bin rebalancing failed: ${err.message}`);
      }
    }
    
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
        
        logRebalancingEvent(env.irma_logs, {
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
        await logger.error(`❌ Manual redemption bin rebalancing failed: ${err.message}`);
        console.error(`❌ Manual redemption bin rebalancing failed: ${err.message}`);
      }
    }
    
    // Update active bins
    await updateActiveBins(env.irma_logs, {
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
    await logger.error(`❌ Manual rebalancing failed: ${err.message}`);
    console.error(`❌ Manual rebalancing failed: ${err.message}`);
    await logger.flush();
    throw err;
  }
}

// ==================================================================
// WORKER LOGIC
// ==================================================================

export default {
  // Handle HTTP requests (webhooks from Helius, manual triggers, etc.)
  async fetch(request, env, ctx) {
    return handleRequest(request, env, ctx);
  },
  
  // Handle scheduled triggers (cron jobs) - runs once daily to update mint price
  async scheduled(event, env, ctx) {
    console.log("⏰ Scheduled trigger: Updating mint price from Truflation...");
    ctx.waitUntil(handleScheduledMintPriceUpdate(env));
  }
};

/**
 * Handle scheduled mint price update (called by cron trigger)
 * Also triggers automatic bin rebalancing after price update
 */
async function handleScheduledMintPriceUpdate(env) {
  const logger = new Logger(env.irma_logs);
  
  try {
    await logger.log("⏰ Scheduled trigger: Updating mint price from Truflation...");
    
    const result = await updateMintPriceFromTruflation(env);
    await logger.log(`✅ Scheduled mint price update completed: ${JSON.stringify(result)}`);
    
    // Log price update to D1
    await logPriceUpdate(env.irma_logs, {
      inflationRate: result.inflationRate,
      quoteTokenPriceUsd: result.quoteTokenPriceUsd,
      newMintPrice: result.newMintPrice,
      txSignature: result.transaction,
      triggerType: 'scheduled',
      success: true,
    });
    
    // After price update, check and rebalance bins if needed
    if (result.success) {
      await logger.log("🔄 Checking bin synchronization after price update...");
      
      try {
        // Get current prices from the program to check both mint and redemption
        const { connection, adminKeypair, program, statePda, corePda } = await setupSolanaConnection(env);
        const prices = await getPrices(program, statePda, corePda, adminKeypair.publicKey, RESERVE_SYMBOL);
        
        const rebalanceResult = await checkAndRebalanceBins(
          env, 
          prices.mintPrice, 
          prices.redemptionPrice, 
          'auto'
        );
        
        await logger.log(`✅ Bin rebalancing check complete: ${JSON.stringify(rebalanceResult)}`);
      } catch (rebalanceError) {
        await logger.error(`❌ Bin rebalancing after price update failed: ${rebalanceError.message}`);
        console.error(`❌ Bin rebalancing after price update failed: ${rebalanceError.message}`);
      }
    }
    
    await logger.flush();
  } catch (error) {
    await logger.error(`❌ Scheduled mint price update failed: ${error.message}`);
    console.error("❌ Scheduled mint price update failed:", error.message);
    
    await logPriceUpdate(env.irma_logs, {
      inflationRate: 0,
      quoteTokenPriceUsd: 0,
      newMintPrice: 0,
      triggerType: 'scheduled',
      success: false,
      errorMessage: error.message,
    });
    
    await logger.flush();
  }
}

async function handleRequest(request, env, ctx) {
  const url = new URL(request.url);
  
  // ============================================================
  // ACTION ENDPOINTS - Triggered via query parameter or GET request
  // ============================================================
  
  // GET /update-mint-price - Manually trigger mint price update
  // Also supports: GET /?action=update-mint-price
  if (request.method === 'GET') {
    const action = url.searchParams.get('action') || url.pathname.slice(1);
    
    if (action === 'update-mint-price') {
      console.log("🔧 Manual trigger: Update mint price from Truflation");
      try {
        const result = await updateMintPriceFromTruflation(env);
        return new Response(JSON.stringify({
          success: true,
          message: "Mint price updated successfully",
          ...result
        }), {
          status: 200,
          headers: { "Content-Type": "application/json" }
        });
      } catch (error) {
        return new Response(JSON.stringify({
          success: false,
          error: error.message
        }), {
          status: 500,
          headers: { "Content-Type": "application/json" }
        });
      }
    }
    
    if (action === 'fetch-inflation') {
      console.log("🔧 Manual trigger: Fetch Truflation inflation rate (test)");
      try {
        const inflationRate = await fetchTruflationRate(env);
        return new Response(JSON.stringify({
          success: true,
          inflationRate,
          message: `Current inflation rate: ${inflationRate}%`
        }), {
          status: 200,
          headers: { "Content-Type": "application/json" }
        });
      } catch (error) {
        return new Response(JSON.stringify({
          success: false,
          error: error.message
        }), {
          status: 500,
          headers: { "Content-Type": "application/json" }
        });
      }
    }
    
    // Manual bin rebalancing endpoint
    if (action === 'rebalance-bins') {
      console.log("🔧 Manual trigger: Rebalance bins");
      try {
        const result = await manualRebalanceBins(env);
        return new Response(JSON.stringify({
          success: true,
          message: "Bin rebalancing completed",
          ...result
        }), {
          status: 200,
          headers: { "Content-Type": "application/json" }
        });
      } catch (error) {
        return new Response(JSON.stringify({
          success: false,
          error: error.message
        }), {
          status: 500,
          headers: { "Content-Type": "application/json" }
        });
      }
    }
    
    // View logs endpoint
    if (action === 'view-logs') {
      const logType = url.searchParams.get('type') || 'console';
      const limit = parseInt(url.searchParams.get('limit') || '100', 10);
      const offset = parseInt(url.searchParams.get('offset') || '0', 10);
      const expand = url.searchParams.get('expand') === 'true';
      
      try {
        const result = await queryLogs(env.irma_logs, logType, limit, offset, expand);
        return new Response(JSON.stringify(result), {
          status: result.error ? 400 : 200,
          headers: { "Content-Type": "application/json" }
        });
      } catch (error) {
        return new Response(JSON.stringify({
          success: false,
          error: error.message
        }), {
          status: 500,
          headers: { "Content-Type": "application/json" }
        });
      }
    }
    
    // View current active bins
    if (action === 'view-bins') {
      try {
        const activeBins = await getActiveBins(env.irma_logs);
        if (!activeBins) {
          return new Response(JSON.stringify({
            success: true,
            message: "No active bins stored yet",
            data: null
          }), {
            status: 200,
            headers: { "Content-Type": "application/json" }
          });
        }
        return new Response(JSON.stringify({
          success: true,
          data: activeBins
        }), {
          status: 200,
          headers: { "Content-Type": "application/json" }
        });
      } catch (error) {
        return new Response(JSON.stringify({
          success: false,
          error: error.message
        }), {
          status: 500,
          headers: { "Content-Type": "application/json" }
        });
      }
    }
    
    if (action === 'health' || action === '') {
      return new Response(JSON.stringify({
        status: "ok",
        endpoints: [
          "GET /?action=health - Health check", 
          "GET /?action=fetch-inflation - Test fetching Truflation inflation rate",
          "GET /?action=update-mint-price - Update mint price from Truflation",
          "GET /?action=rebalance-bins - Manual bin rebalancing",
          "GET /?action=view-bins - View current active bins",
          "GET /?action=view-logs&type=console|swaps|prices|rebalancing|bins&limit=100&offset=0&expand=true - Query logs",
          "POST / - Helius webhook for swap events"
        ]
      }), {
        status: 200,
        headers: { "Content-Type": "application/json" }
      });
    }
    
    return new Response("Unknown action", { status: 400 });
  }
  
  // ============================================================
  // POST ENDPOINT - Helius webhook for swap events
  // ============================================================
  if (request.method !== 'POST') {
    return new Response("Method not allowed", { status: 405 });
  }

  try {
    const data = await request.json();
    const tx = data[0]; 

    // --- BASIC CHECKS ---
    if (!tx) return new Response("Ignored", { status: 200 });

    console.log(`🔔 tx.meta: ${tx.meta}`);
    // --- LOOP PREVENTION ---
    // Check for our specific Memo tag in the logs
    const logs = tx.meta.logMessages || [];
    const isWorkerSwap = logs.some(log => log.includes(WORKER_MEMO_STRING));
    if (isWorkerSwap) {
      console.log("🚫 Ignored Worker-Initiated Swap (Loop Prevention)");
      return new Response("Ignored (Worker Swap)", { status: 200 });
    }

    console.log(`🔍 Is there an error? ${tx.meta.err}`);
    if (!tx.meta || tx.meta.err !== null) return new Response("Ignored", { status: 200 });

    const isSwapInstruction = logs.some(log => 
      log.includes("Instruction: Swap2") || log.includes("Instruction: Swap")
    );
    if (!isSwapInstruction) return new Response("Ignored (Not a Swap)", { status: 200 });

    // Use waitUntil to allow processing to complete after response
    ctx.waitUntil(processRebalance(tx, env, ctx));
    
    return new Response("Processing started", { status: 200 });

  } catch (err) {
    console.error("Worker Error:", err);
    return new Response("Error handled", { status: 200 });
  }
}

async function processRebalance(tx, env, ctx) {
  const logger = new Logger(env.irma_logs);
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
        await logger.log("Ignored (No Pool Balance)");
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
      
      await logger.log(`🚨 TRIGGER: ${delta > 0 ? "MINT" : "REDEMPTION"} Detected. Delta: ${delta}`);

      // --- SETUP ---
      const secretString = env.ADMIN_PRIVATE_KEY;
      const secretKey = new Uint8Array(JSON.parse(secretString));
      const connection = new Connection(HELIUS_RPC_URL, "confirmed");
      const adminKeypair = Keypair.fromSecretKey(secretKey);
      await logger.log(`🔑 Admin Public Key: ${adminKeypair.publicKey.toBase58()}`);
      
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
        await logger.error(`❌ Failed to get prices: ${priceError.message}`);
        await logger.error(`Stack: ${priceError.stack}`);
        throw priceError;
      }

      // --- INITIALIZE DLMM POOL ---
      await logger.log(`📊 Initializing DLMM pool...`);
      const poolKey = new PublicKey(POOL_ADDRESS);
      let dlmmPool;
      try {
        dlmmPool = await DLMM.create(connection, poolKey);
        await logger.log(`✅ DLMM pool initialized`);
      } catch (dlmmError) {
        await logger.error(`❌ Failed to initialize DLMM: ${dlmmError.message}`);
        throw dlmmError;
      }

      // --- CONVERT PRICES TO BIN IDs ---
      const mintBinId = dlmmPool.getBinIdFromPrice(mintPrice.toString(), true);
      const redemptionBinId = dlmmPool.getBinIdFromPrice(redemptionPrice.toString(), false);
      const binDistance = Math.abs(mintBinId - redemptionBinId);
      
      swapEventData.mintBinId = mintBinId;
      swapEventData.redemptionBinId = redemptionBinId;
      
      await logger.log(`📍 Mint Bin: ${mintBinId}, Redemption Bin: ${redemptionBinId}, Distance: ${binDistance}`);

      // Check if rebalancing is needed (skip if same bin or adjacent bins)
      if (binDistance < 2) {
        await logger.log(`⏭️ Bins too close (distance: ${binDistance}), skipping rebalancing`);
        // Still call trade event but no counter-swap
        if (delta > 0) {
          await logger.log(`📝 Building saleTradeEvent transaction...`);
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
          
          await logger.log(`📝 Sending saleTradeEvent transaction...`);
          tradeTxInstruction.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
          tradeTxInstruction.feePayer = adminKeypair.publicKey;
          tradeTxInstruction.sign(adminKeypair);
          
          const tradeTx = await connection.sendRawTransaction(tradeTxInstruction.serialize(), { 
            skipPreflight: false,
            maxRetries: 0 // Don't wait for confirmation
          });
          await logger.log(`✅ Sale trade event sent (no rebalancing): ${tradeTx}`);
          swapEventData.txSignature = tradeTx;
        } else {
          await logger.log(`📝 Building buyTradeEvent transaction...`);
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
          
          await logger.log(`📝 Sending buyTradeEvent transaction...`);
          tradeTxInstruction.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
          tradeTxInstruction.feePayer = adminKeypair.publicKey;
          tradeTxInstruction.sign(adminKeypair);
          
          const tradeTx = await connection.sendRawTransaction(tradeTxInstruction.serialize(), { 
            skipPreflight: false,
            maxRetries: 0 // Don't wait for confirmation
          });
          await logger.log(`✅ Buy trade event sent (no rebalancing): ${tradeTx}`);
          swapEventData.txSignature = tradeTx;
        }
        
        await logger.log(`📝 Trade event complete, preparing to log and return`);
        swapEventData.success = true;
        
        // Use waitUntil for logging operations to ensure they complete
        if (ctx) {
          ctx.waitUntil(Promise.all([
            logSwapEvent(env.irma_logs, swapEventData),
            logger.flush()
          ]));
        } else {
          logSwapEvent(env.irma_logs, swapEventData);
          logger.flush();
        }
        await logger.log(`📝 Returning from early exit`);
        return;
      }

      // --- GET EXISTING POSITIONS ---
      const { userPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
      await logger.log(`📍 Found ${userPositions.length} position(s)`);
      
      for (const pos of userPositions) {
        await logger.log(`   Position ${pos.publicKey.toBase58()}: bins ${pos.positionData.lowerBinId} to ${pos.positionData.upperBinId}`);
      }
      
      // --- GET STORED ACTIVE BINS FOR REBALANCING ---
      const storedActiveBins = await getActiveBins(env.irma_logs);

      // =========================================================
      // LOGIC: MINT EVENT (User bought IRMA with USDC)
      // =========================================================
      if (delta > 0) {
        await logger.log(`👉 Step 1: Performing counter-swap (IRMA -> USDC)...`);

        const swapForY = true; 
        
        await logger.log("DEBUG: Fetching bin arrays...");
        const binArrays = await dlmmPool.getBinArrayForSwap(swapForY);
        
        await logger.log("DEBUG: Getting swap quote...");
        const irmaSwapAmount = amountAtomic.mul(new BN(95)).div(new BN(100));
        await logger.log(`💰 User delta: ${amountAtomic.toString()}, Counter-swap IRMA amount: ${irmaSwapAmount.toString()}`);
        
        const swapQuote = await dlmmPool.swapQuote(
          irmaSwapAmount, 
          swapForY,
          new BN(1500),
          binArrays
        );
        
        const usdcOutputAmount = swapQuote.minOutAmount;
        await logger.log(`💰 Expected USDC output: ${usdcOutputAmount.toString()}`);

        await logger.log("DEBUG: Creating swap transaction...");
        const swapTx = await dlmmPool.swap({
          inToken: dlmmPool.tokenX.publicKey,
          outToken: dlmmPool.tokenY.publicKey,
          inAmount: irmaSwapAmount,
          binArraysPubkey: swapQuote.binArraysPubkey,
          lbPair: poolKey,
          user: adminKeypair.publicKey,
          minOutAmount: usdcOutputAmount,
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

        await logger.log(`👉 Step 2: Adding USDC to redemption bin ${redemptionBinId}...`);

        // Check if redemption bin has changed and we need to rebalance
        const oldRedemptionBinId = storedActiveBins?.redemption_bin_id;
        let totalUsdcToAdd = usdcOutputAmount;
        
        if (oldRedemptionBinId && oldRedemptionBinId !== redemptionBinId) {
          await logger.log(`🔄 Redemption bin changed: ${oldRedemptionBinId} → ${redemptionBinId}, rebalancing...`);
          
          // Remove liquidity from old redemption bin first
          try {
            const rebalanceResult = await rebalanceRedemptionBin(
              env, oldRedemptionBinId, redemptionBinId,
              dlmmPool, connection, adminKeypair, userPositions, logger
            );
            
            // Add the recovered USDC to our total
            if (rebalanceResult.usdcAmountMoved && rebalanceResult.usdcAmountMoved !== '0') {
              totalUsdcToAdd = totalUsdcToAdd.add(new BN(rebalanceResult.usdcAmountMoved));
              await logger.log(`📦 Added ${rebalanceResult.usdcAmountMoved} USDC from old bin to deposit`);
            }
            
            logRebalancingEvent(env.irma_logs, {
              rebalanceType: 'redemption_bin',
              oldRedemptionBinId,
              newRedemptionBinId: redemptionBinId,
              usdcAmountMoved: rebalanceResult.usdcAmountMoved,
              removeLiquiditySignature: rebalanceResult.removeLiquiditySignature,
              triggerType: 'auto_swap',
              success: true,
            });
          } catch (rebalanceErr) {
            await logger.error(`❌ Redemption bin rebalancing failed: ${rebalanceErr.message}`);
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
            totalYAmount: totalUsdcToAdd,
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
            totalYAmount: totalUsdcToAdd,
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
        // LOGIC: REDEMPTION EVENT (User sold IRMA for USDC)
        // =========================================================
        await logger.log(`👉 Step 1: Performing counter-swap (USDC -> IRMA)...`);

        const swapForY = false; 
        
        await logger.log("DEBUG: Fetching bin arrays...");
        const binArrays = await dlmmPool.getBinArrayForSwap(swapForY);
        
        await logger.log("DEBUG: Getting swap quote...");
        const usdcSwapAmount = amountAtomic.abs().mul(new BN(95)).div(new BN(100));
        await logger.log(`💰 User delta: ${amountAtomic.toString()}, Counter-swap USDC amount: ${usdcSwapAmount.toString()}`);
        
        const swapQuote = await dlmmPool.swapQuote(
          usdcSwapAmount,
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
          inAmount: usdcSwapAmount,
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
        const oldMintBinId = storedActiveBins?.mint_bin_id;
        let totalIrmaToAdd = irmaOutputAmount;
        
        if (oldMintBinId && oldMintBinId !== mintBinId) {
          await logger.log(`🔄 Mint bin changed: ${oldMintBinId} → ${mintBinId}, rebalancing...`);
          
          // Remove liquidity from old mint bin first
          try {
            const rebalanceResult = await rebalanceMintBin(
              env, oldMintBinId, mintBinId,
              dlmmPool, connection, adminKeypair, userPositions, logger
            );
            
            // Add the recovered IRMA to our total
            if (rebalanceResult.irmaAmountMoved && rebalanceResult.irmaAmountMoved !== '0') {
              totalIrmaToAdd = totalIrmaToAdd.add(new BN(rebalanceResult.irmaAmountMoved));
              await logger.log(`📦 Added ${rebalanceResult.irmaAmountMoved} IRMA from old bin to deposit`);
            }
            
            logRebalancingEvent(env.irma_logs, {
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
      updateActiveBins(env.irma_logs, {
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
        logSwapEvent(env.irma_logs, swapEventData),
        logger.flush()
      ]));
    } else {
      logSwapEvent(env.irma_logs, swapEventData);
      logger.flush();
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
        logSwapEvent(env.irma_logs, swapEventData),
        logger.flush()
      ]));
    } else {
      logSwapEvent(env.irma_logs, swapEventData);
      logger.flush();
    }
  }
}