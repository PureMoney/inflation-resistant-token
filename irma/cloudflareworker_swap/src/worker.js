// IRMA Cloudflare Worker - Compatible with Workers Runtime
import {  SystemProgram } from "@solana/web3.js";

import { Logger, logPriceUpdate, queryLogs, getActiveBins } from "./d1_logs.js";
import { 
  processRebalance, 
  setupSolanaConnection, 
  check_shift_price_range_worker, 
  WORKER_MEMO_STRING 
} from "./process_rebalance.js";
import { getPrices } from "./dlmm.js";
// Pool/token configuration comes from Worker environment variables — see config.js for the full list.

import { PythLazerClient } from "@pythnetwork/pyth-lazer-sdk";

/**
 * Target inflation rate (%) below which the mint price is not adjusted.
 * Read from env so each pool environment can tune it; defaults to 2.0.
 */
function get_target_inflation_rate(env) {
  const parsed = parseFloat(env.TARGET_INFLATION_RATE);
  return Number.isFinite(parsed) ? parsed : 2.0;
}

/**
 * Fetch current inflation rate from Truflation Vercel proxy
 * Returns inflation rate as percentage (e.g., 2.169 for 2.169%)
 */
async function fetch_truflation_rate(env) {
  console.log(`📊 Fetching inflation data from ${env.TRUFLATION_PROXY_URL}...`);

  const proxy_url = env.TRUFLATION_PROXY_URL;
  if (!proxy_url) {
    throw new Error("TRUFLATION_PROXY_URL not configured. Deploy truflation-proxy first.");
  }

  const api_url = `${proxy_url}/api/inflation`;
  console.log(`📡 Querying: ${api_url}`);

  try {
    const res = await fetch(api_url, {
      method: "GET",
      headers: {
        "Accept": "application/json",
      },
    });

    if (!res.ok) {
      const error_text = await res.text();
      throw new Error(`HTTP ${res.status}: ${error_text}`);
    }

    const data = await res.json();

    if (!data.success || !data.data || typeof data.data.inflationRate !== "number") {
      throw new Error(`Invalid response from proxy: ${JSON.stringify(data)}`);
    }

    const inflation_rate = data.data.inflationRate;
    console.log(`📈 Truflation US Inflation Index: ${inflation_rate}%`);
    console.log(`📅 Data timestamp: ${new Date(data.data.timestamp).toISOString()}`);
    
    return inflation_rate;
  } catch (error) {
    console.error("❌ Failed to fetch Truflation data:", error.message);
    throw error;
  }
}

/**
 * Calculate IRMA mint price based on Truflation inflation rate
 * Formula: if inflation > target, adjust upward; otherwise use 1.0
 */
function calculate_mint_price(inflation_rate, latest_price, yesterday_price, current_mint_price) {
  let mint_price;
  const target_inflation_rate = get_target_inflation_rate(process.env);

  if (inflation_rate > target_inflation_rate) {
    // Inflation above target: adjust mint price upward for a single day
    let inflation_adjustment = (inflation_rate - target_inflation_rate) / 36525.0;
    mint_price = current_mint_price * (1.00 + inflation_adjustment) * yesterday_price / latest_price;
    console.log(`📊 Inflation ${inflation_rate}% > ${target_inflation_rate}%: adjustment = ${inflation_adjustment}`);
  } else {
    // Below target: no adjustment
    mint_price = current_mint_price * yesterday_price / latest_price;
    console.log(`📊 Inflation ${inflation_rate}% <= ${target_inflation_rate}%: no adjustment`);
  }
  
  console.log(`💰 Calculated mint price: ${mint_price} (quote token price: ${latest_price} USD)`);
  return mint_price;
}

let priceYesterday = 1.000;

