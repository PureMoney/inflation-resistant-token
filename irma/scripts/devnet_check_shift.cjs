// Trigger checkShiftPriceRanges on devnet for a specific pool.
//
// Forces a mint-price shift so shift_mint_position() runs, which:
//   - Calls withdraw() on whatever position_pks[0] currently is
//   - If not found (e.g. stale V1 key), auto-creates a fresh V2 PDA
//   - Sets position_pks[0] to the new V2 PDA
//   - Deposits IRMA into it
//
// This is the workaround for the V1-position issue where seed_dlmm_liquidity.cjs
// created a legacy V1 keypair position that the program can no longer use.
//
// Usage: node scripts/devnet_check_shift.cjs <symbol>
//   e.g. node scripts/devnet_check_shift.cjs usdt
"use strict";

const {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  ComputeBudgetProgram,
  SYSVAR_RENT_PUBKEY,
  SYSVAR_CLOCK_PUBKEY,
  sendAndConfirmTransaction,
} = require("@solana/web3.js");
const { AnchorProvider, Program } = require("@coral-xyz/anchor");
const {
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
} = require("@solana/spl-token");
const DLMM = require("@meteora-ag/dlmm");
const fs = require("fs");
const path = require("path");
const os = require("os");

require("dotenv").config({ path: path.join(__dirname, "../.env") });

const config = JSON.parse(fs.readFileSync(path.join(__dirname, "../devnet-config.json"), "utf-8"));
const idl = JSON.parse(fs.readFileSync(path.join(__dirname, "../target/idl/irma.json"), "utf-8"));

const REAL_PROGRAM_ID = new PublicKey("E15v5VirGqdbH4fYhxxxZHNiLAP3t3y1SPonhrQxoTcs");
const DLMM_PROGRAM_ID = new PublicKey("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");
const MEMO_PROGRAM = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

const i32le = (n) => { const b = Buffer.alloc(4); b.writeInt32LE(n, 0); return b; };
const i64le = (n) => { const b = Buffer.alloc(8); b.writeBigInt64LE(BigInt(n), 0); return b; };
const BIN_ARRAY_SIZE = 70;

