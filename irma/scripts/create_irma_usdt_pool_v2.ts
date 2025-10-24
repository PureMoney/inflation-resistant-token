import {
  WhirlpoolContext,
  buildWhirlpoolClient,
  ORCA_WHIRLPOOL_PROGRAM_ID,
  PDAUtil,
  PoolUtil,
  PriceMath,
} from "@orca-so/whirlpools-sdk";
import { AnchorProvider, Wallet, BN } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { DecimalUtil, Percentage } from "@orca-so/common-sdk";
import Decimal from "decimal.js";

// Configuration
const DEVNET_RPC_URL = "https://api.devnet.solana.com";
const IRMA_MINT = new PublicKey("irmacFBRx7148dQ6qq1zpzUPq57Jr8V4vi5eXDxsDe1");
const USDT_MINT = new PublicKey("Es9vMFrzaTmVRL3P15S3BtQDvVwWZEzPDk1e45sA2v6p");
const FUNDER = new PublicKey("irmaVaRc8xvqiXxAA55dYK2NbRidJ27DPzyQ4HEDfSV");

// Tick spacing for 0.01 USDT
// Standard Orca tick spacings: 1, 8, 64, 128
// For 0.01% fee tier, use tick spacing 1
const TICK_SPACING = 1;

// Fee rate: 0.01% = 100 basis points (out of 1,000,000)
const FEE_RATE = 100;

