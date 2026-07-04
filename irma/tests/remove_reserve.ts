import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import { Connection, PublicKey, SystemProgram, Keypair } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import dotenv from "dotenv";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
dotenv.config({ path: path.join(__dirname, "../.env") });

const idl = JSON.parse(fs.readFileSync(path.join(__dirname, "../target/idl/irma.json"), "utf-8"));
const connection = new Connection(process.env.ANCHOR_PROVIDER_URL || "https://api.devnet.solana.com", "confirmed");
const keypair = Keypair.fromSecretKey(new Uint8Array(JSON.parse(process.env.SOLANA_PRIVATE_KEY!)));
const provider = new AnchorProvider(connection, new Wallet(keypair), { commitment: "confirmed" });
const program = new Program(idl, provider);

const [statePda] = PublicKey.findProgramAddressSync([Buffer.from("state_v5")], program.programId);
const [corePda]  = PublicKey.findProgramAddressSync([Buffer.from("core_v5")],  program.programId);

// Usage: ts-node tests/remove_reserve.ts <symbol>
const symbol = process.argv[2];
if (!symbol) {
  console.error("Usage: ts-node tests/remove_reserve.ts <symbol>");
  process.exit(1);
}

(async () => {
  console.log(`Removing reserve: ${symbol}`);
  const tx = await (program.methods as any)
    .removeReserve(symbol)
    .accounts({ state: statePda, irmaAdmin: keypair.publicKey, core: corePda, systemProgram: SystemProgram.programId })
    .rpc();
  console.log(`✅ Removed ${symbol} — tx: ${tx}`);
})().catch(console.error);
