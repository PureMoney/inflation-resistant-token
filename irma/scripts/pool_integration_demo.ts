#!/usr/bin/env ts-node

/**
 * IRMA/USDT Pool Integration Example
 * 
 * This script demonstrates how to interact with an existing IRMA/USDT pool
 * on Orca Whirlpools for use in the IRMA program.
 */

import {
  WhirlpoolContext,
  buildWhirlpoolClient,
  ORCA_WHIRLPOOL_PROGRAM_ID,
  PDAUtil,
  PoolUtil,
  PriceMath,
  swapQuoteByInputToken,
} from "@orca-so/whirlpools-sdk";
import { AnchorProvider, Wallet } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  PublicKey,
} from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { Percentage, DecimalUtil } from "@orca-so/common-sdk";
import Decimal from "decimal.js";

// Configuration
const DEVNET_RPC_URL = "https://api.devnet.solana.com";
const IRMA_MINT = new PublicKey("irmacFBRx7148dQ6qq1zpzUPq57Jr8V4vi5eXDxsDe1");
const USDT_MINT = new PublicKey("Es9vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p");
const TICK_SPACING = 1;

// Example Orca config (replace with actual)
const WHIRLPOOLS_CONFIG = new PublicKey("2LecshUwdy9xi7meFgHtFJQNSKk4KdTrcpvaB56dP2NQ");

