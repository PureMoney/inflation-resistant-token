import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import BN from "bn.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function testMintPriceInflation() {
  console.log("\n💰 Testing Mint Price & Inflation");
  console.log("==================================\n");

  // Load config
  const configPath = path.join(__dirname, "../devnet-config.json");
  const config = JSON.parse(fs.readFileSync(configPath, "utf-8"));

  // Setup
  const clusterUrl = process.env.ANCHOR_PROVIDER_URL || "https://api.devnet.solana.com";
  const connection = new Connection(clusterUrl, "confirmed");
  const walletPath = path.join(process.env.HOME || "", ".config/solana/phantom1.json");
  const walletKeypair = Keypair.fromSecretKey(
    new Uint8Array(JSON.parse(fs.readFileSync(walletPath, "utf-8")))
  );
  const wallet = new Wallet(walletKeypair);
  const provider = new AnchorProvider(connection, wallet, { commitment: "confirmed" });

  // Load program
  const idlPath = path.join(__dirname, "../target/idl/irma.json");
  const idl = JSON.parse(fs.readFileSync(idlPath, "utf-8"));
  const program = new Program(idl, provider);

  // Get protocol state PDA
  const [protocolState] = PublicKey.findProgramAddressSync(
    [Buffer.from("protocol_state")],
    program.programId
  );

  console.log(`Protocol State: ${protocolState.toBase58()}\n`);

  try {
    // Step 1: Read current state
    console.log("📖 Step 1: Reading current protocol state...\n");
    const state1 = await (program.account as any).protocolState.fetch(protocolState);

    const mintPrice1 = Number(state1.mintPrice);
    const redemptionPrice1 = Number(state1.redemptionPrice);
    const lastUpdate1 = new Date(Number(state1.lastPriceUpdate) * 1000);

    console.log("BEFORE Inflation:");
    console.log(`  Mint Price: ${(mintPrice1 / 1_000_000_000).toFixed(6)} USDC (raw: ${mintPrice1})`);
    console.log(`  Redemption Price: ${(redemptionPrice1 / 1_000_000_000).toFixed(6)} USDC`);
    console.log(`  Spread: ${((mintPrice1 - redemptionPrice1) / redemptionPrice1 * 100).toFixed(2)}%`);
    console.log(`  Last Update: ${lastUpdate1.toISOString()}\n`);

    // Step 2: Apply 5% inflation (500 basis points)
    console.log("📊 Step 2: Applying 5% inflation (500 bps)...\n");
    const inflationRate = new BN(500); // 5% = 500 bps

    const tx = await program.methods
      .applyInflation(inflationRate)
      .accounts({
        protocolState: protocolState,
        authority: wallet.publicKey,
      })
      .rpc();

    console.log(`✅ Transaction: ${tx}\n`);

    // Wait a bit for confirmation
    await new Promise((resolve) => setTimeout(resolve, 3000));

    // Step 3: Read updated state
    console.log("📖 Step 3: Reading updated protocol state...\n");
    const state2 = await (program.account as any).protocolState.fetch(protocolState);

    const mintPrice2 = Number(state2.mintPrice);
    const redemptionPrice2 = Number(state2.redemptionPrice);
    const lastUpdate2 = new Date(Number(state2.lastPriceUpdate) * 1000);

    console.log("AFTER Inflation:");
    console.log(`  Mint Price: ${(mintPrice2 / 1_000_000_000).toFixed(6)} USDC (raw: ${mintPrice2})`);
    console.log(`  Redemption Price: ${(redemptionPrice2 / 1_000_000_000).toFixed(6)} USDC`);
    console.log(`  Spread: ${((mintPrice2 - redemptionPrice2) / redemptionPrice2 * 100).toFixed(2)}%`);
    console.log(`  Last Update: ${lastUpdate2.toISOString()}\n`);

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
      console.log(`  ✓ Mint price increased (${(mintPrice2 - mintPrice1) / 1_000_000_000} USDC)`);
    } else {
      console.log(`  ✗ ERROR: Mint price did not increase!`);
    }

    if (redemptionPrice2 > redemptionPrice1) {
      console.log(`  ✓ Redemption price increased (${(redemptionPrice2 - redemptionPrice1) / 1_000_000_000} USDC)`);
    } else {
      console.log(`  ✗ ERROR: Redemption price did not increase!`);
    }

    if (lastUpdate2 > lastUpdate1) {
      console.log(`  ✓ Timestamp updated`);
    } else {
      console.log(`  ✗ ERROR: Timestamp not updated!`);
    }

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

testMintPriceInflation().catch((err) => {
  console.error(err);
  process.exit(1);
});
