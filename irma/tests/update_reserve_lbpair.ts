import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import { Connection, PublicKey, SystemProgram, Keypair } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import dotenv from "dotenv";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
dotenv.config({ path: path.join(__dirname, "../.env") });

const config = JSON.parse(fs.readFileSync(path.join(__dirname, "../devnet-config.json"), "utf-8"));
const idl = JSON.parse(fs.readFileSync(path.join(__dirname, "../target/idl/irma.json"), "utf-8"));

const PROGRAM_ID = new PublicKey(idl.address);
const connection = new Connection(process.env.ANCHOR_PROVIDER_URL || "https://api.devnet.solana.com", "confirmed");
const keypair = Keypair.fromSecretKey(new Uint8Array(JSON.parse(process.env.SOLANA_PRIVATE_KEY!)));
const provider = new AnchorProvider(connection, new Wallet(keypair), { commitment: "confirmed" });
const program = new Program(idl, provider);

const [statePda] = PublicKey.findProgramAddressSync([Buffer.from("state_v5")], PROGRAM_ID);
const [corePda]  = PublicKey.findProgramAddressSync([Buffer.from("core_v5")],  PROGRAM_ID);

// Build the remaining-accounts list the program needs to link reserves to their LbPairs.
//
// update_reserve_lbpair reads two things from remaining_accounts:
//   1. fetch_lb_pair_state — needs the LbPair (pool) account itself, to verify token_y_mint
//      matches the registered reserve's mint.
//   2. fetch_token_info — loops over EVERY position the program currently knows about
//      (on the first call this means all 6, since it seeds them all at once) and needs
//      both that pool's LbPair account and its two token-mint accounts.
//
// All of these addresses are already recorded in devnet-config.json — no guesswork needed.
function buildRemainingAccounts(): { pubkey: PublicKey; isSigner: boolean; isWritable: boolean }[] {
  const seen = new Set<string>();
  const metas: { pubkey: PublicKey; isSigner: boolean; isWritable: boolean }[] = [];

  const add = (address: string) => {
    if (seen.has(address)) return;
    seen.add(address);
    metas.push({ pubkey: new PublicKey(address), isSigner: false, isWritable: false });
  };

  // All 6 pool (LbPair) addresses
  for (const symbol of Object.keys(config.pools)) {
    add(config.pools[symbol].address);
  }
  // All 7 token mints (IRMA + 6 stablecoins)
  for (const symbol of Object.keys(config.tokens)) {
    add(config.tokens[symbol].mint);
  }

  return metas;
}

async function updateReserveLbpair(symbol: string, lbPairAddress: string) {
  console.log(`\n🔗 Linking reserve ${symbol} to LbPair ${lbPairAddress}`);

  const remainingAccounts = buildRemainingAccounts();
  console.log(`   Passing ${remainingAccounts.length} remaining accounts (pools + token mints)`);

  const tx = await (program.methods as any)
    .updateReserveLbpair(symbol, lbPairAddress)
    .accounts({
      state: statePda,
      irmaAdmin: keypair.publicKey,
      core: corePda,
      systemProgram: SystemProgram.programId,
    })
    .remainingAccounts(remainingAccounts)
    .rpc();

  console.log(`   ✅ Linked ${symbol} — tx: ${tx}`);
  return tx;
}

// Usage:
//   npx ts-node tests/update_reserve_lbpair.ts <symbol>        — link a single reserve
//   npx ts-node tests/update_reserve_lbpair.ts all             — link all 6 reserves in sequence
const arg = process.argv[2];
if (!arg) {
  console.error("Usage: ts-node tests/update_reserve_lbpair.ts <symbol|all>");
  process.exit(1);
}

(async () => {
  if (arg === "all") {
    for (const symbol of Object.keys(config.pools)) {
      const tokenCfg = config.tokens[symbol];
      const poolCfg = config.pools[symbol];
      await updateReserveLbpair(tokenCfg.name, poolCfg.address);
    }
  } else {
    const tokenCfg = Object.values(config.tokens).find((t: any) => t.name === arg) as any;
    const poolSymbol = Object.keys(config.tokens).find((k) => config.tokens[k].name === arg);
    const poolCfg = poolSymbol ? config.pools[poolSymbol] : undefined;
    if (!tokenCfg || !poolCfg) {
      console.error(`❌ Unknown symbol or no pool configured for: ${arg}`);
      process.exit(1);
    }
    await updateReserveLbpair(tokenCfg.name, poolCfg.address);
  }
})().catch(console.error);
