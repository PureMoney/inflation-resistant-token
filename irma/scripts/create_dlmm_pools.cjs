// Create IRMA DLMM Pools on Meteora Devnet
//
// Creates 6 DLMM pools pairing IRMA with each backing stablecoin:
//   IRMA/USDC, IRMA/USDT, IRMA/devPYUSD, IRMA/devUSDS, IRMA/devUSDG, IRMA/devFDUSD
//
// Pool parameters (per SOW):
//   - Bin step : 5 bps (0.05% price range per bin — tight, suited for stablecoins)
//   - Fee      : 40 bps (0.40%)
//   - Initial price: 1.0 (1 IRMA = 1 stablecoin at launch)
//
// Run from irma/ directory:
//   node scripts/create_dlmm_pools.cjs

"use strict";

// CJS build exports DLMM as the module itself (no .default wrapper)
const DLMM = require("@meteora-ag/dlmm");
const { ActivationType, deriveCustomizablePermissionlessLbPair } = DLMM;
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

// Meteora DLMM program on devnet
const DLMM_PROGRAM_ID = new PublicKey(
  "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo"
);

const BIN_STEP = new BN(5);   // 5 bps per bin — tight for stablecoins
const FEE_BPS  = new BN(40);  // 40 bps swap fee (per SOW)
const ACTIVE_ID = new BN(0);  // bin 0 = price 1.0 (1 IRMA per stablecoin at launch)

// The 6 stablecoins we're pairing IRMA with
const STABLECOINS = ["usdc", "usdt", "pyusd", "usds", "usdg", "fdusd"];

