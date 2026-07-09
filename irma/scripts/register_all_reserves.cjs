// register_all_reserves.cjs
// Register all stablecoins from devnet-config.json as reserves in our deployed program.

"use strict";

const { AnchorProvider, Program, Wallet } = require("@coral-xyz/anchor");
const { Connection, PublicKey, SystemProgram, Keypair } = require("@solana/web3.js");
const fs = require("fs");
const path = require("path");
require("dotenv").config({ path: path.join(__dirname, "../.env") });

const config = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../devnet-config.json"), "utf-8")
);

const idl = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../target/idl/irma.json"), "utf-8")
);

const PROGRAM_ID = new PublicKey(idl.address);

async function main() {
  const rpcUrl = process.env.ANCHOR_PROVIDER_URL || process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
  const commitment = "confirmed";
  
  console.log("\n🏦 Registering Reserves in IRMA Program");
  console.log("======================================");
  console.log("Program ID:", PROGRAM_ID.toBase58());

  const connection = new Connection(rpcUrl, commitment);
  
  const keypairPath =
    process.env.SOLANA_KEYPAIR_PATH ||
    path.join(require("os").homedir(), ".config/solana/id.json");
  
  const payer = Keypair.fromSecretKey(
    new Uint8Array(JSON.parse(fs.readFileSync(keypairPath, "utf-8")))
  );
  
  const wallet = new Wallet(payer);
  const provider = new AnchorProvider(connection, wallet, { commitment });
  const program = new Program(idl, provider);
  
  console.log("Admin Wallet:", payer.publicKey.toBase58());

  // Derive PDAs
  const [statePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("state_v5")],
    PROGRAM_ID
  );
  
  const [corePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("core_v5")],
    PROGRAM_ID
  );

  console.log("State PDA:", statePda.toBase58());
  console.log("Core PDA :", corePda.toBase58());

  // We want to register these symbols from config.tokens: usdc, usdt, pyusd, usds, usdg, fdusd
  const STABLECOINS = ["usdc", "usdt", "pyusd", "usds", "usdg", "fdusd"];

  for (const symbol of STABLECOINS) {
    const token = config.tokens[symbol];
    if (!token) {
      console.log(`⚠️ No config entry found for ${symbol} — skipping.`);
      continue;
    }

    console.log(`\n🔹 Registering reserve for ${symbol.toUpperCase()} (${token.name})...`);
    console.log(`   Mint Address: ${token.mint}`);
    console.log(`   Decimals    : ${token.decimals}`);

    try {
      const tx = await program.methods
        .addReserve(token.name, new PublicKey(token.mint), token.decimals)
        .accounts({
          state: statePda,
          irmaAdmin: payer.publicKey,
          core: corePda,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      console.log(`   ✅ Success! Tx: ${tx}`);
    } catch (err) {
      const msg = err?.message ?? String(err);
      if (msg.includes("ReserveAlreadyExists") || msg.includes("0x1774") || msg.includes("already")) {
        console.log(`   ℹ️ Reserve already exists in program.`);
      } else {
        console.error(`   ❌ Failed: ${msg}`);
      }
    }
  }

  // Fetch and show current reserves state
  console.log("\n📖 Fetching updated protocol state...");
  const state = await program.account.stateMap.fetch(statePda);
  console.log("Current registered reserves in state:");
  console.log(JSON.stringify(state.reserves, null, 2));
}

main().catch((err) => {
  console.error("❌ Fatal error:", err);
  process.exit(1);
});
