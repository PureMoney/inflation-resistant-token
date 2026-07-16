import { AnchorProvider, Program, Wallet } from "@coral-xyz/anchor";
import pkg from "@coral-xyz/anchor";
const { BN } = pkg;
import {
    Connection,
    PublicKey,
    SystemProgram,
    Keypair,
    ComputeBudgetProgram,
} from "@solana/web3.js";
import { getAssociatedTokenAddress } from "@solana/spl-token";
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

// Load IDLs
const idl = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../target/idl/irma.json"), "utf-8")
);
const dlmmIdl = JSON.parse(
  fs.readFileSync(path.join(__dirname, "../idls/dlmm.json"), "utf-8")
);

const PROGRAM_ID = new PublicKey(idl.address);
const DLMM_PROGRAM_ID = new PublicKey(dlmmIdl.address);

console.log("🆔 IRMA Program ID:", PROGRAM_ID.toBase58());
console.log("🆔 DLMM Program ID:", DLMM_PROGRAM_ID.toBase58());

// Utility to convert bin ID to bin array index
function binIdToBinArrayIndex(binId: number): number {
    return Math.floor(binId / 70);
}

// Utility to derive bin array PDA
function deriveBinArrayPda(lbPair: PublicKey, binId: number): PublicKey {
    const binArrayIndex = binIdToBinArrayIndex(binId);
    const binArrayIdxBuffer = Buffer.alloc(8);
    binArrayIdxBuffer.writeBigInt64LE(BigInt(binArrayIndex));
    const [binArrayPda] = PublicKey.findProgramAddressSync(
        [Buffer.from("bin_array"), lbPair.toBuffer(), binArrayIdxBuffer],
        DLMM_PROGRAM_ID
    );
    return binArrayPda;
}

// Utility to get Token Program ID for a mint
async function getTokenProgram(connection: Connection, mint: PublicKey): Promise<PublicKey> {
    const info = await connection.getAccountInfo(mint);
    if (!info) {
        throw new Error(`Mint account not found: ${mint.toBase58()}`);
    }
    return info.owner;
}

