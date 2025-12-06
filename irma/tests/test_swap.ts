import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import pkg from "@coral-xyz/anchor";
const { BN } = pkg;
import { Connection, PublicKey, SystemProgram, Keypair } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import dotenv from "dotenv";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Load environment variables from the main .env file
dotenv.config({ path: path.join(__dirname, "../.env") });

// Load config
const config = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../devnet-config.json"), "utf-8")
);

// Load IDL
const idl = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../target/idl/irma.json"), "utf-8")
);

// Use program ID from IDL (which should match the declare_id! in Rust code)
const PROGRAM_ID = new PublicKey(idl.address);

console.log("🆔 Using Program ID from IDL:", PROGRAM_ID.toBase58());


// Get prices for USDC
async function get_prices_for_usdc(
    program: Program,
    statePda: PublicKey,
    corePda: PublicKey,
    payer: PublicKey
) {
  console.log("\nGet both mint and redemption prices for USDC");
  console.log("======\n");
    const pricesResult = await program.methods
    .getPrices("devUSDC")
    .accounts({
        state: statePda,
        irmaAdmin: payer,
        core: corePda,
        systemProgram: SystemProgram.programId,
    })
    .simulate();
    
    // Look for the "Program return" line in the raw logs
    const returnLine = pricesResult.raw.find((line: string) => 
    line.includes("Program return:"));
    
    if (returnLine) {
        // Extract the base64 data from the return line
        // Format: "Program return: <PROGRAM_ID> <BASE64_DATA>"
        const base64Data = returnLine.split(' ').pop();
        if (base64Data) {
            // Decode the base64 data
            const decodedData = Buffer.from(base64Data, 'base64');
            console.log("📊 Decoded data length:", decodedData.length, "bytes");
            console.log("📊 Raw bytes:", Array.from(decodedData).map(b => b.toString(16).padStart(2, '0')).join(' '));
            
            // Read two f64 values (8 bytes each, little-endian)
            if (decodedData.length >= 16) {
            const mintPrice = decodedData.readDoubleLE(0);
            const redemptionPrice = decodedData.readDoubleLE(8);
            console.log("📊 Get prices for USDC - Mint Price:", mintPrice, "Redemption Price:", redemptionPrice);
            } else {
            console.log("❌ Insufficient data length. Expected 16 bytes, got", decodedData.length);
            }
        }
    }
}

async function test_swap() {
  console.log("\n🚀 Test Swap Effect on redemption price");
  console.log("==========================================\n");

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
    console.log("💡 To use a persistent wallet, set SOLANA_PRIVATE_KEY environment variable");
    console.log(`   Example: export SOLANA_PRIVATE_KEY='[${Array.from(keypair.secretKey).join(',')}]'`);
  }
  
  const wallet = new Wallet(keypair);
  const provider = new AnchorProvider(connection, wallet, { commitment });
  const program = new Program(idl, provider);
  
  const payer = provider.wallet.publicKey;
  console.log("👤 Using wallet public key:", payer.toBase58());
  
  // Check current balance
  const balance = await connection.getBalance(payer);
  console.log("💰 Current balance:", balance / 1e9, "SOL");
  
  if (balance < 1e9) { // Less than 1 SOL
    console.log("⚠️ Low balance detected. You need to fund this wallet.");
    console.log(`💸 Run: solana airdrop 2 ${payer.toBase58()} --url devnet`);
    console.log("   Or fund it manually from a faucet or another wallet");
    
    // Don't proceed without funds
    throw new Error("Insufficient funds. Please fund the wallet and try again.");
  }

  // Derive PDAs
  const [statePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("state_v5")],
    PROGRAM_ID
  );
  
  const [corePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("core_v5")],
    PROGRAM_ID
  );

  console.log("📍 Derived PDAs:");
  console.log(`   State PDA: ${statePda.toBase58()}`);
  console.log(`   Core PDA: ${corePda.toBase58()}\n`);

  try {

    const configKeys = [
      // Add some example pair addresses - these should be actual DLMM pair addresses
       "4KVmauYHQp4kToXuVE7p89q8np3gjKZjULj6JBBDzDXR", // Example position
    //   "8BnEgHoWFysVcuFFX7QztDmzuH8r5ZFvyP3sYwn1XTh6", // Example pair 2
    ];

    // Sale Trade Event
    console.log("🔄 Calling sale_trade_event() instruction...");
    
    const tx_sale = await program.methods
      .saleTradeEvent("devUSDC", new BN(100_000_000)) // Example: selling 1 USDC (6 decimals)
      .accounts({
        state: statePda,
        irmaAdmin: payer,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(configKeys.map((key) => ({
        pubkey: new PublicKey(key),
        isSigner: false,
        isWritable: false,
      })))
      .rpc();
      // .simulate();
    console.log("✅ SaleTradeEvent transaction results:", tx_sale);
    console.log();

    await get_prices_for_usdc(program, statePda, corePda, payer).then(() => {
      console.log("\n");
    });

    // Buy Trade Event
    console.log("🔄 Calling buy_trade_event() instruction...");

    const tx_buy = await program.methods
      .buyTradeEvent("devUSDC", new BN(10_000_000)) // Example: buying 1 USDC (6 decimals)
      .accounts({
        state: statePda,
        irmaAdmin: payer,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(configKeys.map((key) => ({
        pubkey: new PublicKey(key),
        isSigner: false,
        isWritable: false,
      })))
      .rpc();
      // .simulate();

    await get_prices_for_usdc(program, statePda, corePda, payer).then(() => {
      console.log("\n");
    });

    console.log("✅ BuyTradeEvent transaction results:", tx_buy);
    console.log();
    // console.log("⏳ Waiting for confirmation...");

    // // Wait for confirmation
    // await connection.confirmTransaction(tx);
    // console.log("✅ Transaction confirmed!");

    // Fetch the initialized state
    console.log("📖 Fetching current state...");
    const state = await (program.account as any).stateMap.fetch(statePda);
    const core = await (program.account as any).core.fetch(corePda);

    console.log("🎉 Protocol data fetched successfully!\n");
    console.log("📊 State Data:", JSON.stringify(state, null, 2));
    console.log("📊 Core Data:", JSON.stringify(core, null, 2));

    return { state, core };

  } catch (error: any) {
    console.error("❌ Error during transaction:");
    console.error(error);
    
    if (error.message.includes("insufficient funds")) {
      console.log("\n💡 Hint: Need more SOL. Try running:");
      console.log(`   solana airdrop 2 ${payer.toBase58()} --url devnet`);
    }
    
    // throw error;
  }
}

// Run the function (removed catch so it doesn't display the error twice)
test_swap(); // .catch(console.error);
