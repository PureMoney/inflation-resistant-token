import {
  WhirlpoolContext,
  buildWhirlpoolClient,
  ORCA_WHIRLPOOL_PROGRAM_ID,
  PDAUtil,
  PoolUtil,
  PriceMath,
  WhirlpoolIx,
} from "@orca-so/whirlpools-sdk";
import { AnchorProvider, Wallet, Program } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  sendAndConfirmTransaction,
  TransactionInstruction,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { DecimalUtil, Percentage } from "@orca-so/common-sdk";
import Decimal from "decimal.js";

// Configuration
const DEVNET_RPC_URL = "https://api.devnet.solana.com";
const IRMA_MINT = new PublicKey("irmacFBRx7148dQ6qq1zpzUPq57Jr8V4vi5eXDxsDe1");
const USDT_MINT = new PublicKey("Es9vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p");
const FUNDER = new PublicKey("irmaVaRc8xvqiXxAA55dYK2NbRidJ27DPzyQ4HEDfSV");

// Tick spacing for 0.01 USDT (1 basis point)
// In Orca, tick spacing is related to fee tiers
// For 0.01% (1 basis point), we typically use tick spacing of 1
const TICK_SPACING = 1;

// Fee rate: 0.01% = 100 basis points (out of 1,000,000)
const FEE_RATE = 100;

async function createIrmaUsdtPool() {
  console.log("🌊 Creating IRMA/USDT Pool on Orca Whirlpools (Devnet)");
  console.log("======================================================");

  // Initialize connection
  const connection = new Connection(DEVNET_RPC_URL, "confirmed");
  
  // For this example, we'll need a keypair with SOL for transaction fees
  // In production, this would be loaded from a secure location
  const payerKeypair = Keypair.generate(); // Replace with actual funded keypair
  console.log("⚠️  Generated new keypair for demo. Fund this address with SOL:");
  console.log("   Address:", payerKeypair.publicKey.toString());
  console.log("   You can get devnet SOL from: https://faucet.solana.com/");
  
  // Check if we should continue (in production, load an actual funded keypair)
  const balance = await connection.getBalance(payerKeypair.publicKey);
  if (balance === 0) {
    console.log("❌ Payer has no SOL balance. Please fund the address above and run again.");
    return;
  }

  const wallet = new Wallet(payerKeypair);
  const provider = new AnchorProvider(connection, wallet, AnchorProvider.defaultOptions());
  
  // Initialize Whirlpool client
  const ctx = WhirlpoolContext.withProvider(provider, ORCA_WHIRLPOOL_PROGRAM_ID);
  const client = buildWhirlpoolClient(ctx);

  try {
    // Step 1: Get or create WhirlpoolsConfig
    console.log("📋 Step 1: Getting WhirlpoolsConfig...");
    
    const whirlpoolsConfigKeypair = Keypair.generate();
    const whirlpoolsConfig = whirlpoolsConfigKeypair.publicKey;
    
    // Initialize WhirlpoolsConfig account
    const configInitTx = new Transaction();
    configInitTx.add(
      SystemProgram.createAccount({
        fromPubkey: payerKeypair.publicKey,
        newAccountPubkey: whirlpoolsConfig,
        lamports: await connection.getMinimumBalanceForRentExemption(8 + 32 + 32 + 16 + 1), // Approximate size
        space: 8 + 32 + 32 + 16 + 1, // discriminator + fee_authority + collect_protocol_fees_authority + protocol_fee_rate + default_protocol_fee_rate
        programId: ORCA_WHIRLPOOL_PROGRAM_ID,
      })
    );

    await sendAndConfirmTransaction(connection, configInitTx, [payerKeypair, whirlpoolsConfigKeypair]);
    console.log("✅ WhirlpoolsConfig created:", whirlpoolsConfig.toString());

    // Step 2: Create FeeTier if it doesn't exist
    console.log("📋 Step 2: Creating FeeTier...");
    
    const feeTierPda = PDAUtil.getFeeTier(
      ORCA_WHIRLPOOL_PROGRAM_ID,
      whirlpoolsConfig,
      TICK_SPACING
    );

    console.log("   FeeTier PDA:", feeTierPda.publicKey.toString());

    // Check if FeeTier already exists
    const feeTierAccount = await connection.getAccountInfo(feeTierPda.publicKey);
    if (!feeTierAccount) {
      console.log("   Creating new FeeTier...");
      
      const initFeeTierTx = await client.initializeFeeTierTx({
        whirlpoolsConfig,
        tickSpacing: TICK_SPACING,
        defaultFeeRate: FEE_RATE,
        feeAuthority: payerKeypair.publicKey,
      });

      const signature = await initFeeTierTx.buildAndExecute();
      console.log("✅ FeeTier initialized, signature:", signature);
    } else {
      console.log("✅ FeeTier already exists");
    }

    // Step 3: Create the Whirlpool
    console.log("📋 Step 3: Creating IRMA/USDT Whirlpool...");

    // Ensure token order (Orca requires token A < token B lexicographically)
    const [tokenAMint, tokenBMint] = PoolUtil.orderMints(IRMA_MINT, USDT_MINT);
    const tokenA = new PublicKey(tokenAMint);
    const tokenB = new PublicKey(tokenBMint);
    
    console.log("   Token A (lower):", tokenA.toString());
    console.log("   Token B (higher):", tokenB.toString());
    console.log("   Tick Spacing:", TICK_SPACING);

    // Calculate initial price (1 IRMA = 1 USDT)
    // Both tokens have 6 decimals
    const initialPrice = new Decimal(1.0); // 1 IRMA = 1 USDT
    const initialSqrtPrice = PriceMath.priceToSqrtPriceX64(
      initialPrice,
      6, // IRMA decimals
      6  // USDT decimals
    );

    console.log("   Initial Price:", initialPrice.toString());
    console.log("   Initial Sqrt Price:", initialSqrtPrice.toString());

    // Get Whirlpool PDA
    const whirlpoolPda = PDAUtil.getWhirlpool(
      ORCA_WHIRLPOOL_PROGRAM_ID,
      whirlpoolsConfig,
      tokenA,
      tokenB,
      TICK_SPACING
    );

    console.log("   Whirlpool PDA:", whirlpoolPda.publicKey.toString());

    // Check if Whirlpool already exists
    const whirlpoolAccount = await connection.getAccountInfo(whirlpoolPda.publicKey);
    if (!whirlpoolAccount) {
      console.log("   Creating new Whirlpool...");

      const initPoolTx = await client.initializePoolTx({
        whirlpoolsConfig,
        tokenMintA: tokenA,
        tokenMintB: tokenB,
        tickSpacing: TICK_SPACING,
        initialSqrtPrice: initialSqrtPrice,
        funder: FUNDER,
      });

      const signature = await initPoolTx.buildAndExecute();
      console.log("✅ Whirlpool created, signature:", signature);
    } else {
      console.log("✅ Whirlpool already exists");
    }

    // Step 4: Initialize TickArrays (required for swaps)
    console.log("📋 Step 4: Initializing TickArrays...");

    const whirlpool = await client.getPool(whirlpoolPda.publicKey);
    const whirlpoolData = whirlpool.getData();

    // Initialize tick arrays around the current price
    const currentTickIndex = whirlpoolData.tickCurrentIndex;
    const tickArrays = [
      currentTickIndex - TICK_SPACING * 88, // Lower tick array
      currentTickIndex,                      // Current tick array  
      currentTickIndex + TICK_SPACING * 88  // Upper tick array
    ];

    for (const startTickIndex of tickArrays) {
      const tickArrayPda = PDAUtil.getTickArray(
        ORCA_WHIRLPOOL_PROGRAM_ID,
        whirlpoolPda.publicKey,
        startTickIndex
      );

      const tickArrayAccount = await connection.getAccountInfo(tickArrayPda.publicKey);
      if (!tickArrayAccount) {
        console.log(`   Creating TickArray at index ${startTickIndex}...`);
        
        const initTickArrayTx = await client.initializeTickArrayTx({
          whirlpool: whirlpoolPda.publicKey,
          startTickIndex,
          funder: FUNDER,
        });

        const signature = await initTickArrayTx.buildAndExecute();
        console.log(`   ✅ TickArray created, signature: ${signature}`);
      } else {
        console.log(`   ✅ TickArray at index ${startTickIndex} already exists`);
      }
    }

    // Step 5: Summary
    console.log("\n🎉 IRMA/USDT Pool Creation Complete!");
    console.log("=====================================");
    console.log("WhirlpoolsConfig:", whirlpoolsConfig.toString());
    console.log("FeeTier PDA:", feeTierPda.publicKey.toString());
    console.log("Whirlpool PDA:", whirlpoolPda.publicKey.toString());
    console.log("Token A (IRMA):", IRMA_MINT.toString());
    console.log("Token B (USDT):", USDT_MINT.toString());
    console.log("Tick Spacing:", TICK_SPACING);
    console.log("Fee Rate:", FEE_RATE, "basis points (0.01%)");
    console.log("Initial Price: 1 IRMA = 1 USDT");
    
    console.log("\n📝 To use this pool in your Solana program:");
    console.log("1. Use the Whirlpool PDA:", whirlpoolPda.publicKey.toString());
    console.log("2. Derive oracle PDA using the whirlpool address");
    console.log("3. Get token vaults from the whirlpool account data");
    console.log("4. Use tick arrays for swap calculations");

  } catch (error) {
    console.error("❌ Error creating pool:", error);
    if (error instanceof Error) {
      console.error("Error message:", error.message);
      console.error("Stack trace:", error.stack);
    }
  }
}

