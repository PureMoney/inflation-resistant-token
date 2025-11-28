import { AccountClient, AnchorProvider, Program } from "@coral-xyz/anchor";
import { Connection, PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Load config
const config = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../devnet-config.json"), "utf-8")
);

// Load IDL
const idl = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../target/idl/irma.json"), "utf-8")
);

const PROGRAM_ID = new PublicKey(config.program.programId);

async function readProtocolState() {
  console.log("\n📖 Reading IRMA Protocol State (Read-Only)");
  console.log("==========================================\n");

  // Connect to devnet WITHOUT wallet (read-only mode)
  const clusterUrl = process.env.ANCHOR_PROVIDER_URL || "https://api.devnet.solana.com";
  const connection = new Connection(clusterUrl, "confirmed");
  
  // Create a dummy provider for reading (doesn't need wallet)
  const provider = new AnchorProvider(connection, null as any, { commitment: "confirmed" });
  const program = new Program(idl, provider);

  // Find protocol state PDA
  const [core] = PublicKey.findProgramAddressSync(
    [Buffer.from("core")],
    PROGRAM_ID
  );

  console.log("📍 Protocol State PDA:");
  console.log(`   ${core.toBase58()}\n`);

  try {
    // Fetch account info without signing anything
    console.log("🔍 Fetching protocol state data...\n");
    console.log("🔗 Try to display program: ", program);
    
    const state = await (program.account as any).core.fetch(core.toBase58());

    console.log("✅ Protocol state data fetched successfully.\n");
    console.log("📊 Protocol State Data:", state);

    // const mintPrice = Number(state.mintPrice) / 1_000_000_000;
    // const redemptionPrice = Number(state.redemptionPrice) / 1_000_000_000;
    // const lastUpdate = new Date(Number(state.lastPriceUpdate) * 1000);
    // const lastRebalance = new Date(Number(state.lastRebalance) * 1000);

    // console.log("✅ Protocol State Data:");
    // console.log("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    // console.log(`  Authority: ${state.authority.toBase58()}`);
    // console.log(`  Mint Price: $${mintPrice.toFixed(6)} USDC`);
    // console.log(`  Redemption Price: $${redemptionPrice.toFixed(6)} USDC`);
    // console.log(`  Mint Price (raw): ${state.mintPrice}`);
    // console.log(`  Redemption Price (raw): ${state.redemptionPrice}`);
    // console.log(`  Last Price Update: ${lastUpdate.toISOString()}`);
    // console.log(`  Last Rebalance: ${lastRebalance.toISOString()}`);
    // console.log(`  Whirlpool: ${state.whirlpool.toBase58()}`);
    // console.log(`  Position: ${state.position.toBase58()}`);
    // console.log(`  Token A Mint: ${state.tokenAMint.toBase58()}`);
    // console.log(`  Token B Mint: ${state.tokenBMint.toBase58()}`);
    // console.log(`  Bump: ${state.bump}`);
    // console.log("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // Calculate time since last update
    const now = Math.floor(Date.now() / 1000);
    const timeSinceUpdate = now - Number(state.lastPriceUpdate);
    const hours = Math.floor(timeSinceUpdate / 3600);
    const minutes = Math.floor((timeSinceUpdate % 3600) / 60);
    const seconds = timeSinceUpdate % 60;

    console.log("⏱️  Time Information:");
    console.log(`  Time since last update: ${hours}h ${minutes}m ${seconds}s`);
    console.log(`  Current timestamp: ${now}`);
    console.log(`  Last update timestamp: ${Number(state.lastPriceUpdate)}\n`);

    // Get account size
    const accountInfo = connection.getAccountInfo(core);
    if (accountInfo) {
      console.log("💾 Account Information:");
      console.log(`  Data Size: ${accountInfo.data.length} bytes`);
      console.log(`  Owner: ${accountInfo.owner.toBase58()}`);
      console.log(`  Executable: ${accountInfo.executable}`);
      console.log(`  Lamports (SOL): ${accountInfo.lamports / 1e9} SOL\n`);
    }

    console.log("✅ Successfully read protocol state!");
    console.log("ℹ️  This is a read-only operation. No wallet required.\n");

    return state;

  } catch (error: any) {
    console.error("❌ Error reading protocol state:");
    console.error(error.message);
    
    if (error.message.includes("Account does not exist")) {
      console.log("\n💡 Hint: Protocol state hasn't been initialized yet.");
      console.log("   Run this command with your wallet:");
      console.log("   npx ts-node scripts/initialize_protocol_with_position.ts\n");
    }
    
    throw error;
  }
}

// Run the function
readProtocolState().catch(console.error);
