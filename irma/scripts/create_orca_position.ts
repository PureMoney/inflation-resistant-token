import { Connection, Keypair, PublicKey, SystemProgram, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function createOrcaPosition() {
  console.log("\n🌊 Creating Real Orca Position");
  console.log("================================\n");

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

  const balance = await connection.getBalance(wallet.publicKey);
  console.log(`Wallet: ${wallet.publicKey.toBase58()}`);
  console.log(`Balance: ${(balance / LAMPORTS_PER_SOL).toFixed(4)} SOL\n`);

  const whirlpoolAddress = new PublicKey(config.orca.whirlpool);
  const irmaMint = new PublicKey(config.tokens.irma.mint);
  const usdcMint = new PublicKey(config.tokens.usdc.mint);

  console.log(`📍 Whirlpool: ${whirlpoolAddress.toBase58()}`);
  console.log(`🪙  IRMA: ${irmaMint.toBase58()}`);
  console.log(`💵 USDC: ${usdcMint.toBase58()}\n`);

  try {
    // Get token balances first
    console.log("📊 Checking token balances...");
    const irmaAta = await connection.getTokenAccountsByOwner(wallet.publicKey, { mint: irmaMint });
    const usdcAta = await connection.getTokenAccountsByOwner(wallet.publicKey, { mint: usdcMint });

    if (irmaAta.value.length === 0 || usdcAta.value.length === 0) {
      console.log("\n❌ Error: Token accounts not found");
      console.log("   Please create token accounts for IRMA and USDC first:");
      console.log("   spl-token create-account <MINT> --url devnet\n");
      process.exit(1);
    }

    const irmaTokenAccount = irmaAta.value[0];
    const usdcTokenAccount = usdcAta.value[0];

    // Get token amounts
    const irmaBalance = await connection.getTokenAccountBalance(irmaTokenAccount.pubkey);
    const usdcBalance = await connection.getTokenAccountBalance(usdcTokenAccount.pubkey);

    console.log(`   IRMA: ${irmaBalance.value.amount} (decimals: ${irmaBalance.value.decimals})`);
    console.log(`   USDC: ${usdcBalance.value.amount} (decimals: ${usdcBalance.value.decimals})\n`);

    const irmaAmountBN = BigInt(irmaBalance.value.amount);
    const usdcAmountBN = BigInt(usdcBalance.value.amount);

    if (irmaAmountBN === 0n || usdcAmountBN === 0n) {
      console.log("❌ Error: Insufficient token balances");
      console.log("   You need both IRMA and USDC tokens to create a position\n");
      process.exit(1);
    }

    console.log("💧 Fetching whirlpool data...\n");

    // Import Orca SDK
    const { WhirlpoolContext, AccountFetcher, ORCA_WHIRLPOOL_PROGRAM_ID } = await import("@orca-so/whirlpools-sdk");
    const BN = (await import("bn.js")).default;

    // Create Orca context
    const ctx = WhirlpoolContext.withProvider(provider, ORCA_WHIRLPOOL_PROGRAM_ID);
    const fetcher = new AccountFetcher(ctx.connection);

    // Get whirlpool account
    const whirlpoolAccount = await fetcher.getPool(whirlpoolAddress, true);
    if (!whirlpoolAccount) {
      throw new Error("Whirlpool account not found");
    }

    console.log("✅ Whirlpool data fetched");
    console.log(`   Tick spacing: ${whirlpoolAccount.tickSpacing}`);
    console.log(`   Current tick: ${whirlpoolAccount.tickCurrentIndex}`);
    console.log(`   Fee rate: ${whirlpoolAccount.feeRate} bps`);
    console.log(`   Liquidity: ${whirlpoolAccount.liquidity.toString()}\n`);

    // Define position range
    const tickSpacing = whirlpoolAccount.tickSpacing;
    const currentTick = whirlpoolAccount.tickCurrentIndex;
    
    // Round current tick to nearest tick spacing
    const lowerTick = Math.floor(currentTick / tickSpacing) * tickSpacing;
    const upperTick = lowerTick + (tickSpacing * 128); // 128 positions up

    console.log(`📍 Position range:`);
    console.log(`   Lower tick: ${lowerTick}`);
    console.log(`   Upper tick: ${upperTick}`);
    console.log(`   Current tick: ${currentTick}\n`);

    // Create position mint keypair (this would be used to create the NFT)
    console.log("📝 Generating position mint keypair...");
    const positionMint = Keypair.generate();

    console.log(`   Position Mint: ${positionMint.publicKey.toBase58()}`);
    console.log(`   (Save this keypair to deploy the position)\n`);

  } catch (err: any) {
    console.error("\n❌ Error:");
    console.error(`   ${err.message}`);
    if (err.logs) {
      console.error("\n📋 Logs:");
      err.logs.forEach((log: string) => console.error(`   ${log}`));
    }
    throw err;
  }
}

createOrcaPosition().catch((err) => {
  console.error(err);
  process.exit(1);
});