async function buildRemainingAccounts(dlmmPool, adminKeypair, irmaMint, reserveMint) {
  const [{ userPositions }, binArraysX2Y, binArraysY2X] = await Promise.all([
    dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey),
    dlmmPool.getBinArrayForSwap(false),
    dlmmPool.getBinArrayForSwap(true),
  ]);

  const binArrayKeySet = new Set([
    ...binArraysX2Y.map((b) => b.publicKey.toBase58()),
    ...binArraysY2X.map((b) => b.publicKey.toBase58()),
  ]);

  const nearbyIndices = new Set([-1, 0, 1]);
  for (const p of userPositions) {
    if (p?.positionData?.lowerBinId === undefined) continue;
    const idx = Math.floor(p.positionData.lowerBinId / BIN_ARRAY_SIZE);
    for (const d of [-1, 0, 1]) nearbyIndices.add(idx + d);
  }

  const derivedMetas = [...nearbyIndices].flatMap((idx) => {
    const lowerBinId = idx * BIN_ARRAY_SIZE;
    const [positionPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("position"), dlmmPool.pubkey.toBuffer(), adminKeypair.publicKey.toBuffer(), i32le(lowerBinId), i32le(BIN_ARRAY_SIZE)],
      DLMM_PROGRAM_ID
    );
    const [binArrayPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("bin_array"), dlmmPool.pubkey.toBuffer(), i64le(idx)],
      DLMM_PROGRAM_ID
    );
    return [
      { pubkey: positionPda, isSigner: false, isWritable: true },
      { pubkey: binArrayPda, isSigner: false, isWritable: true },
    ];
  });

  const adminIrmaAta = getAssociatedTokenAddressSync(irmaMint, adminKeypair.publicKey, false, TOKEN_2022_PROGRAM_ID);
  const adminReserveAta = getAssociatedTokenAddressSync(reserveMint, adminKeypair.publicKey, false, TOKEN_PROGRAM_ID);

  const entries = [
    { pubkey: dlmmPool.pubkey, isSigner: false, isWritable: true },
    ...derivedMetas,
    ...userPositions.map((p) => ({ pubkey: p.publicKey, isSigner: false, isWritable: true })),
    ...[...binArrayKeySet].map((k) => ({ pubkey: new PublicKey(k), isSigner: false, isWritable: true })),
    { pubkey: irmaMint, isSigner: false, isWritable: false },
    { pubkey: reserveMint, isSigner: false, isWritable: false },
    { pubkey: adminIrmaAta, isSigner: false, isWritable: true },
    { pubkey: adminReserveAta, isSigner: false, isWritable: true },
    { pubkey: dlmmPool.tokenX.reserve, isSigner: false, isWritable: true },
    { pubkey: dlmmPool.tokenY.reserve, isSigner: false, isWritable: true },
    { pubkey: dlmmPool.tokenX.owner, isSigner: false, isWritable: false },
    { pubkey: dlmmPool.lbPair.oracle, isSigner: false, isWritable: true },
    { pubkey: adminKeypair.publicKey, isSigner: true, isWritable: true },
    { pubkey: PublicKey.findProgramAddressSync([Buffer.from("__event_authority")], DLMM_PROGRAM_ID)[0], isSigner: false, isWritable: false },
    ...(dlmmPool.tokenX.transferHookAccountMetas || []),
    { pubkey: DLMM_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: TOKEN_2022_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
    { pubkey: SYSVAR_CLOCK_PUBKEY, isSigner: false, isWritable: false },
    { pubkey: MEMO_PROGRAM, isSigner: false, isWritable: false },
  ];

  const seen = new Set();
  return entries.filter(({ pubkey }) => {
    const key = pubkey.toBase58();
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

async function main() {
  const symbol = process.argv[2];
  if (!symbol) {
    console.error("Usage: node scripts/devnet_check_shift.cjs <symbol>  (e.g. usdt)");
    process.exit(1);
  }

  const poolCfg = config.pools?.[symbol];
  const tokenCfg = config.tokens[symbol];
  if (!poolCfg || !tokenCfg) {
    console.error(`Unknown symbol: ${symbol}`);
    process.exit(1);
  }

  const rpcUrl = process.env.ANCHOR_PROVIDER_URL || "https://api.devnet.solana.com";
  const connection = new Connection(rpcUrl, "confirmed");

  const keypairPath = process.env.SOLANA_KEYPAIR_PATH || path.join(os.homedir(), ".config/solana/phantom1.json");
  const admin = Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync(keypairPath, "utf-8"))));
  console.log("Admin:", admin.publicKey.toBase58());

  class CustomWallet {
    constructor(kp) { this.payer = kp; }
    async signTransaction(tx) { tx.partialSign(this.payer); return tx; }
    async signAllTransactions(txs) { return txs.map((t) => { t.partialSign(this.payer); return t; }); }
    get publicKey() { return this.payer.publicKey; }
  }

  const provider = new AnchorProvider(connection, new CustomWallet(admin), { commitment: "confirmed" });
  const program = new Program(idl, provider);

  const [statePda] = PublicKey.findProgramAddressSync([Buffer.from("state_v5")], REAL_PROGRAM_ID);
  const [corePda]  = PublicKey.findProgramAddressSync([Buffer.from("core_v5")],  REAL_PROGRAM_ID);

  const irmaMint   = new PublicKey(config.tokens.irma.mint);
  const reserveMint = new PublicKey(tokenCfg.mint);
  const poolAddress = new PublicKey(poolCfg.address);
  const reserveSymbol = tokenCfg.name; // e.g. "devUSDT"

  // --- 1. Force a price shift by setting mint price to a value in a different bin ---
  // Current max_bin_id = 2 (price ≈ 1.001). Bump to 1.011 (~bin 22) so shift_mint_position runs.
  // After the fix succeeds, set it back to a reasonable oracle-derived value.
  const stateAcct = await program.account.stateMap.fetch(statePda);
  const reserve = stateAcct.reserves.find((r) => r.symbol === reserveSymbol);
  if (!reserve) {
    console.error(`Reserve ${reserveSymbol} not found in StateMap.`);
    process.exit(1);
  }
  const currentPrice = reserve.mintPrice;
  console.log(`\nCurrent ${reserveSymbol} mintPrice: ${currentPrice}`);

  // Alternate between 1.011 and 1.002 to always force a shift
  const newPrice = Math.abs(currentPrice - 1.011) < 1e-9 ? 1.002 : 1.011;
  console.log(`Setting mintPrice to ${newPrice} to force needs_mint_shift=true...`);
  await program.methods
    .setMintPrice(reserveSymbol, newPrice)
    .accounts({ state: statePda, irmaAdmin: admin.publicKey, core: corePda, systemProgram: SystemProgram.programId })
    .rpc();
  console.log("  ✅ Price set.");

  // --- 2. Build remaining accounts and call checkShiftPriceRanges ---
  console.log(`\nLoading DLMM pool ${poolAddress.toBase58()}...`);
  const dlmmPool = await DLMM.create(connection, poolAddress, { cluster: "devnet" });
  const remainingAccounts = await buildRemainingAccounts(dlmmPool, admin, irmaMint, reserveMint);
  console.log(`Built ${remainingAccounts.length} remaining accounts.`);

  console.log("\nCalling checkShiftPriceRanges...");
  const tx = await program.methods
    .checkShiftPriceRanges(reserveSymbol)
    .accounts({ state: statePda, irmaAdmin: admin.publicKey, core: corePda, systemProgram: SystemProgram.programId })
    .remainingAccounts(remainingAccounts)
    .transaction();

  tx.instructions.unshift(
    ComputeBudgetProgram.setComputeUnitLimit({ units: 1_000_000 }),
    ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1 })
  );

  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
  tx.recentBlockhash = blockhash;
  tx.feePayer = admin.publicKey;
  tx.sign(admin);

  let signature;
  try {
    signature = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
    console.log("  Sent:", signature);
    await connection.confirmTransaction({ signature, blockhash, lastValidBlockHeight }, "confirmed");
    console.log("  ✅ Transaction confirmed:", signature);
  } catch (err) {
    console.error("  ❌ Transaction failed:", err.message);
    if (err.logs) for (const l of err.logs) console.log(" ", l);
  }

  if (signature) {
    const txInfo = await connection.getTransaction(signature, { commitment: "confirmed", maxSupportedTransactionVersion: 0 });
    const logs = txInfo?.meta?.logMessages || [];
    console.log("\n--- Transaction logs ---");
    for (const l of logs) console.log(" ", l);

    const ok = logs.some((l) => l.includes("check_shift_price_ranges called") || l.includes("Depositing liquidity"));
    console.log(ok ? "\n✅ checkShiftPriceRanges succeeded!" : "\n⚠️  No success marker in logs — check above for errors.");
  }
}

main().catch((err) => {
  console.error("\n❌ Fatal:", err);
  process.exit(1);
});
