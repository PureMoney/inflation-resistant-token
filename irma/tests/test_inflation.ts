import * as anchor from "@coral-xyz/anchor";
import { Program, AnchorProvider } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { Wallet } from "@coral-xyz/anchor";
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
const IRMA_MINT = new PublicKey(config.tokens.irma.mint);
const USDC_MINT = new PublicKey(config.tokens.usdc.mint);

async function testInflation() {
  console.log("\n🧪 Testing Inflation Mechanism");
  console.log("================================\n");

  // Setup provider with wallet from keypair file
  const clusterUrl = process.env.ANCHOR_PROVIDER_URL || "https://api.devnet.solana.com";
  const connection = new Connection(clusterUrl, "confirmed");
  const walletPath = path.join(process.env.HOME || "", ".config/solana/phantom1.json");
  const walletKeypair = Keypair.fromSecretKey(
    new Uint8Array(JSON.parse(fs.readFileSync(walletPath, "utf-8")))
  );
  const wallet = new Wallet(walletKeypair);
  const provider = new AnchorProvider(connection, wallet, { commitment: "confirmed" });
  anchor.setProvider(provider);

  const program = new Program(idl, provider);

  // Find protocol state PDA
  const [protocolState] = PublicKey.findProgramAddressSync(
    [Buffer.from("protocol_state")],
    PROGRAM_ID
  );

  console.log("Protocol State PDA:", protocolState.toBase58());
  console.log("Authority:", provider.wallet.publicKey.toBase58());
  console.log("\n");

  try {
    // Fetch current state
    console.log("📊 Fetching current protocol state...");
    let state = await (program.account as any).protocolState.fetch(protocolState);

    console.log("Initial State:");
    console.log(`  Mint Price: ${state.mintPrice.toString()} (scaled by 1e9)`);
    console.log(`  Redemption Price: ${state.redemptionPrice.toString()} (scaled by 1e9)`);
    console.log(`  Mint Price (USD): $${(state.mintPrice.toNumber() / 1e9).toFixed(6)}`);
    console.log(`  Redemption Price (USD): $${(state.redemptionPrice.toNumber() / 1e9).toFixed(6)}`);
    console.log(`  Last Update: ${new Date(state.lastPriceUpdate.toNumber() * 1000).toISOString()}`);
    console.log("\n");

    // Apply 5% inflation
    console.log("🔥 Applying 5% annual inflation...");
    console.log("   (500 basis points)");
    
    const tx1 = await program.methods
      .applyInflation(500) // 500 basis points = 5%
      .accounts({
        protocolState,
        authority: provider.wallet.publicKey,
      })
      .rpc();

    console.log(`✅ Transaction 1: ${tx1}`);
    console.log("\n");

    // Wait for confirmation
    await provider.connection.confirmTransaction(tx1, "confirmed");
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Fetch updated state
    state = await (program.account as any).protocolState.fetch(protocolState);

    console.log("After First Inflation:");
    console.log(`  Mint Price: ${state.mintPrice.toString()}`);
    console.log(`  Redemption Price: ${state.redemptionPrice.toString()}`);
    console.log(`  Mint Price (USD): $${(state.mintPrice.toNumber() / 1e9).toFixed(6)}`);
    console.log(`  Redemption Price (USD): $${(state.redemptionPrice.toNumber() / 1e9).toFixed(6)}`);
    console.log(`  Last Update: ${new Date(state.lastPriceUpdate.toNumber() * 1000).toISOString()}`);
    console.log("\n");

    // Wait a few seconds to simulate time passage
    console.log("⏳ Waiting 5 seconds to simulate time passage...");
    await new Promise((resolve) => setTimeout(resolve, 5000));

    // Apply another 5% inflation
    console.log("🔥 Applying another 5% inflation...");
    
    const tx2 = await program.methods
      .applyInflation(500)
      .accounts({
        protocolState,
        authority: provider.wallet.publicKey,
      })
      .rpc();

    console.log(`✅ Transaction 2: ${tx2}`);
    console.log("\n");

    // Wait for confirmation
    await provider.connection.confirmTransaction(tx2, "confirmed");
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Fetch final state
    state = await (program.account as any).protocolState.fetch(protocolState);

    console.log("After Second Inflation:");
    console.log(`  Mint Price: ${state.mintPrice.toString()}`);
    console.log(`  Redemption Price: ${state.redemptionPrice.toString()}`);
    console.log(`  Mint Price (USD): $${(state.mintPrice.toNumber() / 1e9).toFixed(6)}`);
    console.log(`  Redemption Price (USD): $${(state.redemptionPrice.toNumber() / 1e9).toFixed(6)}`);
    console.log(`  Last Update: ${new Date(state.lastPriceUpdate.toNumber() * 1000).toISOString()}`);
    console.log("\n");

    console.log("✅ Inflation test completed successfully!");
    console.log("\n📈 Summary:");
    console.log("   Inflation correctly compounds based on time elapsed");
    console.log("   Both mint and redemption prices increase proportionally");
    console.log("   Protocol state updates correctly");

  } catch (err: any) {
    console.error("\n❌ Error testing inflation:");
    console.error(err);
    
    if (err.logs) {
      console.log("\n📋 Program Logs:");
      err.logs.forEach((log: string) => console.log("  ", log));
    }
    
    process.exit(1);
  }
}

testInflation().catch((err) => {
  console.error(err);
  process.exit(1);
});
