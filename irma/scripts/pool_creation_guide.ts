#!/usr/bin/env ts-node

/**
 * IRMA/USDT Pool Creation Instructions
 * 
 * This script provides step-by-step instructions for creating an IRMA/USDT 
 * pool on Orca Whirlpools devnet using various methods.
 */

import { PublicKey } from "@solana/web3.js";
import { PoolUtil } from "@orca-so/whirlpools-sdk";

// Configuration
const IRMA_MINT = new PublicKey("irmacFBRx7148dQ6qq1zpzUPq57Jr8V4vi5eXDxsDe1");
const USDT_MINT = new PublicKey("Es9vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p");
const FUNDER = new PublicKey("irmaVaRc8xvqiXxAA55dYK2NbRidJ27DPzyQ4HEDfSV");

// Tick spacing for 0.01 USDT (1 basis point)
const TICK_SPACING = 1;

function printPoolCreationInstructions() {
  console.log("🌊 IRMA/USDT Pool Creation Instructions");
  console.log("=====================================\n");

  // Determine token order
  const [tokenA, tokenB] = PoolUtil.orderMints(IRMA_MINT, USDT_MINT);
  
  console.log("📋 Pool Configuration:");
  console.log("   Token A (lexicographically lower):", tokenA);
  console.log("   Token B (lexicographically higher):", tokenB);
  console.log("   Tick Spacing:", TICK_SPACING, "(for 0.01% fee tier)");
  console.log("   Initial Price: 1 IRMA = 1 USDT");
  console.log("   Funder/Authority:", FUNDER.toString());
  console.log("   Network: Devnet");

  console.log("\n🎯 Method 1: Orca Web Interface (Recommended)");
  console.log("=============================================");
  console.log("1. Visit: https://www.orca.so/pools");
  console.log("2. Connect wallet:", FUNDER.toString());
  console.log("3. Click 'Create Pool'");
  console.log("4. Enter token addresses:");
  console.log("   - First token:", IRMA_MINT.toString());
  console.log("   - Second token:", USDT_MINT.toString());
  console.log("5. Set initial price: 1.0 (1 IRMA = 1 USDT)");
  console.log("6. Choose fee tier: 0.01% (1 basis point)");
  console.log("7. Confirm and sign transaction");

  console.log("\n⚙️  Method 2: Orca CLI Tools");
  console.log("============================");
  console.log("1. Install Orca CLI:");
  console.log("   npm install -g @orca-so/whirlpools-cli");
  console.log("");
  console.log("2. Create pool:");
  console.log("   whirlpools create-pool \\");
  console.log("     --network devnet \\");
  console.log(`     --token-a ${tokenA} \\`);
  console.log(`     --token-b ${tokenB} \\`);
  console.log(`     --tick-spacing ${TICK_SPACING} \\`);
  console.log("     --initial-price 1.0 \\");
  console.log(`     --funder ${FUNDER.toString()}`);

  console.log("\n🔧 Method 3: TypeScript SDK");
  console.log("===========================");
  console.log("Use the WhirlpoolIx from @orca-so/whirlpools-sdk:");
  console.log("");
  console.log("```typescript");
  console.log("import { WhirlpoolIx, ORCA_WHIRLPOOL_PROGRAM_ID } from '@orca-so/whirlpools-sdk';");
  console.log("");
  console.log("// 1. Initialize FeeTier");
  console.log("const initFeeTierIx = WhirlpoolIx.initializeFeeTierIx(");
  console.log("  program, {");
  console.log("    whirlpoolsConfig,");
  console.log(`    tickSpacing: ${TICK_SPACING},`);
  console.log("    defaultFeeRate: 100, // 0.01%");
  console.log("    feeAuthority: funder.publicKey");
  console.log("  }");
  console.log(");");
  console.log("");
  console.log("// 2. Initialize Whirlpool");
  console.log("const initPoolIx = WhirlpoolIx.initializePoolIx(");
  console.log("  program, {");
  console.log("    whirlpoolsConfig,");
  console.log(`    tokenMintA: new PublicKey('${tokenA}'),`);
  console.log(`    tokenMintB: new PublicKey('${tokenB}'),`);
  console.log(`    tickSpacing: ${TICK_SPACING},`);
  console.log("    initialSqrtPrice: PriceMath.priceToSqrtPriceX64(new Decimal(1.0), 6, 6),");
  console.log("    funder: funder.publicKey");
  console.log("  }");
  console.log(");");
  console.log("```");

  console.log("\n📋 Required Accounts & PDAs");
  console.log("===========================");
  console.log("When creating the pool, you'll need these derived addresses:");
  console.log("");
  console.log("WhirlpoolsConfig: (Use Orca's official config or create your own)");
  console.log("FeeTier PDA: Derived from [whirlpoolsConfig, tickSpacing]");
  console.log("Whirlpool PDA: Derived from [whirlpoolsConfig, tokenA, tokenB, tickSpacing]");
  console.log("Token Vaults: Created automatically during pool initialization");
  console.log("Oracle PDA: Derived from whirlpool address");

  console.log("\n⚠️  Prerequisites");
  console.log("=================");
  console.log("1. ✅ IRMA token must exist on devnet");
  console.log("2. ✅ USDT token must exist on devnet (or use devnet equivalent)");
  console.log("3. ✅ Funder wallet must have SOL for transaction fees");
  console.log("4. ✅ Funder wallet should have initial tokens for liquidity");
  console.log("5. ✅ WhirlpoolsConfig must be initialized (or use Orca's)");

  console.log("\n🎯 Next Steps After Pool Creation");
  console.log("=================================");
  console.log("1. Initialize TickArrays for price ranges");
  console.log("2. Add initial liquidity to the pool");
  console.log("3. Test swaps to ensure functionality");
  console.log("4. Integrate pool address into IRMA program");
  console.log("5. Set up price feeds and oracle integration");

  console.log("\n📞 Support");
  console.log("===========");
  console.log("- Orca Documentation: https://docs.orca.so/");
  console.log("- Orca Discord: https://discord.gg/orcaprotocol");
  console.log("- Whirlpools SDK: https://github.com/orca-so/whirlpools");

  console.log("\n" + "=".repeat(60));
  console.log("💡 Tip: Start with the Web Interface for easiest setup!");
  console.log("=".repeat(60));
}

// Run the instructions
if (require.main === module) {
  printPoolCreationInstructions();
}

export {
  printPoolCreationInstructions,
  IRMA_MINT,
  USDT_MINT,
  FUNDER,
  TICK_SPACING
};