/// Get quote token price from Pyth oracle
// Requirements: Node.js 24 or later
//   npm install @pythnetwork/pyth-lazer-sdk
//   npm pkg set type=module
//
// Usage:
//   node pyth-pro-example.ts
async function get_quote_token_price(env) {
  // Read your API key from the environment (never hard-code it)
  const reserve_symbol = env.RESERVE_SYMBOL;
  const token = env.PYTH_API_KEY;
  if (!token) throw new Error("Set PYTH_API_KEY in your environment");

  const price_feed_ids = {
    'USDC': 7,
    'USDT': 8,
    'PYUSD': 156,
    'USDS': 611,
    'USDG': 232,
    'USD1': 604,
    'devUSDC': 7,
    'devUSDT': 8,
    'devPYUSD': 156,
    'devUSDS': 611,
    'devUSDG': 232,
    'devUSD1': 604,
  };
  // const now = new Date();
  // const secondsNow = Math.floor(now.getTime() / 1000);
  // const secondsYesterday = secondsNow - 24*60*60;
  // console.log(`Yesterday's timestamp in seconds: ${secondsYesterday}`);

  // default prices in case Pyth feed is unavailable
  let latest_price = 1.000;
  let yesterday_price = priceYesterday;

  const price_feed_id = price_feed_ids[reserve_symbol];
  if (!price_feed_id) {
    console.log(`⚠️  No Pyth feed for ${reserve_symbol}, defaulting to $1.00`);
    return { latest_price, yesterday_price };
  }

  // Create the Pyth Pro client with WebSocket pool configuration
  const client = await PythLazerClient.create(
    {
      token,
      // priceServiceUrl: "https://pyth-lazer-0.dourolabs.app", <-- getting a 404
      webSocketPoolConfig: {
        urls: [
          "wss://pyth-lazer-0.dourolabs.app/v1/stream",
          "wss://pyth-lazer-1.dourolabs.app/v1/stream",
          "wss://pyth-lazer-2.dourolabs.app/v1/stream",
        ],
      },
    }
  );
  console.log("ℹ️ Pyth Lazer client created");

  // get the latest price for the specified feed ID
  try {
    client.subscribe({
      type: "subscribe",
      subscriptionId: 1,
      priceFeedIds: [price_feed_id],
      properties: ["price", "feedUpdateTimestamp", "confidence", "exponent"],
      formats: ["solana"],
      channel: "fixed_rate@1000ms",
      jsonBinaryEncoding: "hex",
      parsed: true,
    });
  } catch (error) {
    console.log(`⚠️  Failed to subscribe for ${reserve_symbol}, defaulting to $1.00`);
    return { latest_price, yesterday_price };
  }
  console.log(`ℹ️ Subscription for ${reserve_symbol} successful.`)

  let exponent = 0;
  // Listen for price updates
  client.addMessageListener((message) => {
    if (message.type === "json") {
      // console.log("Received JSON update:", JSON.stringify(message.value, null, 2));
      const priceUpdate = message.value.parsed.priceFeeds[0];
      if (priceUpdate && priceUpdate.price !== undefined) {
        exponent = priceUpdate.exponent !== undefined ? priceUpdate.exponent : 0;
        latest_price = (priceUpdate.price) * Math.pow(10, exponent);
        // console.log(`ℹ️ Updated latest price for ${reserve_symbol}: ${priceUpdate.price} with exponent ${exponent}`);
      }
    } else {
      console.log("Received binary update:", {
        subscriptionId: message.value.subscriptionId,
        parsed: message.value.parsed,
      });
    }
  });
  // wait for 5 seconds to receive price updates
  await new Promise(resolve => setTimeout(resolve, 5000));
  console.log(`ℹ️ Latest price for ${reserve_symbol}: ${latest_price}`);

  // Handle connection errors
  client.addAllConnectionsDownListener(() => {
    console.error("All WebSocket connections are down!");
  })
  console.log(`ℹ️ Yesterday's price for ${reserve_symbol}: ${priceYesterday}`);
  priceYesterday = latest_price;

  return { latest_price, yesterday_price };
}



/**
 * Update IRMA mint price on-chain using Truflation data
 */
