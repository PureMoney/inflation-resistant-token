import { AnchorProvider, Program } from "@coral-xyz/anchor";
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

  // Find protocol state PDA - try different seed combinations
  let protocolState: PublicKey | null = null;
  let accountInfo: any = null;
  
  // Try different possible PDA seeds
  const possibleSeeds = [
    [Buffer.from("protocol_state")],
    [Buffer.from("state")],
    [Buffer.from("irma")],
    [Buffer.from("state_map")],
  ];
  
  console.log("📍 Searching for Protocol State PDA...");
  
  for (const seeds of possibleSeeds) {
    const [pda] = PublicKey.findProgramAddressSync(seeds, PROGRAM_ID);
    const info = await connection.getAccountInfo(pda);
    
    console.log(`Checking PDA with seeds [${seeds.map(s => s.toString())}]: ${pda.toBase58()}`);
    
    if (info) {
      protocolState = pda;
      accountInfo = info;
      console.log(`✅ Found account at: ${pda.toBase58()}\n`);
      break;
    } else {
      console.log(`❌ No account found at: ${pda.toBase58()}`);
    }
  }
  
  if (!accountInfo) {
    console.log("\n❌ Protocol state account not found at any PDA address.");
    console.log("💡 The protocol might not be initialized yet.");
    console.log("💡 Try running the initialization instruction first.");
    return;
  }

  try {
    // Fetch account info without signing anything
    console.log("🔍 Fetching protocol state data...\n");
    
    // Debug: Check what accounts are available
    console.log("Available account types:", Object.keys(program.account));
    
    console.log("✅ Found protocol state account:");
    console.log(`  Data Size: ${accountInfo.data.length} bytes`);
    console.log(`  Owner: ${accountInfo.owner.toBase58()}`);
    console.log(`  Executable: ${accountInfo.executable}`);
    console.log(`  Lamports (SOL): ${accountInfo.lamports / 1e9} SOL\n`);
    
    // Try to decode using program account types
    let state;
    try {
      // Use stateMap since that's what we found in the IDL
      console.log("Attempting to decode using stateMap...");
      state = await (program.account as any).stateMap.fetch(protocolState);
    } catch (decodeError) {
      console.log("❌ Error decoding account data:", decodeError);
      console.log("Raw account data length:", accountInfo.data.length);
      return;
    }

    const mintPrice = Number(state.mintPrice) / 1_000_000_000;
    const redemptionPrice = Number(state.redemptionPrice) / 1_000_000_000;
    const lastUpdate = new Date(Number(state.lastPriceUpdate) * 1000);
    const lastRebalance = new Date(Number(state.lastRebalance) * 1000);

    console.log("✅ Protocol State Data:");
    console.log("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    console.log(`  Authority: ${state.authority.toBase58()}`);
    console.log(`  Mint Price: $${mintPrice.toFixed(6)} USDC`);
    console.log(`  Redemption Price: $${redemptionPrice.toFixed(6)} USDC`);
    console.log(`  Mint Price (raw): ${state.mintPrice}`);
    console.log(`  Redemption Price (raw): ${state.redemptionPrice}`);
    console.log(`  Last Price Update: ${lastUpdate.toISOString()}`);
    console.log(`  Last Rebalance: ${lastRebalance.toISOString()}`);
    console.log(`  Whirlpool: ${state.whirlpool.toBase58()}`);
    console.log(`  Position: ${state.position.toBase58()}`);
    console.log(`  Token A Mint: ${state.tokenAMint.toBase58()}`);
    console.log(`  Token B Mint: ${state.tokenBMint.toBase58()}`);
    console.log(`  Bump: ${state.bump}`);
    console.log("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

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

    // Get account size (we already have the data, but double-check)
    if (protocolState) {
      const accountData = await connection.getAccountInfo(protocolState);
      if (accountData) {
        console.log("💾 Account Information:");
        console.log(`  Data Size: ${accountData.data.length} bytes`);
        console.log(`  Owner: ${accountData.owner.toBase58()}`);
        console.log(`  Executable: ${accountData.executable}`);
        console.log(`  Lamports (SOL): ${accountData.lamports / 1e9} SOL\n`);
      }
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
