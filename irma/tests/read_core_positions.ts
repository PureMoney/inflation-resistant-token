import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import { Connection, PublicKey, Keypair } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import * as os from "os";
import dotenv from "dotenv";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
dotenv.config({ path: path.join(__dirname, "../.env") });

const idl = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../target/idl/irma.json"), "utf-8")
);
const PROGRAM_ID = new PublicKey(idl.address);

async function main() {
  const rpcUrl =
    process.env.ANCHOR_PROVIDER_URL ||
    process.env.SOLANA_RPC_URL ||
    "https://api.devnet.solana.com";
  const connection = new Connection(rpcUrl, "confirmed");
  const keypair = Keypair.fromSecretKey(
    new Uint8Array(
      JSON.parse(
        fs.readFileSync(path.join(os.homedir(), ".config/solana/phantom1.json"), "utf-8")
      )
    )
  );
  const wallet = new Wallet(keypair);
  const provider = new AnchorProvider(connection, wallet, { commitment: "confirmed" });
  const program = new Program(idl, provider);

  const [corePda] = PublicKey.findProgramAddressSync([Buffer.from("core_v5")], PROGRAM_ID);
  const core: any = await (program.account as any).core.fetch(corePda);

  for (const pos of core.positionData.allPositions) {
    const lbPair = pos.lbPair.toBase58();
    console.log(`\nlbPair: ${lbPair}`);
    console.log("minBinId:", pos.minBinId, "maxBinId:", pos.maxBinId);
    console.log("positionPks:", pos.positionPks.map((p: PublicKey) => p.toBase58()));
    console.log("binArrayPks:", pos.binArrayPks?.map((p: PublicKey) => p.toBase58()));
  }
}
main().catch(console.error);
