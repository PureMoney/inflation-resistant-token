const { Connection, PublicKey } = require("@solana/web3.js");
const fs = require("fs");
const path = require("path");
require("dotenv").config({ path: path.join(__dirname, "../.env") });

const rpcUrl = process.env.ANCHOR_PROVIDER_URL || "https://api.devnet.solana.com";
const connection = new Connection(rpcUrl, "confirmed");

// Load IDLs
const idl = JSON.parse(fs.readFileSync(path.join(__dirname, "../target/idl/irma.json"), "utf-8"));
const dlmmIdl = JSON.parse(fs.readFileSync(path.join(__dirname, "../idls/dlmm.json"), "utf-8"));

const PROGRAM_ID = new PublicKey(idl.address);

// State PDA
const [statePda] = PublicKey.findProgramAddressSync([Buffer.from("state_v5")], PROGRAM_ID);

async function main() {
    console.log("Connecting to devnet and fetching program state...");
    
    // Anchor account fetcher simulation
    const stateInfo = await connection.getAccountInfo(statePda);
    if (!stateInfo) {
        throw new Error("State account not found");
    }

    // Use anchor library to fetch and decode account
    const anchor = require("@coral-xyz/anchor");
    const provider = new anchor.AnchorProvider(connection, new anchor.Wallet(anchor.web3.Keypair.generate()), {});
    const program = new anchor.Program(idl, provider);
    const dlmmProgram = new anchor.Program(dlmmIdl, provider);

    const state = await program.account.stateMap.fetch(statePda);
    
    const config = {
        network: "devnet",
        tokens: {},
        program: {
            programId: PROGRAM_ID.toBase58()
        },
        pools: {}
    };

    let irmaMint = null;

    // Fetch details for each reserve
    for (const reserve of state.reserves) {
        if (!reserve.active) continue;

        const symbol = reserve.symbol; // e.g. devUSDT
        const mint = reserve.mintAddress.toBase58();
        const poolId = reserve.poolId.toBase58();
        const decimals = reserve.backingDecimals.toNumber();

        // Query the mint account to find owner token program
        const mintInfo = await connection.getAccountInfo(reserve.mintAddress);
        const tokenProgram = mintInfo ? mintInfo.owner.toBase58() : "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

        // Map token key, e.g. devUSDT -> usdt
        const configKey = symbol.replace("dev", "").toLowerCase();
        
        config.tokens[configKey] = {
            mint,
            name: symbol,
            symbol: symbol.replace("dev", ""),
            decimals,
            program: tokenProgram
        };

        // Fetch pool (LbPair) data to get IRMA mint
        console.log(`Fetching pool ${poolId} details...`);
        const lbPairData = await dlmmProgram.account.lbPair.fetch(reserve.poolId);
        irmaMint = lbPairData.tokenXMint.toBase58();

        config.pools[configKey] = {
            address: poolId,
            tokenX: lbPairData.tokenXMint.toBase58(),
            tokenY: lbPairData.tokenYMint.toBase58()
        };
    }

    if (irmaMint) {
        config.tokens["irma"] = {
            mint: irmaMint,
            name: "IRMA",
            symbol: "IRMA",
            decimals: 6,
            program: "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        };
    }

    const configPath = path.join(__dirname, "../devnet-config.json");
    fs.writeFileSync(configPath, JSON.stringify(config, null, 2));
    console.log(`Successfully reconstructed config at: ${configPath}`);
}

main().catch(console.error);
