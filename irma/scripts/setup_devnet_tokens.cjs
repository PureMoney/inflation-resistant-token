// setup_devnet_tokens.cjs
// Create new token mints where our wallet holds the mint authority, and mint initial balances.

"use strict";

const {
  Connection,
  Keypair,
  PublicKey,
} = require("@solana/web3.js");
const {
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
} = require("@solana/spl-token");
const fs = require("fs");
const path = require("path");
require("dotenv").config({ path: path.join(__dirname, "../.env") });

const configPath = path.join(__dirname, "../devnet-config.json");
const config = JSON.parse(fs.readFileSync(configPath, "utf-8"));

// 10 billion tokens of each (6 decimals)
const MINT_AMOUNT = 10000000000000000n; 

const TOKENS_TO_SETUP = [
  { symbol: "irma",  name: "IRMA",      programId: TOKEN_2022_PROGRAM_ID },
  { symbol: "usdc",  name: "devUSDC",   programId: TOKEN_PROGRAM_ID },
  { symbol: "usdt",  name: "devUSDT",   programId: TOKEN_PROGRAM_ID },
  { symbol: "pyusd", name: "devPYUSD",  programId: TOKEN_PROGRAM_ID },
  { symbol: "usds",  name: "devUSDS",   programId: TOKEN_PROGRAM_ID },
  { symbol: "usdg",  name: "devUSDG",   programId: TOKEN_PROGRAM_ID },
  { symbol: "fdusd", name: "devFDUSD",  programId: TOKEN_PROGRAM_ID },
];

async function main() {
  const rpcUrl =
    process.env.ANCHOR_PROVIDER_URL ||
    process.env.SOLANA_RPC_URL ||
    "https://api.devnet.solana.com";

  const connection = new Connection(rpcUrl, "confirmed");

  // Load keypair from env or default
  const keypairPath =
    process.env.SOLANA_KEYPAIR_PATH ||
    path.join(require("os").homedir(), ".config/solana/id.json");
  
  const payer = Keypair.fromSecretKey(
    new Uint8Array(JSON.parse(fs.readFileSync(keypairPath, "utf-8")))
  );

  console.log("\n🪙 Setting up Devnet Tokens");
  console.log("===========================");
  console.log("Payer Wallet:", payer.publicKey.toBase58());
  const balance = await connection.getBalance(payer.publicKey);
  console.log("Balance     :", balance / 1e9, "SOL\n");

  const keypairsDir = path.join(__dirname, "../keypairs");
  if (!fs.existsSync(keypairsDir)) {
    fs.mkdirSync(keypairsDir, { recursive: true });
    console.log(`📁 Created keypairs directory: ${keypairsDir}`);
  }

  for (const token of TOKENS_TO_SETUP) {
    console.log(`\n🔹 Setting up ${token.symbol.toUpperCase()} (${token.name})...`);

    const kpPath = path.join(keypairsDir, `${token.symbol}-mint.json`);
    let mintKeypair;
    if (fs.existsSync(kpPath)) {
      const kpData = JSON.parse(fs.readFileSync(kpPath, "utf-8"));
      mintKeypair = Keypair.fromSecretKey(new Uint8Array(kpData));
      console.log(`   Loaded existing keypair from ${kpPath}`);
    } else {
      mintKeypair = Keypair.generate();
      fs.writeFileSync(kpPath, JSON.stringify(Array.from(mintKeypair.secretKey)));
      console.log(`   Generated new keypair and saved to ${kpPath}`);
    }

    const mintPubkey = mintKeypair.publicKey;
    console.log(`   Mint address: ${mintPubkey.toBase58()}`);

    // Check if token already exists on-chain
    let accountInfo = await connection.getAccountInfo(mintPubkey);
    if (!accountInfo) {
      console.log(`   Token does not exist on-chain. Creating mint...`);
      await createMint(
        connection,
        payer,
        payer.publicKey,     // mint authority
        null,                // freeze authority
        6,                   // decimals
        mintKeypair,
        { commitment: "confirmed" },
        token.programId
      );
      console.log(`   ✅ Mint created successfully.`);
    } else {
      console.log(`   ℹ️ Mint already exists on-chain.`);
    }

    // Get or create Associated Token Account
    const ata = await getOrCreateAssociatedTokenAccount(
      connection,
      payer,
      mintPubkey,
      payer.publicKey,
      false,
      "confirmed",
      { commitment: "confirmed" },
      token.programId
    );
    console.log(`   ATA address: ${ata.address.toBase58()}`);
    console.log(`   Current balance: ${ata.amount} base units`);

    // Mint tokens if balance is low
    if (ata.amount < MINT_AMOUNT) {
      const amountToMint = MINT_AMOUNT - ata.amount;
      console.log(`   Minting ${amountToMint} base units...`);
      await mintTo(
        connection,
        payer,
        mintPubkey,
        ata.address,
        payer,
        amountToMint,
        [],
        { commitment: "confirmed" },
        token.programId
      );
      console.log(`   ✅ Minted successfully.`);
    } else {
      console.log(`   ✅ Balance is sufficient.`);
    }

    // Update devnet-config.json
    config.tokens[token.symbol] = {
      mint: mintPubkey.toBase58(),
      name: token.name,
      symbol: token.symbol.toUpperCase(),
      decimals: 6,
      program: token.programId.toBase58()
    };
  }

  // Update program ID to match the newly deployed one from Anchor.toml / IDL
  const idlPath = path.join(__dirname, "../target/idl/irma.json");
  if (fs.existsSync(idlPath)) {
    const idl = JSON.parse(fs.readFileSync(idlPath, "utf-8"));
    config.program.programId = idl.address;
    console.log(`\n🆔 Updated program ID in config to match IDL: ${idl.address}`);
  }

  fs.writeFileSync(configPath, JSON.stringify(config, null, 2));
  console.log("\n💾 devnet-config.json updated successfully.");
}

main().catch((err) => {
  console.error("❌ Fatal error in script:", err);
  process.exit(1);
});
