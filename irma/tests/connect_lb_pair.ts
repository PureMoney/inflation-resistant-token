// Connect Liquidity-Bearing Pair Test
// Retrieves list of reserves from data account and connects specified liquidity-bearing pair to
// the relevant reserve. 
// Liquidity-bearing pairs are expected to be from the DLMM protocol.
// Program params: pair address, reserve mint address
// Example usage: 
// npx ts-node tests/connect_lb_pair.ts HfQQYJTJkRw49yNufxnH4dBaDGNG3JWPLHLVhswkdpsP BRjpCHtyQLNCo8gqRUr8jtdAj5AjPYQaoqbvcZiHok1k
// (Connect devUSDC-IRMA DLMM pair to devUSDC reserve)
import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
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

// Main function to connect liquidity-bearing pair
// Params: reserveSymbol - Reserve stablecoin symbol (.e.g "devUSDC")
//         pairAddress - DLMM pair address
async function connect_lb_pair(reserveSymbol: string, pairAddress: string) {

  console.log("\n🚀 Connecting liquidity-bearing pair to IRMA Protocol");
  console.log("=====================================================\n");

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
    console.log("🔑 Generated new test keypair - must save this keypair!");
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
    [Buffer.from("state_v4")],
    PROGRAM_ID
  );
  
  const [corePda] = PublicKey.findProgramAddressSync(
    [Buffer.from("core_v4")],
    PROGRAM_ID
  );

  console.log("📍 Derived PDAs:");
  console.log(`   State PDA: ${statePda.toBase58()}`);
  console.log(`   Core PDA: ${corePda.toBase58()}\n`);

  try {
    // Call API for connecting liquidity-bearing pair
    console.log("🔍 Call updateReserveLbpair...");
    let stateAccount: any = null;
    let stableCoinStruct: any = null;
    try {
      let updateTxId = await program.methods
        .updateReserveLbpair(reserveSymbol, pairAddress)
        .accounts({
          state: statePda,
          irmaAdmin: payer,
          core: corePda,
          systemProgram: SystemProgram.programId,
        })
        .remainingAccounts(
          [pairAddress].map((addr) => ({
            pubkey: new PublicKey(addr),
            isWritable: false,
            isSigner: false,
          }))
        )
        .rpc();
        // .simulate();
      console.log("✅ updateReserveLbpair updated stablecoin mint signature:", updateTxId);
      stateAccount = await (program.account as any).stateMap.fetch(statePda);
      console.log("✅ State account fetched successfully");
      stableCoinStruct = stateAccount.reserves.filter((r: any) => r.symbol === reserveSymbol)[0];
      if (stableCoinStruct.poolId.toBase58() === pairAddress) {
        console.log(`🎉 Successfully connected pair ${pairAddress} to reserve ${reserveSymbol}`);
      }
      else {
        console.log(`❌ Failed to connect pair ${pairAddress} to reserve ${reserveSymbol}`);
      }
      console.log("📊 State account data:", JSON.stringify(stateAccount, null, 2));
      // core account is fetched below
    } catch (stateError) {
      console.log("❌ Error fetching state account:", stateError);
      console.log("🔍 Let's check if the account exists at all...");
      console.log("📊 State account data:", JSON.stringify(stateAccount, null, 2));
      // core account is fetched below
      
      // Check account info directly
      const accountInfo = await connection.getAccountInfo(statePda);
      if (accountInfo) {
        console.log("📊 State account exists - Owner:", accountInfo.owner.toBase58());
        console.log("📊 State account data length:", accountInfo.data.length);
        console.log("📊 First 32 bytes:", accountInfo.data.slice(0, 32));
        console.log("⚠️ The state account exists but cannot be deserialized.");
        console.log("🔧 This usually means the account structure has changed since it was created.");
        console.log("🔧 You may need to:");
        console.log("   1. Close and recreate the state account, OR");
        console.log("   2. Create a new state account with a different seed, OR");
        console.log("   3. Migrate the existing account data to the new structure");
        console.log(`🔧 To close the account, use: solana program close ${statePda.toBase58()} --bypass-warning`);
      } else {
        console.log("❌ State account does not exist!");
        return;
      }
    }

    // Check if already initialized
    let existingCore;
      try {
        existingCore = await (program.account as any).core.fetch(corePda);
        console.log("ℹ️ Protocol already initialized!");
        console.log("📊 Existing core:", existingCore);
        try {
          const pricesResult = await program.methods
            .listReserves()
            .accounts({
              state: statePda,
              irmaAdmin: payer,
              core: corePda,
              systemProgram: SystemProgram.programId,
            })
            .simulate();
          
          if (pricesResult.raw) {
            // fetch state data account again to see updated pool ID
            const updatedState = await (program.account as any).stateMap.fetch(statePda);
            console.log("🎉 Liquidity-bearing pair connected successfully!\n");
            console.log("📊 Updated State Data:", JSON.stringify(updatedState, null, 2));
            // console.log("📊 Core Data:", JSON.stringify(existingCore, null, 2));
            return { core: existingCore };
          } else {
            console.log("❌ No program return data found in logs");
            console.log("📊 Raw logs:", pricesResult.raw);
          }
          
        } catch (error) {
          console.log("❌ Error simulating get_prices:", error);
        }
        return { core: existingCore };
      } catch (error: unknown) {
        // Safely handle unknown errors without assuming they have a `message` property
        const errMsg =
          error instanceof Error
            ? error.message
            : typeof error === "string"
            ? error
            : JSON.stringify(error);
        if (existingCore == null)
          console.log("📝 Error:", errMsg, ", proceeding with initialization...");
        else {
          console.log("📝 Error:", errMsg);
          return { core: existingCore };
        }
      }
  
    // Fetch the initialized state
    console.log("📖 Fetching initialized state...");
    const state = await (program.account as any).stateMap.fetch(statePda);
    const core = await (program.account as any).core.fetch(corePda);

    console.log("🎉 Protocol successfully initialized!\n");
    console.log("📊 State Data:", JSON.stringify(state, null, 2));
    console.log("📊 Core Data:", JSON.stringify(core, null, 2));

    return { state, core };

  } catch (error: any) {
    console.error("❌ Error during initialization:");
    console.error(error);
    
    if (error.message.includes("insufficient funds")) {
      console.log("\n💡 Hint: Need more SOL. Try running:");
      console.log(`   solana airdrop 2 ${payer.toBase58()} --url devnet`);
    }
    
    throw error;
  }
}

const args = process.argv.slice(2);
if (args.length < 2) {
  console.error("❌ Missing arguments. Usage:");
  console.error("   npx ts-node tests/connect_lb_pair.ts <PAIR_ADDRESS> <RESERVE_MINT_ADDRESS>");
  process.exit(1);
}
const reserveSymbol = args[0];
const pairAddress = args[1];
// Run the function
connect_lb_pair(reserveSymbol, pairAddress).catch(console.error);
