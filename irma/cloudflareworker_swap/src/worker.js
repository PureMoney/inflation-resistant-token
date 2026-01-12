// 1. IMPORTS
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { Program, AnchorProvider } from "@coral-xyz/anchor";
import { Buffer } from "buffer";
import IDL from "../../target/idl/irma.json";
import { processRebalance } from "./process_rebalance.js";
import { Logger, logPriceUpdate, queryLogs, getActiveBins } from "./d1_logs.js";
import { 
  CustomWallet, 
  getPrices, 
  setupSolanaConnection, 
  checkAndRebalanceBins, 
  manualRebalanceBins } from "./dlmm.js";
import { POOL_ADDRESS, RESERVE_SYMBOL, TARGET_INFLATION_RATE, ENABLE_TEST_SCAFFOLDING } from "./config.js";

const WORKER_MEMO_STRING = "IRMA_WORKER_SWAP";

// ==================================================================
// HELPER FUNCTIONS
// ==================================================================

/**
 * Fetch current inflation rate from our Truflation Vercel proxy
 * The proxy handles the SDK/axios complexity in a Node.js environment
 * Returns inflation rate as a percentage (e.g., 2.169 for 2.169%)
 */
async function fetchTruflationRate(env) {
  console.log(`📊 Fetching inflation data from ${env.TRUFLATION_PROXY_URL}...`);

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
  // For stablecoins like USDC or USDT, the price is typically very close to $1
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
  const logger = new Logger(env.DB);
  
  try {
    await logger.log("⏰ Scheduled trigger: Updating mint price from Truflation...");
    
    const result = await updateMintPriceFromTruflation(env);
    await logger.log(`✅ Scheduled mint price update completed: ${JSON.stringify(result)}`);
    
    // Log price update to D1
    await logPriceUpdate(env.DB, {
      inflationRate: result.inflationRate,
      quoteTokenPriceUsd: result.quoteTokenPriceUsd,
      newMintPrice: result.newMintPrice,
      txSignature: result.transaction,
      triggerType: 'scheduled',
      success: true,
    });
    
    // After price update, check and rebalance bins if needed
    if (result.success) {
      logger.log("🔄 Checking bin synchronization after price update...");
      
      try {
        // Get current prices from the program to check both mint and redemption
        const { adminKeypair, program, statePda, corePda } = await setupSolanaConnection(env);
        const prices = await getPrices(program, statePda, corePda, adminKeypair.publicKey, RESERVE_SYMBOL);
        
        const rebalanceResult = await checkAndRebalanceBins(
          env, 
          prices.mintPrice, 
          prices.redemptionPrice, 
          'auto'
        );
        
        logger.log(`✅ Bin rebalancing check complete: ${JSON.stringify(rebalanceResult)}`);
      } catch (rebalanceError) {
        logger.error(`❌ Bin rebalancing after price update failed: ${rebalanceError.message}`);
        console.error(`❌ Bin rebalancing after price update failed: ${rebalanceError.message}`);
      }
    }
    
    await logger.flush();
  } catch (error) {
    logger.error(`❌ Scheduled mint price update failed: ${error.message}`);
    console.error("❌ Scheduled mint price update failed:", error.message);
    
    await logPriceUpdate(env.DB, {
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
    if (ENABLE_TEST_SCAFFOLDING !== true) {
      return new Response("Not Found", { status: 404 });
    }
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
        const result = await queryLogs(env.DB, logType, limit, offset, expand);
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
    
    // View current active bins as stored in D1
    if (action === 'view-bins') {
      try {
        const activeBins = await getActiveBins(env.DB);
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

    console.log(`🔔 tx.meta.logMessages: ${JSON.stringify(tx.meta.logMessages)}`);
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