async function main() {
  const rpcUrl =
    process.env.ANCHOR_PROVIDER_URL ||
    process.env.SOLANA_RPC_URL ||
    "https://api.devnet.solana.com";

  const connection = new Connection(rpcUrl, "confirmed");

  // Load payer keypair from phantom1.json
  const keypairPath = path.join(
    process.env.HOME || "~",
    ".config/solana/phantom1.json"
  );
  const keypair = Keypair.fromSecretKey(
    new Uint8Array(JSON.parse(fs.readFileSync(keypairPath, "utf-8")))
  );

  console.log("\n🚀 Creating IRMA DLMM Pools on Meteora Devnet");
  console.log("==============================================");
  console.log("Wallet :", keypair.publicKey.toBase58());
  const balance = await connection.getBalance(keypair.publicKey);
  console.log("Balance:", balance / 1e9, "SOL\n");

  if (balance < 0.5e9) {
    throw new Error("Insufficient balance — need at least 0.5 SOL.");
  }

  const irmaMint = new PublicKey(config.tokens.irma.mint);

  // Meteora requires the creator to hold non-zero balances of both tokenX (IRMA)
  // and tokenY (stablecoin) as a "token launch proof" before a pool can be created.
  // For IRMA and all 4 mock stablecoins we are the mint authority, so we self-mint.
  // For canonical devUSDC / devUSDT we are NOT the mint authority — those are handled
  // separately (the script will skip them with a clear message).

  console.log("🔧 Pre-flight: ensuring wallet holds ≥1 unit of every token we control...\n");

  // Tokens we own the mint authority for: IRMA (Token-2022) + 4 mock stablecoins (classic)
  const OWN_TOKENS = [
    { symbol: "irma",  programId: TOKEN_2022_PROGRAM_ID },
    { symbol: "usdt",  programId: TOKEN_PROGRAM_ID },
    { symbol: "pyusd", programId: TOKEN_PROGRAM_ID },
    { symbol: "usds",  programId: TOKEN_PROGRAM_ID },
    { symbol: "usdg",  programId: TOKEN_PROGRAM_ID },
    { symbol: "fdusd", programId: TOKEN_PROGRAM_ID },
  ];

  for (const { symbol, programId } of OWN_TOKENS) {
    const mint = new PublicKey(config.tokens[symbol].mint);
    const tokenAccount = await getOrCreateAssociatedTokenAccount(
      connection, keypair, mint, keypair.publicKey,
      false, "confirmed", {}, programId
    );
    if (tokenAccount.amount === 0n) {
      await mintTo(
        connection, keypair, mint, tokenAccount.address,
        keypair,         // mint authority = phantom1
        1_000_000n,      // 1 token (6 decimals)
        [], {}, programId
      );
      console.log(`   ✅ Minted 1 ${symbol.toUpperCase()} to ${tokenAccount.address.toBase58()}`);
    } else {
      console.log(`   ✅ ${symbol.toUpperCase()}: already have ${tokenAccount.amount} units`);
    }
  }
  console.log();

  const createdPools = {};
  const skipped = [];

  for (const symbol of STABLECOINS) {
    const token = config.tokens[symbol];
    if (!token) {
      console.warn(`⚠️  No config entry for "${symbol}" — skipping.`);
      continue;
    }

    const stableMint = new PublicKey(token.mint);

    // Derive the pool PDA so we know the address before sending the tx
    const [poolAddress] = deriveCustomizablePermissionlessLbPair(
      irmaMint,
      stableMint,
      DLMM_PROGRAM_ID
    );

    console.log(`\n📦 Pool: IRMA / ${token.name}`);
    console.log(`   Stablecoin mint : ${stableMint.toBase58()}`);
    console.log(`   Expected address: ${poolAddress.toBase58()}`);

    try {
      const createTx = await DLMM.createCustomizablePermissionlessLbPair2(
        connection,
        BIN_STEP,
        irmaMint,             // tokenX = IRMA (base)
        stableMint,           // tokenY = stablecoin (quote)
        ACTIVE_ID,
        FEE_BPS,
        ActivationType.Slot,  // activate immediately on next slot
        false,                // hasAlphaVault
        keypair.publicKey,    // creator
        undefined,            // activationPoint — none means immediate
        false,                // creatorPoolOnOffControl
        { cluster: "devnet" }
      );

      const { blockhash } = await connection.getLatestBlockhash();
      createTx.feePayer = keypair.publicKey;
      createTx.recentBlockhash = blockhash;

      const sig = await sendAndConfirmTransaction(
        connection,
        createTx,
        [keypair],
        { commitment: "confirmed" }
      );

      console.log(`   ✅ Created. Tx: ${sig}`);
      createdPools[symbol] = poolAddress.toBase58();
    } catch (err) {
      const msg = err?.message ?? String(err);

      // Pool already exists — idempotent, just record and move on
      if (
        msg.includes("already in use") ||
        msg.includes("already exists") ||
        msg.includes("0x0")
      ) {
        console.log(`   ℹ️  Pool already exists — recording address.`);
        createdPools[symbol] = poolAddress.toBase58();
      } else if (msg.includes("MissingTokenAmountAsTokenLaunchProof") || msg.includes("0x17ac")) {
        // We don't hold any of this stablecoin (e.g. canonical devUSDC/devUSDT).
        // Skip for now — user must obtain tokens from a faucet first.
        console.log(`   ⚠️  Skipped — wallet holds no ${token.name}.`);
        console.log(`      Obtain some ${token.name} from a faucet then re-run.`);
        skipped.push(symbol);
      } else {
        console.error(`   ❌ Failed for ${symbol}: ${msg}`);
        throw err;
      }
    }
  }

  // Write pool addresses back into devnet-config.json
  config.pools = {};
  for (const [symbol, address] of Object.entries(createdPools)) {
    config.pools[symbol] = {
      address,
      tokenX: config.tokens.irma.mint,
      tokenY: config.tokens[symbol].mint,
    };
  }

  fs.writeFileSync(
    path.join(__dirname, "../devnet-config.json"),
    JSON.stringify(config, null, 2)
  );

  console.log("\n✅ devnet-config.json updated with pool addresses.");
  console.log("\n📋 Summary:");
  for (const [symbol, address] of Object.entries(createdPools)) {
    const name = config.tokens[symbol]?.name ?? symbol.toUpperCase();
    console.log(`   IRMA/${name.padEnd(10)} → ${address}`);
  }
  if (skipped.length > 0) {
    console.log(`\n⚠️  Skipped (no token balance): ${skipped.join(", ")}`);
    console.log("   Get tokens from a devnet faucet and re-run to create these pools.");
  }
}

main().catch((err) => {
  console.error("\n❌ Fatal error:", err);
  process.exit(1);
});