async function demonstratePoolIntegration() {
  console.log("🔗 IRMA/USDT Pool Integration Demo");
  console.log("=================================\n");

  // Initialize connection and client
  const connection = new Connection(DEVNET_RPC_URL, "confirmed");
  const dummyKeypair = Keypair.generate(); // For read-only operations
  const wallet = new Wallet(dummyKeypair);
  const provider = new AnchorProvider(connection, wallet, AnchorProvider.defaultOptions());
  
  const ctx = WhirlpoolContext.withProvider(provider, ORCA_WHIRLPOOL_PROGRAM_ID);
  const client = buildWhirlpoolClient(ctx);

  try {
    // Step 1: Get pool address
    console.log("📋 Step 1: Getting pool address...");
    
    const [tokenA, tokenB] = PoolUtil.orderMints(IRMA_MINT, USDT_MINT);
    const tokenAKey = new PublicKey(tokenA);
    const tokenBKey = new PublicKey(tokenB);
    
    const whirlpoolPda = PDAUtil.getWhirlpool(
      ORCA_WHIRLPOOL_PROGRAM_ID,
      WHIRLPOOLS_CONFIG,
      tokenAKey,
      tokenBKey,
      TICK_SPACING
    );

    console.log("   Whirlpool Address:", whirlpoolPda.publicKey.toString());
    console.log("   Token A:", tokenAKey.toString());
    console.log("   Token B:", tokenBKey.toString());

    // Step 2: Check if pool exists
    console.log("\n📋 Step 2: Checking pool status...");
    
    const poolAccount = await connection.getAccountInfo(whirlpoolPda.publicKey);
    if (!poolAccount) {
      console.log("❌ Pool not found. Please create it first using the pool creation guide.");
      console.log("   Run: ts-node scripts/pool_creation_guide.ts");
      return;
    }

    // Step 3: Get pool data
    console.log("\n📋 Step 3: Reading pool data...");
    
    const pool = await client.getPool(whirlpoolPda.publicKey);
    const poolData = pool.getData();
    
    console.log("✅ Pool found and loaded!");
    console.log("   Current Tick:", poolData.tickCurrentIndex);
    console.log("   Current Sqrt Price:", poolData.sqrtPrice.toString());
    console.log("   Liquidity:", poolData.liquidity.toString());
    console.log("   Fee Rate:", poolData.feeRate, "basis points");

    // Step 4: Calculate current price
    console.log("\n📋 Step 4: Calculating current IRMA price...");
    
    const currentPrice = PriceMath.sqrtPriceX64ToPrice(
      poolData.sqrtPrice,
      6, // IRMA decimals
      6  // USDT decimals
    );
    
    console.log("   Current IRMA Price:", currentPrice.toString(), "USDT");

    // Step 5: Get important PDAs for Solana program integration
    console.log("\n📋 Step 5: Getting integration PDAs...");
    
    const oraclePda = PDAUtil.getOracle(
      ORCA_WHIRLPOOL_PROGRAM_ID,
      whirlpoolPda.publicKey
    );
    
    // Get token vaults (need to read from pool data)
    console.log("   Oracle PDA:", oraclePda.publicKey.toString());
    console.log("   Token Vault A:", poolData.tokenVaultA.toString());
    console.log("   Token Vault B:", poolData.tokenVaultB.toString());

    // Step 6: Demo swap quote
    console.log("\n📋 Step 6: Demo swap quote (1000 IRMA → USDT)...");
    
    try {
      const inputTokenAmount = new Decimal(1000); // 1000 IRMA
      const inputTokenMint = IRMA_MINT;
      const slippageTolerance = Percentage.fromFraction(1, 100); // 1%
      
      const quote = await swapQuoteByInputToken(
        pool,
        inputTokenMint,
        DecimalUtil.toU64(inputTokenAmount, 6), // Convert to token units
        slippageTolerance,
        ctx.program.programId,
        ctx.fetcher,
        true // refresh
      );
      
      const outputAmount = DecimalUtil.fromU64(quote.estimatedAmountOut, 6);
      console.log("   Input: 1000 IRMA");
      console.log("   Estimated Output:", outputAmount.toString(), "USDT");
      
      // Price impact calculation may not be available in all quote types
      if ('otherAmountThreshold' in quote) {
        console.log("   Quote type: Normal swap");
      }
      
    } catch (error) {
      console.log("   ⚠️  Swap quote failed (pool may need liquidity or API changed):", 
                 error instanceof Error ? error.message : error);
      console.log("   This is normal if the pool doesn't have liquidity yet.");
    }

    // Step 7: Integration code for Solana program
    console.log("\n📋 Step 7: Solana Program Integration Code...");
    console.log("\n```rust");
    console.log("// Add to your Solana program's account context:");
    console.log("pub struct SwapWithOrca<'info> {");
    console.log("    #[account(address = orca_whirlpools::ID)]");
    console.log("    pub whirlpools_program: AccountInfo<'info>,");
    console.log("    ");
    console.log("    #[account(mut, address = whirlpool_pubkey!(\""+ whirlpoolPda.publicKey.toString() +"\"))]");
    console.log("    pub whirlpool: AccountInfo<'info>,");
    console.log("    ");
    console.log("    #[account(mut, address = pubkey!(\""+ poolData.tokenVaultA.toString() +"\"))]");
    console.log("    pub token_vault_a: AccountInfo<'info>,");
    console.log("    ");
    console.log("    #[account(mut, address = pubkey!(\""+ poolData.tokenVaultB.toString() +"\"))]");
    console.log("    pub token_vault_b: AccountInfo<'info>,");
    console.log("    ");
    console.log("    #[account(address = pubkey!(\""+ oraclePda.publicKey.toString() +"\"))]");
    console.log("    pub oracle: AccountInfo<'info>,");
    console.log("    ");
    console.log("    // ... other required accounts");
    console.log("}");
    console.log("```");

    // Step 8: Price reading function
    console.log("\n```rust");
    console.log("// Function to read IRMA price from the pool:");
    console.log("pub fn get_irma_price_from_orca(ctx: Context<ReadOrcaPrice>) -> Result<u64> {");
    console.log("    let whirlpool_data = ctx.accounts.whirlpool.data.borrow();");
    console.log("    ");
    console.log("    // Extract sqrt_price from pool data (offset may need adjustment)");
    console.log("    let sqrt_price_bytes = &whirlpool_data[8..24]; // Skip discriminator");
    console.log("    let sqrt_price = u128::from_le_bytes([");
    console.log("        sqrt_price_bytes[0], sqrt_price_bytes[1], /* ... all 16 bytes */");
    console.log("    ]);");
    console.log("    ");
    console.log("    // Convert to price (simplified)");
    console.log("    let price = (sqrt_price as u128).saturating_pow(2) >> 64;");
    console.log("    ");
    console.log("    Ok(price as u64)");
    console.log("}");
    console.log("```");

    console.log("\n📋 Summary for IRMA Program Integration:");
    console.log("========================================");
    console.log("✅ Pool Address:", whirlpoolPda.publicKey.toString());
    console.log("✅ Oracle PDA:", oraclePda.publicKey.toString());
    console.log("✅ Token Vault A:", poolData.tokenVaultA.toString());
    console.log("✅ Token Vault B:", poolData.tokenVaultB.toString());
    console.log("✅ Current IRMA Price:", currentPrice.toString(), "USDT");
    console.log("✅ Integration code generated above");

    console.log("\n🎯 Next Steps:");
    console.log("1. Copy the pool address to your IRMA program constants");
    console.log("2. Use the account context in your swap functions");
    console.log("3. Implement price reading for IRMA price feeds");
    console.log("4. Test swaps with small amounts first");
    console.log("5. Add proper error handling and slippage protection");

    return {
      poolAddress: whirlpoolPda.publicKey,
      oracleAddress: oraclePda.publicKey,
      tokenVaultA: poolData.tokenVaultA,
      tokenVaultB: poolData.tokenVaultB,
      currentPrice: currentPrice.toString(),
      liquidity: poolData.liquidity.toString(),
    };

  } catch (error) {
    console.error("❌ Error:", error);
    if (error instanceof Error) {
      console.error("Error message:", error.message);
    }
    throw error;
  }
}

// Helper function for just getting pool address
export function getIrmaUsdtPoolAddress(): PublicKey {
  const [tokenA, tokenB] = PoolUtil.orderMints(IRMA_MINT, USDT_MINT);
  const tokenAKey = new PublicKey(tokenA);
  const tokenBKey = new PublicKey(tokenB);
  
  const whirlpoolPda = PDAUtil.getWhirlpool(
    ORCA_WHIRLPOOL_PROGRAM_ID,
    WHIRLPOOLS_CONFIG,
    tokenAKey,
    tokenBKey,
    TICK_SPACING
  );
  
  return whirlpoolPda.publicKey;
}

// Export constants for use in other files
export {
  IRMA_MINT,
  USDT_MINT,
  TICK_SPACING,
  WHIRLPOOLS_CONFIG,
  ORCA_WHIRLPOOL_PROGRAM_ID
};

// Run if called directly
if (require.main === module) {
  demonstratePoolIntegration().catch((error) => {
    console.error("❌ Demo failed:", error);
    process.exit(1);
  });
}