async function main() {
    const args = process.argv.slice(2);
    if (args.length < 2) {
        console.error("❌ Usage:");
        console.error("   npx ts-node tests/test_limit_order.ts place <tokenSymbol> <ask|bid> <amount> [binId]");
        console.error("   npx ts-node tests/test_limit_order.ts cancel <tokenSymbol> <limitOrderPubkey> <binId1,binId2,...> [limitOrderPrivateKeyJsonPath]");
        console.error("   npx ts-node tests/test_limit_order.ts close <limitOrderPubkey> [limitOrderPrivateKeyJsonPath]");
        process.exit(1);
    }

    const action = args[0].toLowerCase();

    // Setup connection and provider
    const rpcUrl = process.env.ANCHOR_PROVIDER_URL || process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
    const commitment = (process.env.ANCHOR_COMMITMENT || process.env.SOLANA_COMMITMENT || "confirmed") as any;
    console.log(`🌐 Connecting to: ${rpcUrl} (Commitment: ${commitment})`);

    const connection = new Connection(rpcUrl, commitment);

    // Load IRMA Admin keypair
    let adminKeypair: Keypair;
    if (process.env.SOLANA_PRIVATE_KEY) {
        try {
            const privateKeyArray = JSON.parse(process.env.SOLANA_PRIVATE_KEY);
            adminKeypair = Keypair.fromSecretKey(new Uint8Array(privateKeyArray));
            console.log("🔑 Loaded admin keypair from SOLANA_PRIVATE_KEY");
        } catch (error) {
            console.log("❌ Failed to parse SOLANA_PRIVATE_KEY, generating temporary keypair");
            adminKeypair = Keypair.generate();
        }
    } else {
        adminKeypair = Keypair.generate();
        console.log("🔑 Generated temporary admin keypair:", adminKeypair.publicKey.toBase58());
    }

    const wallet = new Wallet(adminKeypair);
    const provider = new AnchorProvider(connection, wallet, { commitment });
    const program = new Program(idl, provider);
    const dlmmProgram = new Program(dlmmIdl, provider);

    // Derive IRMA PDAs
    const [statePda] = PublicKey.findProgramAddressSync([Buffer.from("state_v5")], PROGRAM_ID);
    const [corePda] = PublicKey.findProgramAddressSync([Buffer.from("core_v5")], PROGRAM_ID);

    console.log(`📍 State PDA: ${statePda.toBase58()}`);
    console.log(`📍 Core PDA: ${corePda.toBase58()}`);

    if (action === "place") {
        const symbol = args[1].toLowerCase();
        const side = args[2].toLowerCase();
        const amountStr = args[3];
        const isAskSide = side === "ask";

        if (!config.pools[symbol]) {
            throw new Error(`Pool not configured for token: ${symbol}`);
        }

        const poolConfig = config.pools[symbol];
        const lbPairKey = new PublicKey(poolConfig.address);
        console.log(`\n🔍 Fetching LbPair state for ${symbol.toUpperCase()} pool: ${lbPairKey.toBase58()}`);

        const lbPairData = await (dlmmProgram.account as any).lbPair.fetch(lbPairKey);
        const tokenXMint = new PublicKey(poolConfig.tokenX);
        const tokenYMint = new PublicKey(poolConfig.tokenY);
        const activeBinId = lbPairData.activeId;

        console.log(`   Token X Mint: ${tokenXMint.toBase58()}`);
        console.log(`   Token Y Mint: ${tokenYMint.toBase58()}`);
        console.log(`   Active Bin ID: ${activeBinId}`);

        // Set bin ID
        let binId: number;
        if (args[4]) {
            binId = parseInt(args[4]);
        } else {
            // Default to 10 bins away from active bin
            binId = isAskSide ? activeBinId + 10 : activeBinId - 10;
        }
        console.log(`🎯 Target Bin ID for Limit Order: ${binId}`);

        // Parse amount
        const amountFloat = parseFloat(amountStr);
        const tokenDecimals = isAskSide ? config.tokens.irma.decimals : config.tokens[symbol].decimals;
        const amountRaw = new BN(Math.round(amountFloat * Math.pow(10, tokenDecimals)));
        console.log(`💰 Amount: ${amountFloat} (Raw: ${amountRaw.toString()})`);

        // Generate limit order keypair
        const limitOrderKeypair = Keypair.generate();
        console.log(`📝 Generated Limit Order Account: ${limitOrderKeypair.publicKey.toBase58()}`);

        // Save private key array so user can interact with it later
        const keypairPath = path.join(__dirname, `../limit_order_${limitOrderKeypair.publicKey.toBase58().substring(0, 8)}.json`);
        fs.writeFileSync(keypairPath, JSON.stringify(Array.from(limitOrderKeypair.secretKey)));
        console.log(`💾 Saved Limit Order private key to: ${keypairPath}`);

        // Derive event authority
        const [eventAuthority] = PublicKey.findProgramAddressSync(
            [Buffer.from("__event_authority")],
            DLMM_PROGRAM_ID
        );

        // Derive bin array
        const binArrayPda = deriveBinArrayPda(lbPairKey, binId);
        console.log(`📍 Derived Bin Array PDA: ${binArrayPda.toBase58()}`);

        // Determine token details
        const tokenMint = isAskSide ? tokenXMint : tokenYMint;
        const tokenProgram = await getTokenProgram(connection, tokenMint);
        const userToken = await getAssociatedTokenAddress(tokenMint, adminKeypair.publicKey, false, tokenProgram);
        const reserve = isAskSide ? lbPairData.reserveX : lbPairData.reserveY;

        console.log(`   User Token ATA: ${userToken.toBase58()}`);
        console.log(`   Pool Reserve: ${reserve.toBase58()}`);

        // Build remaining accounts array in exact match with instructions.accounts + remaining_accounts_vec
        // Maint accounts struct in IRMA has: state, irma_admin, core, system_program
        // DLMM place_limit_order CPI requires:
        // - lb_pair
        // - bin_array_bitmap_extension (can be omitted since it's None in Rust code, wait, let's verify if CPI uses None or if it expects a placeholder.
        //   Wait, since DLMM IDL lists bin_array_bitmap_extension as optional, and in Rust code they pass `bin_array_bitmap_extension: None`,
        //   the generated Rust client will omit it if it's the last optional account, or pass dummy. But here it's in the middle. Let's see what is done.
        //   Actually, let's pass all possible accounts that could be in instructions.accounts:
        //   Let's check the accounts sequence.
        
        const remainingAccounts = [
            { pubkey: lbPairKey, isSigner: false, isWritable: true },
            { pubkey: reserve, isSigner: false, isWritable: true },
            { pubkey: tokenMint, isSigner: false, isWritable: false },
            { pubkey: limitOrderKeypair.publicKey, isSigner: true, isWritable: true },
            { pubkey: adminKeypair.publicKey, isSigner: true, isWritable: true }, // payer/owner/sender
            { pubkey: userToken, isSigner: false, isWritable: true },
            { pubkey: tokenProgram, isSigner: false, isWritable: false },
            { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
            { pubkey: eventAuthority, isSigner: false, isWritable: false },
            { pubkey: DLMM_PROGRAM_ID, isSigner: false, isWritable: false },
            { pubkey: binArrayPda, isSigner: false, isWritable: true },
        ];

        console.log("🚀 Building placeLimitOrder transaction...");
        const tx = await program.methods
            .placeLimitOrder(
                config.tokens[symbol].name,
                limitOrderKeypair.publicKey,
                isAskSide,
                binId,
                amountRaw
            )
            .accounts({
                state: statePda,
                irmaAdmin: adminKeypair.publicKey,
                core: corePda,
                systemProgram: SystemProgram.programId,
            })
            .remainingAccounts(remainingAccounts)
            .transaction();

        // Add compute budget
        tx.instructions.unshift(
            ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 })
        );

        console.log("✍️ Signing and sending transaction...");
        const signature = await connection.sendTransaction(tx, [adminKeypair, limitOrderKeypair]);
        console.log(`🚀 Transaction broadcasted! Signature: ${signature}`);
        console.log(`🔍 View transaction: https://solscan.io/tx/${signature}?cluster=devnet`);

        const confirmation = await connection.confirmTransaction(signature, "confirmed");
        if (confirmation.value.err) {
            console.error("❌ Transaction failed:", confirmation.value.err);
        } else {
            console.log("✅ Limit Order successfully placed!");
        }

    } else if (action === "cancel") {
        const symbol = args[1].toLowerCase();
        const limitOrderPubkey = new PublicKey(args[2]);
        const binIds = args[3].split(",").map(id => parseInt(id));
        const keypairPath = args[4] || path.join(__dirname, `../limit_order_${limitOrderPubkey.toBase58().substring(0, 8)}.json`);

        if (!fs.existsSync(keypairPath)) {
            throw new Error(`Limit order keypair file not found: ${keypairPath}. Please specify the correct path.`);
        }

        const limitOrderKeypair = Keypair.fromSecretKey(
            new Uint8Array(JSON.parse(fs.readFileSync(keypairPath, "utf-8")))
        );

        if (!config.pools[symbol]) {
            throw new Error(`Pool not configured for token: ${symbol}`);
        }

        const poolConfig = config.pools[symbol];
        const lbPairKey = new PublicKey(poolConfig.address);
        console.log(`\n🔍 Fetching LbPair state for ${symbol.toUpperCase()} pool: ${lbPairKey.toBase58()}`);

        const lbPairData = await (dlmmProgram.account as any).lbPair.fetch(lbPairKey);
        const tokenXMint = new PublicKey(poolConfig.tokenX);
        const tokenYMint = new PublicKey(poolConfig.tokenY);

        const tokenXProgram = await getTokenProgram(connection, tokenXMint);
        const tokenYProgram = await getTokenProgram(connection, tokenYMint);

        const ownerTokenX = await getAssociatedTokenAddress(tokenXMint, adminKeypair.publicKey, false, tokenXProgram);
        const ownerTokenY = await getAssociatedTokenAddress(tokenYMint, adminKeypair.publicKey, false, tokenYProgram);

        // Derive event authority
        const [eventAuthority] = PublicKey.findProgramAddressSync(
            [Buffer.from("__event_authority")],
            DLMM_PROGRAM_ID
        );

        const memoProgram = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

        // Derive bin arrays
        const binArrayPdas = binIds.map(binId => deriveBinArrayPda(lbPairKey, binId));

        const remainingAccounts = [
            { pubkey: lbPairKey, isSigner: false, isWritable: true },
            { pubkey: lbPairData.reserveX, isSigner: false, isWritable: true },
            { pubkey: lbPairData.reserveY, isSigner: false, isWritable: true },
            { pubkey: tokenXMint, isSigner: false, isWritable: false },
            { pubkey: tokenYMint, isSigner: false, isWritable: false },
            { pubkey: limitOrderPubkey, isSigner: true, isWritable: true },
            { pubkey: ownerTokenX, isSigner: false, isWritable: true },
            { pubkey: ownerTokenY, isSigner: false, isWritable: true },
            { pubkey: adminKeypair.publicKey, isSigner: true, isWritable: true }, // owner
            { pubkey: tokenXProgram, isSigner: false, isWritable: false },
            { pubkey: tokenYProgram, isSigner: false, isWritable: false },
            { pubkey: memoProgram, isSigner: false, isWritable: false },
            { pubkey: eventAuthority, isSigner: false, isWritable: false },
            { pubkey: DLMM_PROGRAM_ID, isSigner: false, isWritable: false },
            ...binArrayPdas.map(pda => ({ pubkey: pda, isSigner: false, isWritable: true }))
        ];

        console.log("🚀 Building cancelLimitOrder transaction...");
        const tx = await program.methods
            .cancelLimitOrder(
                config.tokens[symbol].name,
                limitOrderPubkey,
                binIds
            )
            .accounts({
                state: statePda,
                irmaAdmin: adminKeypair.publicKey,
                core: corePda,
                systemProgram: SystemProgram.programId,
            })
            .remainingAccounts(remainingAccounts)
            .transaction();

        tx.instructions.unshift(
            ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 })
        );

        console.log("✍️ Signing and sending transaction...");
        // Both the admin and the limit order need to sign!
        const signature = await connection.sendTransaction(tx, [adminKeypair, limitOrderKeypair]);
        console.log(`🚀 Transaction broadcasted! Signature: ${signature}`);
        console.log(`🔍 View transaction: https://solscan.io/tx/${signature}?cluster=devnet`);

        const confirmation = await connection.confirmTransaction(signature, "confirmed");
        if (confirmation.value.err) {
            console.error("❌ Transaction failed:", confirmation.value.err);
        } else {
            console.log("✅ Limit Order successfully canceled!");
        }

    } else if (action === "close") {
        const limitOrderPubkey = new PublicKey(args[1]);
        const keypairPath = args[2] || path.join(__dirname, `../limit_order_${limitOrderPubkey.toBase58().substring(0, 8)}.json`);

        if (!fs.existsSync(keypairPath)) {
            throw new Error(`Limit order keypair file not found: ${keypairPath}. Please specify the correct path.`);
        }

        const limitOrderKeypair = Keypair.fromSecretKey(
            new Uint8Array(JSON.parse(fs.readFileSync(keypairPath, "utf-8")))
        );

        // Derive event authority
        const [eventAuthority] = PublicKey.findProgramAddressSync(
            [Buffer.from("__event_authority")],
            DLMM_PROGRAM_ID
        );

        const remainingAccounts = [
            { pubkey: limitOrderPubkey, isSigner: true, isWritable: true },
            { pubkey: adminKeypair.publicKey, isSigner: true, isWritable: true }, // owner
            { pubkey: adminKeypair.publicKey, isSigner: false, isWritable: true }, // rent_receiver
            { pubkey: eventAuthority, isSigner: false, isWritable: false },
            { pubkey: DLMM_PROGRAM_ID, isSigner: false, isWritable: false },
        ];

        console.log("🚀 Building closeLimitOrderIfEmpty transaction...");
        const tx = await program.methods
            .closeLimitOrderIfEmpty(
                limitOrderPubkey
            )
            .accounts({
                state: statePda,
                irmaAdmin: adminKeypair.publicKey,
                core: corePda,
                systemProgram: SystemProgram.programId,
            })
            .remainingAccounts(remainingAccounts)
            .transaction();

        tx.instructions.unshift(
            ComputeBudgetProgram.setComputeUnitLimit({ units: 200_000 })
        );

        console.log("✍️ Signing and sending transaction...");
        const signature = await connection.sendTransaction(tx, [adminKeypair, limitOrderKeypair]);
        console.log(`🚀 Transaction broadcasted! Signature: ${signature}`);
        console.log(`🔍 View transaction: https://solscan.io/tx/${signature}?cluster=devnet`);

        const confirmation = await connection.confirmTransaction(signature, "confirmed");
        if (confirmation.value.err) {
            console.error("❌ Transaction failed:", confirmation.value.err);
        } else {
            console.log("✅ Limit Order successfully closed and rent recovered!");
        }
    } else {
        console.error(`❌ Unknown action: ${action}`);
        process.exit(1);
    }
}

main().catch(error => {
    console.error("❌ Error running script:");
    console.error(error);
    process.exit(1);
});
