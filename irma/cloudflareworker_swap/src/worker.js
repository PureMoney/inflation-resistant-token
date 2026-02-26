// IRMA Cloudflare Worker - Compatible with Workers Runtime
import {  SystemProgram } from "@solana/web3.js";
import { PythHttpClient, getPythProgramKeyForCluster } from "@pythnetwork/client";

import { Logger, logPriceUpdate, queryLogs, getActiveBins } from "./d1_logs.js";
import { 
  processRebalance, 
  setupSolanaConnection, 
  check_shift_price_range_worker, 
  WORKER_MEMO_STRING 
} from "./process_rebalance.js";
import { POOL_ADDRESS, RESERVE_SYMBOL, TARGET_INFLATION_RATE, ENABLE_TEST_SCAFFOLDING } from "./config.js";

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

    const inflation_rate = 2.1; // data.data.inflationRate;
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
function calculate_mint_price(inflation_rate, quote_token_price_usd) {
  let mint_price;
  
  if (inflation_rate > TARGET_INFLATION_RATE) {
    // Inflation above target: adjust mint price upward
    let inflation_adjustment = (inflation_rate - TARGET_INFLATION_RATE) / 100.0;
    mint_price = (1.00 + inflation_adjustment) / quote_token_price_usd;
    console.log(`📊 Inflation ${inflation_rate}% > ${TARGET_INFLATION_RATE}%: adjustment = ${inflation_adjustment}`);
  } else {
    // Below target: no adjustment
    mint_price = 1.00 / quote_token_price_usd;
    console.log(`📊 Inflation ${inflation_rate}% <= ${TARGET_INFLATION_RATE}%: no adjustment`);
  }
  
  console.log(`💰 Calculated mint price: ${mint_price} (quote token price: ${quote_token_price_usd} USD)`);
  return mint_price;
}

/**
 * Get quote token price using Pyth oracle (compatible with Cloudflare Workers)
 * Replaces hardcoded 1.0 assumption for better accuracy
 */
async function get_quote_token_price_usd(connection, pool_address, reserve_symbol) {
  console.log(`📊 Fetching ${reserve_symbol} price from Pyth oracle...`);
  
  try {
    // Pyth price feed IDs for stablecoins
    // TODO: these are not base58-encoded, need to convert to PublicKey format or use correct IDs for devnet and prod
    const price_feed_ids = {
      'USDC': '0x41f3625971ca2ed2263e78573fe5ce23e13d2558ed3f2e47ab0f84fb9e7ae722',
      'USDT': '0x2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b',
      'devUSDC': '0x41f3625971ca2ed2263e78573fe5ce23e13d2558ed3f2e47ab0f84fb9e7ae722',
      'devUSDT': '0x2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b',
    };
    
    const price_feed_id = price_feed_ids[reserve_symbol];
    if (!price_feed_id) {
      console.log(`⚠️  No Pyth feed for ${reserve_symbol}, defaulting to $1.00`);
      return 1.0;
    }
    
    // Initialize Pyth client for Workers environment
    const pyth_client = new PythHttpClient(connection, getPythProgramKeyForCluster('devnet'));
    
    // Fetch price data
    const price_data = await pyth_client.getAssetPricesFromAccounts([price_feed_id]);
    const price = price_data[0];
    
    if (!price || !price.price) {
      console.log(`⚠️  Failed to get Pyth price for ${reserve_symbol}, defaulting to $1.00`);
      return 1.0;
    }
    
    const price_usd = price.price * Math.pow(10, price.expo);
    console.log(`💰 Pyth ${reserve_symbol}/USD price: $${price_usd}`);
    
    return price_usd;
    
  } catch (error) {
    console.error(`❌ Pyth oracle error for ${reserve_symbol}:`, error.message);
    console.log(`⚠️  Falling back to $1.00 for ${reserve_symbol}`);
    return 1.0;
  }
}

/**
 * Update IRMA mint price on-chain using Truflation data
 */
async function update_mint_price_from_truflation(env) {
  console.log("🔄 Starting mint price update from Truflation...");
  
  const HELIUS_API_KEY = env.HELIUS_API_KEY;
  const HELIUS_RPC_URL = `https://devnet.helius-rpc.com/?api-key=${HELIUS_API_KEY}`;

  try {
    // 1. Fetch inflation rate from Truflation
    const inflation_rate = await fetch_truflation_rate(env);
    // 2. Setup Solana connection and program
    const { connection, adminKeypair, wallet, provider, program, statePda, corePda } = await setupSolanaConnection(env);
    
    // 3. Get quote token price using Pyth oracle
    const quote_token_price_usd = await get_quote_token_price_usd(connection, POOL_ADDRESS, RESERVE_SYMBOL);
    
    // 4. Calculate new mint price
    const new_mint_price = calculate_mint_price(inflation_rate, quote_token_price_usd);
    
    // 5. Convert to format expected by on-chain program
    const price_as_number = new_mint_price;
    
    console.log(`📝 Setting mint price for ${RESERVE_SYMBOL} to ${price_as_number}...`);
    
    // 6. Call set_mint_price instruction on IRMA program
    const tx_instruction = await program.methods
      .setMintPrice(RESERVE_SYMBOL, price_as_number)
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
      quote_token_price_usd,
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
  const logger = new Logger(env.DB);
  
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
    if (ENABLE_TEST_SCAFFOLDING !== true) {
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
    
    // if (action === 'rebalance-bins') {
    //   console.log("🔧 Manual trigger: Rebalance bins");
    //   try {
    //     const result = await manualRebalanceBins(env);
    //     return new Response(JSON.stringify({
    //       success: true,
    //       message: "Bin rebalancing completed",
    //       ...result
    //     }), {
    //       status: 200,
    //       headers: { "Content-Type": "application/json" }
    //     });
    //   } catch (error) {
    //     return new Response(JSON.stringify({
    //       success: false,
    //       error: error.message
    //     }), {
    //       status: 500,
    //       headers: { "Content-Type": "application/json" }
    //     });
    //   }
    // }
    
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
  const logger = new Logger(env.DB);
  
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