// Helper function to check token mint info
async function checkTokenMints() {
  const connection = new Connection(DEVNET_RPC_URL, "confirmed");
  
  console.log("🔍 Checking token mints on devnet...");
  
  try {
    const irmaMintInfo = await connection.getAccountInfo(IRMA_MINT);
    const usdtMintInfo = await connection.getAccountInfo(USDT_MINT);
    
    console.log("IRMA mint exists:", irmaMintInfo !== null);
    console.log("USDT mint exists:", usdtMintInfo !== null);
    
    if (!irmaMintInfo) {
      console.log("⚠️  IRMA mint not found on devnet. You may need to:");
      console.log("   1. Deploy the IRMA token to devnet first");
      console.log("   2. Update the mint address if different on devnet");
    }
    
    if (!usdtMintInfo) {
      console.log("⚠️  USDT mint not found on devnet. You may need to:");
      console.log("   1. Use a different USDT mint address for devnet");
      console.log("   2. Create a mock USDT token for testing");
    }
  } catch (error) {
    console.error("Error checking mints:", error);
  }
}

// Main execution
async function main() {
  console.log("🚀 IRMA/USDT Pool Creation Script");
  console.log("=================================\n");
  
  // First check if the tokens exist
  await checkTokenMints();
  
  console.log("\n" + "=".repeat(50) + "\n");
  
  // Create the pool
  await createIrmaUsdtPool();
}

// Run the script
if (require.main === module) {
  main().catch((error) => {
    console.error("Script failed:", error);
    process.exit(1);
  });
}

export { createIrmaUsdtPool, checkTokenMints };