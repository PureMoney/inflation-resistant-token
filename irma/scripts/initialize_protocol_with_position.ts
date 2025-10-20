import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import BN from "bn.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function initializeProtocolWithPosition() {
  console.log("\n🚀 Initializing IRMA Protocol With Real Position");
  console.log("====================================================\n");

  // Load config
  const configPath = path.join(__dirname, "../devnet-config.json");
  const config = JSON.parse(fs.readFileSync(configPath, "utf-8"));

  // Setup connection and provider
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const walletPath = path.join(process.env.HOME || "", ".config/solana/phantom1.json");
  const walletKeypair = Keypair.fromSecretKey(
    new Uint8Array(JSON.parse(fs.readFileSync(walletPath, "utf-8")))
  );
  const wallet = new Wallet(walletKeypair);
  const provider = new AnchorProvider(connection, wallet, {
    commitment: "confirmed",
  });

  // Load program IDL
  const idlPath = path.join(__dirname, "../target/idl/irma.json");
  const idl = JSON.parse(fs.readFileSync(idlPath, "utf-8"));
  const program = new Program(idl, provider);

  console.log(`Program ID: ${program.programId.toBase58()}`);
  console.log(`Authority: ${wallet.publicKey.toBase58()}\n`);

  // Get addresses from config
  const irmaMint = new PublicKey(config.tokens.irma.mint);
  const usdcMint = new PublicKey(config.tokens.usdc.mint);
  const whirlpool = new PublicKey(config.orca.whirlpool);

  // Use real position from config
  let position: PublicKey;
  if (config.orca.position && config.orca.position !== "WILL_BE_CREATED") {
    position = new PublicKey(config.orca.position);
    console.log(`IRMA Mint: ${irmaMint.toBase58()}`);
    console.log(`USDC Mint: ${usdcMint.toBase58()}`);
    console.log(`Whirlpool: ${whirlpool.toBase58()}`);
    console.log(`Position: ${position.toBase58()} (REAL)\n`);
  } else {
    throw new Error(
      "No real position in config. Run 'npx ts-node scripts/create_orca_position.ts' first"
    );
  }

  // Find protocol state PDA
  const [protocolState, bump] = PublicKey.findProgramAddressSync(
    [Buffer.from("protocol_state")],
    program.programId
  );

  console.log(`Protocol State PDA: ${protocolState.toBase58()}`);
  console.log(`Bump: ${bump}\n`);

  // Initial prices (as BN for proper serialization)
  // Both mint and redemption start at 1.0 USDC
  // As IRMA is minted and stablecoins are added to backing, redemption price increases
  // As time passes, mint price increases due to inflation
  const initialPrice = new BN(1_000_000_000); // 1.00 USDC for both

  console.log("📊 Initial Prices:");
  console.log(`   Mint Price: ${initialPrice.toString()} (${initialPrice.toNumber() / 1_000_000_000} USDC)`);
  console.log(`   Redemption Price: ${initialPrice.toString()} (${initialPrice.toNumber() / 1_000_000_000} USDC)`);
  console.log(`   Note: Redemption price is calculated as backing_reserves / irma_in_circulation\n`);

  try {
    console.log("📝 Sending initialize_protocol transaction...\n");

    const tx = await program.methods
      .initializeProtocol(
        initialPrice,
        whirlpool,
        position,
        irmaMint,
        usdcMint
      )
      .accounts({
        protocolState: protocolState,
        authority: wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    console.log("✅ Protocol initialized successfully!");
    console.log(`   Transaction: ${tx}`);
    console.log(`   Explorer: https://explorer.solana.com/tx/${tx}?cluster=devnet\n`);

    // Fetch and display protocol state
    const stateAccount = await (program.account as any).protocolState.fetch(protocolState);
    
    console.log("📋 Protocol State:");
    console.log(`   Authority: ${stateAccount.authority.toBase58()}`);
    console.log(`   Mint Price: ${stateAccount.mintPrice.toString()} (${Number(stateAccount.mintPrice) / 1_000_000_000} USDC)`);
    console.log(`   Redemption Price: ${stateAccount.redemptionPrice.toString()} (${Number(stateAccount.redemptionPrice) / 1_000_000_000} USDC)`);
    console.log(`   Whirlpool: ${stateAccount.whirlpool.toBase58()}`);
    console.log(`   Position: ${stateAccount.position.toBase58()} ✓`);
    console.log(`   Token A (IRMA): ${stateAccount.tokenAMint.toBase58()}`);
    console.log(`   Token B (USDC): ${stateAccount.tokenBMint.toBase58()}`);
    console.log(`   Last Price Update: ${new Date(Number(stateAccount.lastPriceUpdate) * 1000).toISOString()}`);
    console.log(`   Last Rebalance: ${new Date(Number(stateAccount.lastRebalance) * 1000).toISOString()}`);
    console.log(`   Bump: ${stateAccount.bump}\n`);

    console.log("🎯 Protocol is ready!");
    console.log("   ✅ Real position configured");
    console.log("   ✅ Prices initialized");
    console.log("   ✅ Whirlpool linked\n");

    console.log("🚀 Next Steps:");
    console.log("   1. Test inflation: npx ts-node scripts/test_inflation.ts");
    console.log("   2. Check protocol state anytime with test_inflation.ts");
    console.log("   3. Implement Phase 2: Liquidity rebalancing\n");

  } catch (err: any) {
    console.error("\n❌ Error initializing protocol:");
    console.error(`   ${err.message}`);
    if (err.logs) {
      console.error("\n📋 Transaction logs:");
      err.logs.forEach((log: string) => console.error(`   ${log}`));
    }
    throw err;
  }
}

initializeProtocolWithPosition().catch((err) => {
  console.error(err);
  process.exit(1);
});