async function update_mint_price_from_truflation(env) {
  console.log("🔄 Starting mint price update from Truflation...");
  
  try {
    // 1. Fetch inflation rate from Truflation
    const inflation_rate = await fetch_truflation_rate(env);
    // 2. Setup Solana connection and program
    const { connection, adminKeypair, wallet, provider, program, statePda, corePda } = await setupSolanaConnection(env);
    
    // 3. Get quote token price using Pyth oracle
    const { latest_price, yesterday_price } = await get_quote_token_price(env);

    // 3a. fetch the current mint price from on-chain state
    const { mintPrice } = await getPrices(
      program, statePda, corePda, adminKeypair.publicKey, env.RESERVE_SYMBOL);
    console.log(`ℹ️ Current mint price for ${env.RESERVE_SYMBOL}: ${mintPrice}`);

    // 4. Calculate new mint price
    const new_mint_price = calculate_mint_price(inflation_rate, latest_price, yesterday_price, mintPrice);

    // 5. Convert to format expected by on-chain program
    const price_as_number = new_mint_price;

    console.log(`📝 Setting mint price for ${env.RESERVE_SYMBOL} to ${price_as_number}...`);

    // 6. Call set_mint_price instruction on IRMA program
    const tx_instruction = await program.methods
      .setMintPrice(env.RESERVE_SYMBOL, price_as_number)
      .accounts({
        state: statePda,
        irmaAdmin: wallet.publicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .transaction();
    
    tx_instruction.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
    tx_instruction.feePayer = adminKeypair.publicKey;
    tx_instruction.sign(adminKeypair);
    
    const tx = await connection.sendRawTransaction(tx_instruction.serialize(), { 
      skipPreflight: false,
      maxRetries: 0
    });
    console.log(`✅ Mint price updated! Transaction: ${tx}`);
    
    return {
      success: true,
      inflation_rate,
      quote_token_price_usd: latest_price,
      new_mint_price,
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
  // Handle HTTP requests (webhooks from Helius, manual triggers)
  async fetch(request, env, ctx) {
    return handle_request(request, env, ctx);
  },
  
  // Handle scheduled triggers (cron jobs) - runs daily to update mint price
  async scheduled(event, env, ctx) {
    console.log("⏰ Scheduled trigger: Updating mint price from Truflation...");
    ctx.waitUntil(handle_scheduled_mint_price_update(env));
  }
};

/**
 * Handle scheduled mint price update with automatic bin rebalancing
 * Calls on-chain check_shift_price_ranges() after price update
 */
async function handle_scheduled_mint_price_update(env) {
  const logger = new Logger();
  
  try {
    logger.log("⏰ Scheduled trigger: Updating mint price from Truflation...");
    
    const result = await update_mint_price_from_truflation(env);
    logger.log(`✅ Scheduled mint price update completed: ${JSON.stringify(result)}`);
    
    // Log price update to D1 database
    await logPriceUpdate(env.DB, {
      inflationRate: result.inflation_rate,
      quoteTokenPriceUsd: result.quote_token_price_usd,
      newMintPrice: result.new_mint_price,
      txSignature: result.transaction,
      triggerType: 'scheduled',
      success: true,
    });
    
    // After price update, check and rebalance bins if needed
    if (result.success) {
      logger.log("🔄 Checking bin synchronization after price update...");
      
      try {
        // Call on-chain check_shift_price_ranges on-chain transaction
        const rebalance_result = await check_shift_price_range_worker(env, logger);
        logger.log("✅ On-chain check shift price complete");
      } catch (shift_price_error) {
        logger.error(`❌ Bin rebalancing after price update failed: ${shift_price_error.message}`);
        console.error(`❌ Bin rebalancing after price update failed: ${shift_price_error.message}`);
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

async function handle_request(request, env, ctx) {
  const url = new URL(request.url);
  
  // ACTION ENDPOINTS - Triggered via query parameter or GET request
  if (request.method === 'GET') {
    if (env.ENABLE_TEST_SCAFFOLDING !== "true") {
      return new Response("Not Found", { status: 404 });
    }
    
    const action = url.searchParams.get('action') || url.pathname.slice(1);
    
    if (action === 'update-mint-price') {
      console.log("🔧 Manual trigger: Update mint price from Truflation");
      try {
        const result = await update_mint_price_from_truflation(env);
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
        const inflation_rate = await fetch_truflation_rate(env);
        return new Response(JSON.stringify({
          success: true,
          inflationRate: inflation_rate,
          message: `Current inflation rate: ${inflation_rate}%`
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
    
    if (action === 'rebalance-bins') {
      console.log("🔧 Manual trigger: Rebalance bins");
      const consoleLogger = { log: console.log.bind(console), error: console.error.bind(console), flush: async () => {} };
      try {
        const result = await check_shift_price_range_worker(env, consoleLogger);
        return new Response(JSON.stringify({
          success: true,
          message: "Bin rebalancing completed",
          result
        }), {
          status: 200,
          headers: { "Content-Type": "application/json" }
        });
      } catch (error) {
        const logs = error.getLogs ? await error.getLogs() : null;
        return new Response(JSON.stringify({
          success: false,
          error: error.message,
          logs
        }), {
          status: 500,
          headers: { "Content-Type": "application/json" }
        });
      }
    }

    if (action === 'view-logs') {
      const log_type = url.searchParams.get('type') || 'console';
      const limit = parseInt(url.searchParams.get('limit') || '100', 10);
      const offset = parseInt(url.searchParams.get('offset') || '0', 10);
      const expand = url.searchParams.get('expand') === 'true';
      
      try {
        const result = await queryLogs(env.DB, log_type, limit, offset, expand);
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
    
    if (action === 'view-bins') {
      try {
        const active_bins = await getActiveBins(env.DB);
        if (!active_bins) {
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
          data: active_bins
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
  
  // POST ENDPOINT - Helius webhook for swap events
  if (request.method !== 'POST') {
    return new Response("Method not allowed", { status: 405 });
  }

  try {
    const data = await request.json();
    const tx = data[0]; 

    // Basic checks
    if (!tx) return new Response("Ignored", { status: 200 });

    console.log(`🔔 tx.meta.logMessages: ${JSON.stringify(tx.meta.logMessages)}`);
    
    // Loop prevention - check for worker memo string
    const logs = tx.meta.logMessages || [];
    const is_worker_swap = logs.some(log => log.includes(WORKER_MEMO_STRING));
    if (is_worker_swap) {
      console.log("🚫 Ignored Worker-Initiated Swap (Loop Prevention)");
      return new Response("Ignored (Worker Swap)", { status: 200 });
    }

    console.log(`🔍 Is there an error? ${tx.meta.err}`);
    if (!tx.meta || tx.meta.err !== null) return new Response("Ignored", { status: 200 });

    const is_swap_instruction = logs.some(log => 
      log.includes("Instruction: Swap2") || log.includes("Instruction: Swap")
    );
    if (!is_swap_instruction) return new Response("Ignored (Not a Swap)", { status: 200 });

    // Use waitUntil to allow processing to complete after response
    ctx.waitUntil(process_rebalance(tx, env, ctx));
    
    return new Response("Processing started", { status: 200 });

  } catch (err) {
    console.error("Worker Error:", err);
    return new Response("Error handled", { status: 200 });
  }
}

/**
 * Process rebalancing logic after detecting a swap event
 */
async function process_rebalance(tx, env, ctx) {
  const logger = new Logger();
  
  try {
    logger.log(`🔄 Processing rebalance for transaction: ${tx.transaction.signatures[0]}`);
      
    try {
      // Notify IRMA on swap event, then call on-chain check_shift_price_ranges on-chain transaction
      await processRebalance(tx, env, ctx);
      logger.log("✅ Processing of rebalance complete");
    }
    catch (shift_price_error) {
      logger.error(`❌ Bin rebalancing after price update failed: ${shift_price_error.message}`);
      console.error(`❌ Bin rebalancing after price update failed: ${shift_price_error.message}`);
    }
    
  } catch (error) {
    logger.error(`❌ Rebalancing failed: ${error.message}`);
    console.error("❌ Rebalancing failed:", error.message);
  }
  
  await logger.flush();
}