async function createIrmaUsdtPool() {
  console.log("🌊 Creating IRMA/USDT Pool on Orca Whirlpools (Devnet)");
  console.log("======================================================");

  // Initialize connection
  const connection = new Connection(DEVNET_RPC_URL, "confirmed");
  
  // Load keypair (replace with your funded keypair)
  const payerKeypair = Keypair.generate();
  console.log("⚠️  Using keypair:", payerKeypair.publicKey.toString());
  console.log("   Make sure this address has SOL for transaction fees");
  
  const wallet = new Wallet(payerKeypair);
  const provider = new AnchorProvider(connection, wallet, AnchorProvider.defaultOptions());
  
  // Initialize Whirlpool client
  const ctx = WhirlpoolContext.withProvider(provider, ORCA_WHIRLPOOL_PROGRAM_ID);
  const client = buildWhirlpoolClient(ctx);

  try {
    // Step 1: Use Orca's standard configuration
    console.log("📋 Step 1: Using Orca configuration...");
    
    // For devnet, you can use Orca's standard config
    // This is typically provided by Orca or needs to be set up
    const whirlpoolsConfig = new PublicKey("2LecshUwdy9xi7meFgHtFJQNSKk4KdTrcpvaB56dP2NQ");

    // Step 2: Determine token order
    console.log("📋 Step 2: Determining token order...");
    
    const tokenMints = PoolUtil.orderMints(IRMA_MINT, USDT_MINT);
    const tokenA = new PublicKey(tokenMints[0]);
    const tokenB = new PublicKey(tokenMints[1]);
    
    console.log("   Token A (lower):", tokenA.toString());
    console.log("   Token B (higher):", tokenB.toString());
    console.log("   Tick Spacing:", TICK_SPACING);

    // Step 3: Calculate initial price (1 IRMA = 1 USDT)
    console.log("📋 Step 3: Calculating initial price...");
    
    const initialPrice = new Decimal(1.0); // 1 IRMA = 1 USDT
    const initialSqrtPrice = PriceMath.priceToSqrtPriceX64(
      initialPrice,
      6, // IRMA decimals
      6  // USDT decimals
    );

    console.log("   Initial Price:", initialPrice.toString());
    console.log("   Initial Sqrt Price:", initialSqrtPrice.toString());

    // Step 4: Get Whirlpool PDA
    console.log("📋 Step 4: Getting Whirlpool PDA...");
    
    const whirlpoolPda = PDAUtil.getWhirlpool(
      ORCA_WHIRLPOOL_PROGRAM_ID,
      whirlpoolsConfig,
      tokenA,
      tokenB,
      TICK_SPACING
    );

    console.log("   Whirlpool PDA:", whirlpoolPda.publicKey.toString());

    // Step 5: Check if pool already exists
    console.log("📋 Step 5: Checking if pool exists...");
    
    const whirlpoolAccount = await connection.getAccountInfo(whirlpoolPda.publicKey);
    if (whirlpoolAccount) {
      console.log("✅ Pool already exists at:", whirlpoolPda.publicKey.toString());
      
      // Get pool data
      const pool = await client.getPool(whirlpoolPda.publicKey);
      const poolData = pool.getData();
      
      console.log("📊 Pool Information:");
      console.log("   Current Tick:", poolData.tickCurrentIndex);
      console.log("   Current Sqrt Price:", poolData.sqrtPrice.toString());
      console.log("   Liquidity:", poolData.liquidity.toString());
      console.log("   Fee Rate:", poolData.feeRate);
      
      return {
        poolAddress: whirlpoolPda.publicKey,
        tokenA,
        tokenB,
        tickSpacing: TICK_SPACING,
      };
    }

    // Step 6: Create pool using Orca's official methods
    console.log("📋 Step 6: Pool creation guidance...");
    console.log("\n🏗️  To create the IRMA/USDT pool:");
    console.log("========================================");
    
    console.log("\n1. 📱 Use Orca's Web UI:");
    console.log("   - Visit: https://www.orca.so/pools");
    console.log("   - Connect wallet with", FUNDER.toString());
    console.log("   - Click 'Create Pool'");
    console.log("   - Token A:", tokenA.toString(), "(IRMA)");
    console.log("   - Token B:", tokenB.toString(), "(USDT)");
    console.log("   - Set initial price: 1.0");
    console.log("   - Fee tier: 0.01%");
    
    console.log("\n2. 🔧 Use Orca SDK programmatically:");
    console.log("   - Install: npm install @orca-so/whirlpools-sdk");
    console.log("   - Use WhirlpoolIx.initializePoolIx()");
    console.log("   - Requires proper FeeTier setup first");
    
    console.log("\n3. 📦 Manual transaction approach:");
    console.log("   - Create FeeTier for tick spacing", TICK_SPACING);
    console.log("   - Initialize Whirlpool with sqrt price:", initialSqrtPrice.toString());
    console.log("   - Initialize TickArrays for trading");
    console.log("   - Set up token vaults");

    console.log("\n📝 Pool Configuration:");
    console.log("   Whirlpool PDA:", whirlpoolPda.publicKey.toString());
    console.log("   WhirlpoolsConfig:", whirlpoolsConfig.toString());
    console.log("   Token A (IRMA):", tokenA.toString());
    console.log("   Token B (USDT):", tokenB.toString());
    console.log("   Tick Spacing:", TICK_SPACING);
    console.log("   Fee Rate:", FEE_RATE, "basis points");
    console.log("   Initial Price: 1 IRMA = 1 USDT");
    console.log("   Funder:", FUNDER.toString());

    // Get additional PDAs for reference
    const feeTierPda = PDAUtil.getFeeTier(
      ORCA_WHIRLPOOL_PROGRAM_ID,
      whirlpoolsConfig,
      TICK_SPACING
    );

    const oraclePda = PDAUtil.getOracle(
      ORCA_WHIRLPOOL_PROGRAM_ID,
      whirlpoolPda.publicKey
    );

    console.log("\n🔗 Required PDAs:");
    console.log("   FeeTier PDA:", feeTierPda.publicKey.toString());
    console.log("   Oracle PDA:", oraclePda.publicKey.toString());

    console.log("\n⚠️  Important Notes:");
    console.log("   - Pool creation requires multiple transactions");
    console.log("   - Ensure both tokens exist on devnet");
    console.log("   - Use proper fee tier (0.01% = 100 basis points)");
    console.log("   - Initialize with sufficient liquidity for trading");

    return {
      poolAddress: whirlpoolPda.publicKey,
      tokenA,
      tokenB,
      tickSpacing: TICK_SPACING,
      feeTierPda: feeTierPda.publicKey,
      oraclePda: oraclePda.publicKey,
    };

  } catch (error) {
    console.error("❌ Error:", error);
    if (error instanceof Error) {
      console.error("Error message:", error.message);
    }
    throw error;
  }
}

