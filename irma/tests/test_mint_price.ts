import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import { Connection, PublicKey, SystemProgram, Keypair } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import dotenv from "dotenv";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Load environment variables from the main .env file
dotenv.config({ path: path.join(__dirname, "../.env") });

// Load config
const config = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../devnet-config.json"), "utf-8")
);

// Load IDL
const idl = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../target/idl/irma.json"), "utf-8")
);

// Use program ID from IDL (which should match the declare_id! in Rust code)
const PROGRAM_ID = new PublicKey(idl.address);

console.log("🆔 Using Program ID from IDL:", PROGRAM_ID.toBase58());

// Use environment variables from .env file
const rpcUrl = process.env.ANCHOR_PROVIDER_URL || process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
const commitment = (process.env.ANCHOR_COMMITMENT || process.env.SOLANA_COMMITMENT || "confirmed") as any;

console.log("🌐 Using RPC URL:", rpcUrl);
console.log("🔒 Using commitment:", commitment);

// Create connection and provider manually using environment variables
const connection = new Connection(rpcUrl, commitment);

// Create or load a keypair for testing
let keypair: Keypair;

// Try to load from environment variable SOLANA_PRIVATE_KEY (base58 encoded)
if (process.env.SOLANA_PRIVATE_KEY) {
  try {
    const privateKeyArray = JSON.parse(process.env.SOLANA_PRIVATE_KEY);
    keypair = Keypair.fromSecretKey(new Uint8Array(privateKeyArray));
    console.log("🔑 Loaded keypair from SOLANA_PRIVATE_KEY environment variable");
  } catch (error) {
    console.log("❌ Failed to parse SOLANA_PRIVATE_KEY, generating new keypair");
    keypair = Keypair.generate();
  }
} else {
  // Generate a new keypair for testing
  keypair = Keypair.generate();
  console.log("🔑 Generated new test keypair");
  console.log("💡 To use a persistent wallet, set SOLANA_PRIVATE_KEY environment variable");
  console.log(`   Example: export SOLANA_PRIVATE_KEY='[${Array.from(keypair.secretKey).join(',')}]'`);
}

const wallet = new Wallet(keypair);
const provider = new AnchorProvider(connection, wallet, { commitment });
const program = new Program(idl, provider);

const payer = provider.wallet.publicKey;
console.log("👤 Using wallet public key:", payer.toBase58());

// Check current balance
const balance = await connection.getBalance(payer);
console.log("💰 Current balance:", balance / 1e9, "SOL");

if (balance < 1e9) { // Less than 1 SOL
  console.log("⚠️ Low balance detected. You need to fund this wallet.");
  console.log(`💸 Run: solana airdrop 2 ${payer.toBase58()} --url devnet`);
  console.log("   Or fund it manually from a faucet or another wallet");
  
  // Don't proceed without funds
  throw new Error("Insufficient funds. Please fund the wallet and try again.");
}

// Derive PDAs
const [statePda] = PublicKey.findProgramAddressSync(
  [Buffer.from("state_v5")],
  PROGRAM_ID
);

const [corePda] = PublicKey.findProgramAddressSync(
  [Buffer.from("core_v5")],
  PROGRAM_ID
);

