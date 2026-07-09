import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import pkg from "@coral-xyz/anchor";
const { BN } = pkg;
import {
  Connection,
  PublicKey,
  SystemProgram,
  Keypair,
  Transaction,
  sendAndConfirmTransaction,
  ComputeBudgetProgram,
  // LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import dotenv from "dotenv";
import { Sign } from "crypto";


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
const dlmm = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../idls/dlmm.json"), "utf-8")
);

// Use program ID from IDL (which should match the declare_id! in Rust code)
const PROGRAM_ID = new PublicKey(idl.address);
const dlmm_id = new PublicKey(dlmm.address);
const DLMM_PROGRAM_ID = new PublicKey(dlmm_id);

console.log("🆔 Using Program ID from IDL:", PROGRAM_ID.toBase58());


async function createDataAccount(
  program: Program,
  payer: Keypair,
  lb_pair: PublicKey
) {
  console.log("createDataAccount payer address:", payer.publicKey.toBase58());

  // Derive the PDA using your program ID (since you removed owner=dlmm::ID)
  const BIN_ARRAY_BITMAP_SEED = Buffer.from("bitmap");

  const [bitmapExtensionPda] = PublicKey.findProgramAddressSync(
    [BIN_ARRAY_BITMAP_SEED, lb_pair.toBuffer()],
    program.programId  // Use your program ID now
  );

  console.log("Derived bitmap extension PDA:", bitmapExtensionPda.toBase58());
  console.log("LB pair:", lb_pair.toBase58());

  let createAccountInstruction = await program.methods
    .initBitmapExtension(lb_pair)
    .accounts({
      bitmapExtension: bitmapExtensionPda, // Use derived PDA
      irmaAdmin: payer.publicKey,
      systemProgram: SystemProgram.programId,
    })
    .transaction();

  // 5. Create and send the transaction with increased compute budget
  const transaction = new Transaction()
    .add(
      // Add compute budget instructions first
      ComputeBudgetProgram.setComputeUnitLimit({
        units: 1_000_000, // Request 1M compute units (5x the default)
      }),
      ComputeBudgetProgram.setComputeUnitPrice({
        microLamports: 1000, // Set priority fee for faster processing
      })
    )
    .add(createAccountInstruction);

  // The new account must sign the transaction to confirm it owns the address
  const signature = await sendAndConfirmTransaction(program.provider.connection, transaction, [
    payer,
  ]);

  console.log("Data account created with signature:", signature);

  // Verify the account was created successfully
  try {
    const accountInfo = await program.provider.connection.getAccountInfo(bitmapExtensionPda);
    if (accountInfo) {
      console.log("✅ Bitmap extension account created successfully!");
      console.log("   Account owner:", accountInfo.owner.toBase58());
      console.log("   Account data length:", accountInfo.data.length);
      console.log("   Account lamports:", accountInfo.lamports);
    } else {
      console.log("❌ Bitmap extension account not found after creation");
    }
  } catch (error) {
    console.log("❌ Error fetching bitmap extension account:", error);
  }

  return bitmapExtensionPda;
}

