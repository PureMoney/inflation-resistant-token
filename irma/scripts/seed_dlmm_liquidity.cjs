// Seed Initial Liquidity into IRMA DLMM Pools on Meteora Devnet
//
// For each of the 6 pools this script:
//   1. Mints extra tokens into our wallet where we hold mint authority
//   2. Opens a new position centered on the active bin (price = 1.0)
//   3. Deposits SEED_AMOUNT of IRMA + stablecoin into that position
//
// Run from irma/ directory:
//   node scripts/seed_dlmm_liquidity.cjs

"use strict";

const DLMM = require("@meteora-ag/dlmm");
const { StrategyType } = DLMM;
const {
  Connection,
  Keypair,
  PublicKey,
  sendAndConfirmTransaction,
} = require("@solana/web3.js");
const {
  getOrCreateAssociatedTokenAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
} = require("@solana/spl-token");
const BN = require("bn.js");
const fs = require("fs");
const path = require("path");
require("dotenv").config({ path: path.join(__dirname, "../.env") });

const config = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../devnet-config.json"), "utf-8")
);

// 10 tokens each side (6 decimals = 10_000_000 base units)
const SEED_AMOUNT = new BN(10_000_000);

// Spread across 10 bins on each side of active price (-10 to +10)
const BIN_RANGE = 10;

// Tokens we hold mint authority for — we can self-mint these
const CAN_MINT = new Set(["irma", "usdt", "pyusd", "usds", "usdg", "fdusd"]);

async function ensureFunded(connection, keypair, symbol) {
  const token = config.tokens[symbol];
  const mint = new PublicKey(token.mint);
  const programId =
    token.program === "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
      ? TOKEN_2022_PROGRAM_ID
      : TOKEN_PROGRAM_ID;

  const ata = await getOrCreateAssociatedTokenAccount(
    connection, keypair, mint, keypair.publicKey,
    false, "confirmed", {}, programId
  );

  const need = BigInt(SEED_AMOUNT.toString()); // need at least SEED_AMOUNT
  if (ata.amount < need) {
    if (!CAN_MINT.has(symbol)) {
      console.log(`   ⚠️  ${symbol.toUpperCase()}: balance ${ata.amount} < ${need} — not mint authority, skipping`);
      return null;
    }
    const toMint = need - ata.amount;
    await mintTo(connection, keypair, mint, ata.address, keypair, toMint, [], {}, programId);
    console.log(`   ✅ Minted ${toMint} base units of ${symbol.toUpperCase()}`);
  } else {
    console.log(`   ✅ ${symbol.toUpperCase()}: balance OK (${ata.amount} units)`);
  }
  return ata.address;
}

async function main() {
  const rpcUrl =
    process.env.ANCHOR_PROVIDER_URL ||
    process.env.SOLANA_RPC_URL ||
    "https://api.devnet.solana.com";

  const connection = new Connection(rpcUrl, "confirmed");

  const keypairPath = path.join(process.env.HOME, ".config/solana/phantom1.json");
  const keypair = Keypair.fromSecretKey(
    new Uint8Array(JSON.parse(fs.readFileSync(keypairPath, "utf-8")))
  );

  console.log("\n💧 Seeding Liquidity into IRMA DLMM Pools");
  console.log("==========================================");
  console.log("Wallet :", keypair.publicKey.toBase58());
  const solBalance = await connection.getBalance(keypair.publicKey);
  console.log("Balance:", solBalance / 1e9, "SOL\n");

  if (solBalance < 0.5e9) {
    throw new Error("Need at least 0.5 SOL.");
  }

  const seeded = [];
  const failed = [];

  for (const symbol of ["usdc", "usdt", "pyusd", "usds", "usdg", "fdusd"]) {
    const poolCfg = config.pools?.[symbol];
    const tokenCfg = config.tokens[symbol];
    if (!poolCfg) {
      console.warn(`⚠️  No pool config for ${symbol} — skipping.`);
      continue;
    }

    console.log(`\n📦 IRMA / ${tokenCfg.name}`);
    console.log(`   Pool: ${poolCfg.address}`);

    try {
      // Step 1 — ensure wallet has enough of both tokens
      console.log("   Funding wallet...");
      const irmaOk  = await ensureFunded(connection, keypair, "irma");
      const stableOk = await ensureFunded(connection, keypair, symbol);
      if (!irmaOk || !stableOk) {
        failed.push(symbol);
        continue;
      }

      // Step 2 — load pool and get active bin
      const dlmmPool = await DLMM.create(
        connection,
        new PublicKey(poolCfg.address),
        { cluster: "devnet" }
      );
      const activeBin = await dlmmPool.getActiveBin();
      console.log(`   Active bin: ${activeBin.binId}  price: ${activeBin.pricePerToken}`);

      // Step 3 — generate a fresh keypair for the position account
      const positionKeypair = Keypair.generate();
      console.log(`   Position  : ${positionKeypair.publicKey.toBase58()}`);

      // Step 4 — build the tx: open position + add liquidity in one shot
      const tx = await dlmmPool.initializePositionAndAddLiquidityByStrategy({
        positionPubKey: positionKeypair.publicKey,
        user: keypair.publicKey,
        totalXAmount: SEED_AMOUNT,
        totalYAmount: SEED_AMOUNT,
        strategy: {
          minBinId: activeBin.binId - BIN_RANGE,
          maxBinId: activeBin.binId + BIN_RANGE,
          strategyType: StrategyType.Spot,
        },
      });

      const { blockhash } = await connection.getLatestBlockhash();
      tx.feePayer = keypair.publicKey;
      tx.recentBlockhash = blockhash;

      const sig = await sendAndConfirmTransaction(
        connection, tx, [keypair, positionKeypair], { commitment: "confirmed" }
      );

      console.log(`   ✅ Done. Tx: ${sig}`);
      seeded.push({ symbol, sig });

    } catch (err) {
      console.error(`   ❌ Failed: ${err?.message ?? err}`);
      failed.push(symbol);
    }
  }

  console.log("\n📋 Summary");
  console.log("==========");
  for (const { symbol, sig } of seeded) {
    console.log(`✅ IRMA/${symbol.toUpperCase().padEnd(6)} seeded — ${sig}`);
  }
  for (const symbol of failed) {
    console.log(`❌ IRMA/${symbol.toUpperCase().padEnd(6)} failed`);
  }
  console.log(`\n${seeded.length}/6 pools seeded.`);
}

main().catch((err) => {
  console.error("\n❌ Fatal error:", err);
  process.exit(1);
});