// End of Prelude stuff
//----------------------------------------------------------------------
// Begin Test: Mint Price Inflation Adjustment
async function testMintPriceInflation(reserveSymbol: string = "devUSDC") {

  console.log("\n🚀 Set IRMA mint price for '${reserveSymbol}'");
  console.log("  (Simulated inflation adjustment).");
  console.log("===============================\n");

  console.log("📍 Derived PDAs:");
  console.log(`   State PDA: ${statePda.toBase58()}`);
  console.log(`   Core PDA: ${corePda.toBase58()}\n`);

  try {
    // Step 1: Read current state
    console.log("📖 Step 1: Reading current protocol state...\n");
    const state1 = await (program.account as any).stateMap.fetch(statePda);
    console.log("📊 Current State Data:", JSON.stringify(state1, null, 2));

    let reserveState = state1.reserves.find((r: any) => r.symbol === reserveSymbol);

    const mintPrice1 = Number(reserveState.mintPrice);
    const redemptionPrice1 = Number(reserveState.backingReserves / reserveState.circulatingSupply);
    // const lastUpdate1 = new Date(Number(reserveState.lastPriceUpdate) * 1000);

    console.log("BEFORE Inflation:");
    console.log(`  Mint Price: ${mintPrice1.toFixed(6)} ${reserveSymbol} (raw: ${mintPrice1 * 1_000_000})`);
    console.log(`  Redemption Price: ${redemptionPrice1.toFixed(6)} ${reserveSymbol} (raw: ${redemptionPrice1 * 1_000_000})`);
    if (redemptionPrice1 > 0) {
      console.log(`  Spread: ${((mintPrice1 - redemptionPrice1) / redemptionPrice1 * 100).toFixed(2)}%`);
    }
    // const lastUpdate1 = new Date(Number(reserveState.lastPriceUpdate) * 1000);
    // console.log(`  Last Update: ${lastUpdate1.toISOString()}\n`);

    // Step 2: Apply 5% inflation (500 basis points)
    console.log("📊 Step 2: Applying 5.23% inflation (523 bps)...\n");
    const totalInflationRate = Number(523); // 5.23% = 523 bps
    const inflationRate = totalInflationRate - Number(200); // Subtract 2% buffer = 323 bps
    const irmaPrice365daysAgo = Number(1_000_000); // 1 reserveToken in raw format
    const currentReserveOraclePrice = Number(999_300); // 1.0523 reserveToken in raw format

    console.log("Inflation Calculation:");
    console.log(`  IRMA Price 365 Days Ago: ${(irmaPrice365daysAgo / 1_000_000).toFixed(6)} ${reserveSymbol}`);
    console.log(`  Current ${reserveSymbol} Oracle Price: ${(currentReserveOraclePrice / 1_000_000).toFixed(6)} ${reserveSymbol}`);

    console.log(`  Total Inflation Rate: ${totalInflationRate} bps`);
    console.log(`  Applying Inflation Rate: ${inflationRate} bps\n`);

    const newPriceRaw = Math.floor(irmaPrice365daysAgo * (10_000 + inflationRate) / (10_000 * currentReserveOraclePrice) * 1_000_000);
    console.log(`  New Mint Price to Set: ${(newPriceRaw / 1_000_000).toFixed(6)} ${reserveSymbol} (raw: ${newPriceRaw})\n`);
    const newPrice = (newPriceRaw / 1_000_000).toFixed(6);

    const tx = await program.methods
      .setMintPrice(reserveSymbol, newPrice)
      .accounts({
        state: statePda,
        irmaAdmin: wallet.publicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    console.log(`✅ Transaction: ${tx}\n`);

    // Wait a bit for confirmation
    await new Promise((resolve) => setTimeout(resolve, 3000));

    // Step 3: Read updated state
    console.log("📖 Step 3: Reading updated protocol state...\n");
    const state2 = await (program.account as any).stateMap.fetch(statePda);

    let reserveState2 = state2.reserves.find((r: any) => r.symbol === reserveSymbol);
    const mintPrice2 = Number(reserveState2.mintPrice);
    const redemptionPrice2 = Number(reserveState2.redemptionPrice);
    // const lastUpdate2 = new Date(Number(reserveState2.lastPriceUpdate) * 1000);

    console.log("AFTER Inflation:");
    console.log(`  Mint Price: ${(mintPrice2 / 1_000_000_000).toFixed(6)} ${reserveSymbol} (raw: ${mintPrice2})`);
    console.log(`  Redemption Price: ${(redemptionPrice2 / 1_000_000_000).toFixed(6)} ${reserveSymbol} (raw: ${redemptionPrice2})`);
    console.log(`  Spread: ${((mintPrice2 - redemptionPrice2) / redemptionPrice2 * 100).toFixed(2)}%`);
    // console.log(`  Last Update: ${lastUpdate2.toISOString()}\n`);

    // Calculate changes
    const mintPriceChange = ((mintPrice2 - mintPrice1) / mintPrice1) * 100;
    const redemptionPriceChange = ((redemptionPrice2 - redemptionPrice1) / redemptionPrice1) * 100;

    console.log("📈 Changes:");
    console.log(`  Mint Price: ${mintPriceChange > 0 ? "+" : ""}${mintPriceChange.toFixed(4)}%`);
    console.log(`  Redemption Price: ${redemptionPriceChange > 0 ? "+" : ""}${redemptionPriceChange.toFixed(4)}%`);
    console.log(`  Time Elapsed: ${(Number(state2.lastPriceUpdate) - Number(state1.lastPriceUpdate))} seconds\n`);

    // Verification
    console.log("✅ Verification:");
    if (mintPrice2 > mintPrice1) {
      console.log(`  ✓ Mint price increased (${(mintPrice2 - mintPrice1) / 1_000_000_000} ${reserveSymbol})`);
    } else {
      console.log(`  ✗ ERROR: Mint price did not increase!`);
    }

    if (redemptionPrice2 > redemptionPrice1) {
      console.log(`  ✓ Redemption price increased (${(redemptionPrice2 - redemptionPrice1) / 1_000_000_000} ${reserveSymbol})`);
    } else {
      console.log(`  ✗ ERROR: Redemption price did not increase!`);
    }

    // if (lastUpdate2 > lastUpdate1) {
    //   console.log(`  ✓ Timestamp updated`);
    // } else {
    //   console.log(`  ✗ ERROR: Timestamp not updated!`);
    // }

    console.log("\n🎉 Inflation test complete!\n");

  } catch (err: any) {
    console.error("❌ Error:");
    console.error(err.message);
    if (err.logs) {
      console.error("\nLogs:");
      err.logs.forEach((log: string) => console.error(`  ${log}`));
    }
    process.exit(1);
  }
}

testMintPriceInflation("devUSDT"); // .catch((err) => {
//   console.error(err);
//   process.exit(1);
// });