// Get both mint and redemption prices for a given symbol
async function get_prices(
  symbol: string,
  program: Program,
  statePda: PublicKey,
  corePda: PublicKey,
  payer: PublicKey
) {
  console.log("\nGet both mint and redemption prices for USDC");
  console.log("======\n");
  const pricesResult = await program.methods
    .getPrices(symbol)
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

async function test_swap(symbol: string, amount: number, exactOut: number) {
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
    // Try to load from SOLANA_KEYPAIR_PATH or ~/.config/solana/id.json
    const os = await import("os");
    const keypairPath =
      process.env.SOLANA_KEYPAIR_PATH ||
      path.join(os.homedir(), ".config/solana/id.json");
    if (fs.existsSync(keypairPath)) {
      try {
        keypair = Keypair.fromSecretKey(
          new Uint8Array(JSON.parse(fs.readFileSync(keypairPath, "utf-8")))
        );
        console.log("🔑 Loaded keypair from local file:", keypairPath);
      } catch (error) {
        keypair = Keypair.generate();
        console.log("❌ Failed to load keypair from local file, generated new test keypair");
      }
    } else {
      keypair = Keypair.generate();
      console.log("🔑 Generated new test keypair");
    }
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

    // 1. Normalize the incoming symbol argument to lower case (e.g., "devUSDT" -> "usdt")
    const tokenSymbolKey = symbol.toLowerCase().replace("dev", "");
    const poolCfg = config.pools[tokenSymbolKey];
    const tokenCfg = config.tokens[tokenSymbolKey];
    const irmaCfg = config.tokens["irma"];

    if (!poolCfg || !tokenCfg) {
      throw new Error(`❌ No configuration found for token asset: ${symbol}`);
    }

    const lbPairAddress = poolCfg.address;
    const lbPairKey = new PublicKey(lbPairAddress);
    console.log(`🎯 Targeting real LB Pair: ${lbPairAddress}`);

    // Check if bitmap extension account exists using the correct dynamic LB Pair key
    const BIN_ARRAY_BITMAP_SEED = Buffer.from("bitmap");
    const [bitmapExtensionPda] = PublicKey.findProgramAddressSync(
      [BIN_ARRAY_BITMAP_SEED, lbPairKey.toBuffer()],
      program.programId
    );

    try {
      const bitmapAccountInfo = await connection.getAccountInfo(bitmapExtensionPda);
      if (bitmapAccountInfo) {
        console.log("✅ Bitmap extension already exists:", bitmapExtensionPda.toBase58());
      } else {
        console.log("🔄 Creating bitmap extension account...");
        await createDataAccount(program, provider.wallet.payer as Keypair, lbPairKey);
      }
    } catch (error) {
      console.log("🔄 Creating bitmap extension account after error flag check...");
      await createDataAccount(program, provider.wallet.payer as Keypair, lbPairKey);
    }

    // 2. Fetch the DLMM pool parameters dynamically to build remainingAccounts
    // 2. Fetch the DLMM pool parameters dynamically to build remainingAccounts using Anchor Program
    const dlmmProgram = new Program(dlmm, provider);
    const lbPairState: any = await dlmmProgram.account.lbPair.fetch(lbPairKey);
    const reserveX = new PublicKey(lbPairState.reserveX);
    const reserveY = new PublicKey(lbPairState.reserveY);
    const oracle = new PublicKey(lbPairState.oracle);

    // Derive user associated token accounts
    const { getAssociatedTokenAddressSync } = await import("@solana/spl-token");
    const userTokenX = getAssociatedTokenAddressSync(
      new PublicKey(irmaCfg.mint),
      payer,
      false,
      new PublicKey(irmaCfg.program)
    );
    const userTokenY = getAssociatedTokenAddressSync(
      new PublicKey(tokenCfg.mint),
      payer,
      false,
      new PublicKey(tokenCfg.program)
    );

    // Derive event authority
    const [eventAuthority] = PublicKey.findProgramAddressSync(
      [Buffer.from("__event_authority")],
      DLMM_PROGRAM_ID
    );

    // Derive bin array PDAs for index -1, 0, 1
    const deriveBinArrayPda = (lbPair: PublicKey, binArrayIdx: number) => {
      const binArrayIdxBuffer = Buffer.alloc(8);
      binArrayIdxBuffer.writeBigInt64LE(BigInt(binArrayIdx));
      const [pda] = PublicKey.findProgramAddressSync(
        [Buffer.from("bin_array"), lbPair.toBuffer(), binArrayIdxBuffer],
        DLMM_PROGRAM_ID
      );
      return pda;
    };
    const binArray0 = deriveBinArrayPda(lbPairKey, 0);
    const binArray1 = deriveBinArrayPda(lbPairKey, 1);
    const binArrayM1 = deriveBinArrayPda(lbPairKey, -1);

    const remainingAccounts = [
      { pubkey: lbPairKey, isSigner: false, isWritable: true },
      { pubkey: reserveX, isSigner: false, isWritable: true },
      { pubkey: reserveY, isSigner: false, isWritable: true },
      { pubkey: userTokenX, isSigner: false, isWritable: true },
      { pubkey: userTokenY, isSigner: false, isWritable: true },
      { pubkey: new PublicKey(irmaCfg.mint), isSigner: false, isWritable: false },
      { pubkey: new PublicKey(tokenCfg.mint), isSigner: false, isWritable: false },
      { pubkey: oracle, isSigner: false, isWritable: true },
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: new PublicKey(irmaCfg.program), isSigner: false, isWritable: false },
      { pubkey: new PublicKey(tokenCfg.program), isSigner: false, isWritable: false },
      { pubkey: new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"), isSigner: false, isWritable: false },
      { pubkey: eventAuthority, isSigner: false, isWritable: false },
      { pubkey: DLMM_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: bitmapExtensionPda, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: binArray0, isSigner: false, isWritable: true },
      { pubkey: binArray1, isSigner: false, isWritable: true },
      { pubkey: binArrayM1, isSigner: false, isWritable: true },
    ];

    // 3. Fire the execution method targeting cSwap
    console.log(`🔄 Calling cSwap() instruction with max_in=${amount}, exact_out=${exactOut}...`);
    const tx_sell = await program.methods
      .cSwap(symbol, new BN(amount), new BN(exactOut), false) // false = sell
      .accounts({
        state: statePda,
        irmaAdmin: payer,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .remainingAccounts(remainingAccounts)
      .transaction();

    // Add compute budget instructions to the swap transaction
    tx_sell.instructions.unshift(
      ComputeBudgetProgram.setComputeUnitLimit({
        units: 250_000, // Higher limit for complex swap operations
      }),
      //   ComputeBudgetProgram.setComputeUnitPrice({
      //     microLamports: 1000, // Higher priority for time-sensitive swaps
      //   })
    );

    // Send transaction
    const signature = await connection.sendTransaction(tx_sell, [wallet.payer]);
    console.log("🚀 Transaction sent:", signature);

    // Wait for confirmation with custom timeout
    try {
      const confirmation = await connection.confirmTransaction(
        {
          signature,
          blockhash: (await connection.getLatestBlockhash()).blockhash,
          lastValidBlockHeight: (await connection.getLatestBlockhash()).lastValidBlockHeight,
        },
        "confirmed"
      );

      if (confirmation.value.err) {
        console.error("❌ Transaction failed:", confirmation.value.err);
      } else {
        console.log("✅ Transaction confirmed:", signature);
      }
    } catch (timeoutError) {
      console.log("⏰ Transaction timeout, but may still be processing...");
      console.log("🔍 Check transaction status:", `https://solscan.io/tx/${signature}?cluster=devnet`);
    }
    console.log("✅ SaleTradeEvent transaction results:", tx_sell);
    console.log();

    await get_prices(symbol, program, statePda, corePda, payer).then(() => {
      console.log("\n");
    });


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

const args = process.argv.slice(2);
if (args.length > 3 || args.length < 2) {
  console.error("❌ Wrong count of arguments. Usage:");
  console.error("   npx tsx tests/test_swap_thru_meteora.ts <reserve symbol> <amount> [exact_out]");
  console.error("   <reserve symbol> = devUSDC | devUSDT, <amount> = max input (in smallest unit)");
  console.error("   [exact_out] = desired exact output amount (defaults to half of max input)");
  process.exit(1);
}
const symbol = args[0];
const amount = args[1];
const exactOut = args[2] ? parseInt(args[2]) : Math.floor(Number(amount) / 2);

// Run the function (removed catch so it doesn't display the error twice)
test_swap(symbol, Number(amount), exactOut);
