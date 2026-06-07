// Seed Initial Mint-Position Liquidity into IRMA DLMM Pools on Meteora Devnet
//
// The IRMA on-chain program manages two single-bin positions per pool:
//   - Mint position: a single bin holding only IRMA (X), at the mint price.
//     This is the position we open here — it's the only one our script should create.
//   - Redemption position: a single bin holding only the stablecoin (Y), at the
//     redemption price. The IRMA program opens this itself after the first swap —
//     we must NOT create it.
//
// For each of the 6 pools this script:
//   1. Mints extra IRMA into our wallet (if we hold mint authority)
//   2. Opens a new single-bin position at the active bin (price = 1.0)
//   3. Deposits SEED_AMOUNT of IRMA only (no stablecoin) into that position
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
const os = require("os");
const path = require("path");
require("dotenv").config({ path: path.join(__dirname, "../.env") });

const config = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../devnet-config.json"), "utf-8")
);

// 10 IRMA (6 decimals = 10_000_000 base units) for the mint position
const SEED_AMOUNT = new BN(10_000_000);

// Tokens we hold mint authority for — we can self-mint these
const CAN_MINT = new Set(["irma", "usdt", "pyusd", "usds", "usdg", "fdusd"]);

async function ensureFunded(connection, keypair, symbol) {
  const token = config.tokens[symbol];
  const mint = new PublicKey(token.mint);
  const programId =
    token.program === TOKEN_2022_PROGRAM_ID.toBase58()
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

  const keypairPath =
    process.env.SOLANA_KEYPAIR_PATH ||
    path.join(os.homedir(), ".config/solana/phantom1.json");
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
      // Step 1 — ensure wallet has enough IRMA (mint position holds only IRMA)
      console.log("   Funding wallet...");
      const irmaOk = await ensureFunded(connection, keypair, "irma");
      if (!irmaOk) {
        failed.push(symbol);
        continue;
      }

      // Step 2 — load pool and get active bin (this is the mint price, bin = 0)
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

      // Step 4 — open a single-bin mint position at the active bin, IRMA (X) only.
      // The redemption position (stablecoin/Y, single bin below) is opened by the
      // IRMA on-chain program itself after the first swap — we must not create it.
      const result = await dlmmPool.initializePositionAndAddLiquidityByStrategy({
        positionPubKey: positionKeypair.publicKey,
        user: keypair.publicKey,
        totalXAmount: SEED_AMOUNT,
        totalYAmount: new BN(0),
        strategy: {
          minBinId: activeBin.binId,
          maxBinId: activeBin.binId,
          strategyType: StrategyType.Spot,
        },
      });

      // The SDK can return either a single Transaction or an array of them
      const txs = Array.isArray(result) ? result : [result];

      let sig;
      for (const tx of txs) {
        const { blockhash } = await connection.getLatestBlockhash();
        tx.feePayer = keypair.publicKey;
        tx.recentBlockhash = blockhash;

        sig = await sendAndConfirmTransaction(
          connection, tx, [keypair, positionKeypair], { commitment: "confirmed" }
        );
      }

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