// Helper function to check token mints
async function checkTokenMints() {
  const connection = new Connection(DEVNET_RPC_URL, "confirmed");
  
  console.log("🔍 Checking token mints on devnet...");
  
  try {
    const irmaMintInfo = await connection.getAccountInfo(IRMA_MINT);
    const usdtMintInfo = await connection.getAccountInfo(USDT_MINT);
    
    console.log("IRMA mint exists:", irmaMintInfo !== null);
    console.log("USDT mint exists:", usdtMintInfo !== null);
    
    if (irmaMintInfo) {
      console.log("✅ IRMA mint found:", IRMA_MINT.toString());
    } else {
      console.log("❌ IRMA mint not found. May need to deploy to devnet first.");
    }
    
    if (usdtMintInfo) {
      console.log("✅ USDT mint found:", USDT_MINT.toString());
    } else {
      console.log("❌ USDT mint not found. May need devnet version or mock token.");
    }

    return {
      irmaExists: irmaMintInfo !== null,
      usdtExists: usdtMintInfo !== null
    };
  } catch (error) {
    console.error("Error checking mints:", error);
    return { irmaExists: false, usdtExists: false };
  }
}

// Function to get existing pool info
async function getPoolInfo() {
  const connection = new Connection(DEVNET_RPC_URL, "confirmed");
  const provider = new AnchorProvider(
    connection, 
    new Wallet(Keypair.generate()), 
    AnchorProvider.defaultOptions()
  );
  
  const ctx = WhirlpoolContext.withProvider(provider, ORCA_WHIRLPOOL_PROGRAM_ID);
  const client = buildWhirlpoolClient(ctx);

  const whirlpoolsConfig = new PublicKey("2LecshUwdy9xi7meFgHtFJQNSKk4KdTrcpvaB56dP2NQ");
  
  const tokenMints = PoolUtil.orderMints(IRMA_MINT, USDT_MINT);
  const tokenA = new PublicKey(tokenMints[0]);
  const tokenB = new PublicKey(tokenMints[1]);
  
  const whirlpoolPda = PDAUtil.getWhirlpool(
    ORCA_WHIRLPOOL_PROGRAM_ID,
    whirlpoolsConfig,
    tokenA,
    tokenB,
    TICK_SPACING
  );

  try {
    const pool = await client.getPool(whirlpoolPda.publicKey);
    const poolData = pool.getData();
    
    console.log("🏊 Pool Found!");
    console.log("Pool Address:", whirlpoolPda.publicKey.toString());
    console.log("Current Price:", PriceMath.sqrtPriceX64ToPrice(poolData.sqrtPrice, 6, 6).toString());
    console.log("Liquidity:", poolData.liquidity.toString());
    console.log("Fee Rate:", poolData.feeRate);
    
    return {
      address: whirlpoolPda.publicKey,
      data: poolData,
      exists: true
    };
  } catch (error) {
    console.log("ℹ️  Pool not found - needs to be created");
    return {
      address: whirlpoolPda.publicKey,
      exists: false
    };
  }
}

// Main execution
async function main() {
  console.log("🚀 IRMA/USDT Pool Setup Script");
  console.log("==============================\n");
  
  // Check token mints
  const mintCheck = await checkTokenMints();
  
  if (!mintCheck.irmaExists || !mintCheck.usdtExists) {
    console.log("\n⚠️  One or both tokens don't exist on devnet.");
    console.log("   You may need to deploy them first or use different addresses.");
  }
  
  console.log("\n" + "=".repeat(50) + "\n");
  
  // Check existing pool
  console.log("🔍 Checking for existing pool...");
  const poolInfo = await getPoolInfo();
  
  if (poolInfo.exists) {
    console.log("✅ Pool already exists! Address:", poolInfo.address.toString());
    return poolInfo;
  }
  
  console.log("\n" + "=".repeat(50) + "\n");
  
  // Show pool creation guidance
  const poolSetup = await createIrmaUsdtPool();
  
  console.log("\n🎉 Pool setup information ready!");
  console.log("Use the addresses above to create the pool through Orca's interface.");
  
  return poolSetup;
}

// Export functions for use in other scripts
export {
  createIrmaUsdtPool,
  checkTokenMints,
  getPoolInfo,
  IRMA_MINT,
  USDT_MINT,
  FUNDER,
  TICK_SPACING,
  FEE_RATE
};

// Run if called directly
if (require.main === module) {
  main().catch((error) => {
    console.error("❌ Script failed:", error);
    process.exit(1);
  });
}