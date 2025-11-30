import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import { Connection, PublicKey, SystemProgram, Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import dotenv from "dotenv";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Load environment variables from the main .env file
dotenv.config({ path: path.join(__dirname, "../.env") });

// Load IDL
const idl = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../target/idl/irma.json"), "utf-8")
);

const PROGRAM_ID = new PublicKey(idl.address);

async function closeAccounts() {
  console.log("\n🗑️ Closing IRMA Protocol Accounts");
  console.log("  This does not actually close the accounts, but checks their existence and balance.");
  console.log("  It should work if there is a close_account instruction implemented on-chain.");
  console.log("===================================================================================\n");

  // Use environment variables from .env file
  const rpcUrl = process.env.ANCHOR_PROVIDER_URL || process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
  const commitment = (process.env.ANCHOR_COMMITMENT || process.env.SOLANA_COMMITMENT || "confirmed") as any;
  
  console.log("🌐 Using RPC URL:", rpcUrl);
  console.log("🔒 Using commitment:", commitment);

  // Create connection and provider manually using environment variables
  const connection = new Connection(rpcUrl, commitment);
  
  // Create or load a keypair for testing
  let keypair: Keypair;
  
  // Try to load from environment variable SOLANA_PRIVATE_KEY (base58 encoded)
  if (process.env.SOLANA_PRIVATE_KEY) {
    try {
      const privateKeyArray = JSON.parse(process.env.SOLANA_PRIVATE_KEY);
      keypair = Keypair.fromSecretKey(new Uint8Array(privateKeyArray));
      console.log("🔑 Loaded keypair from SOLANA_PRIVATE_KEY environment variable");
    } catch (error) {
      console.log("❌ Failed to parse SOLANA_PRIVATE_KEY, generating new keypair");
      keypair = Keypair.generate();
    }
  } else {
    // Generate a new keypair for testing
    keypair = Keypair.generate();
    console.log("🔑 Generated new test keypair");
  }
  
  const wallet = new Wallet(keypair);
  const provider = new AnchorProvider(connection, wallet, { commitment });
  const program = new Program(idl, provider);
  
  const payer = provider.wallet.publicKey;
  console.log("👤 Using wallet public key:", payer.toBase58());

  // Derive PDAs
  const [statePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("state")],
    PROGRAM_ID
  );
  
  const [corePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("core")],
    PROGRAM_ID
  );

  console.log("📍 Accounts to close:");
  console.log(`   State PDA: ${statePda.toBase58()}`);
  console.log(`   Core PDA: ${corePda.toBase58()}\n`);

  try {
    // Check state account
    const stateAccountInfo = await connection.getAccountInfo(statePda);
    if (stateAccountInfo) {
      console.log(`💰 State account balance: ${stateAccountInfo.lamports / LAMPORTS_PER_SOL} SOL`);
      console.log(`📊 State account size: ${stateAccountInfo.data.length} bytes`);
      
      // Transfer the balance back to the payer (this effectively "closes" the account)
      if (stateAccountInfo.lamports > 0) {
        console.log("🔄 Transferring state account balance back to wallet...");
        
        // Create a transfer instruction that will drain the account
        const transferInstruction = SystemProgram.transfer({
          fromPubkey: statePda,
          toPubkey: payer,
          lamports: stateAccountInfo.lamports,
        });
        
        // Note: This won't work because PDAs can't sign transactions
        // Instead, we need the program to close the account
        console.log("❌ Cannot directly transfer from PDA. Need program instruction to close account.");
        console.log("🔧 Alternative: Let's check if we can reinitialize over the existing account...");
      }
    } else {
      console.log("✅ State account does not exist");
    }

    // Check core account  
    const coreAccountInfo = await connection.getAccountInfo(corePda);
    if (coreAccountInfo) {
      console.log(`💰 Core account balance: ${coreAccountInfo.lamports / LAMPORTS_PER_SOL} SOL`);
      console.log(`📊 Core account size: ${coreAccountInfo.data.length} bytes`);
    } else {
      console.log("✅ Core account does not exist");
    }

    console.log("\n💡 Recommendations:");
    console.log("1. Try to reinitialize the protocol - Anchor might handle reinitialization");
    console.log("2. Add a 'close' instruction to your Rust program to properly close accounts");
    console.log("3. Use a different seed for the state account to create a new one");
    console.log("4. Check if the account structure has changed and needs migration");

  } catch (error) {
    console.error("❌ Error checking accounts:", error);
  }
}

// Run the function
closeAccounts().catch(console.error);