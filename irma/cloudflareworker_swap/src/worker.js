// 1. IMPORTS
import * as anchor from "@coral-xyz/anchor";
import { Buffer } from "buffer";
import DLMM, { StrategyType } from "@meteora-ag/dlmm";
import IDL from "../../target/idl/irma.json";

const { Connection, Keypair, PublicKey, SystemProgram, TransactionInstruction } = anchor.web3;
const { Program, AnchorProvider, BN, Wallet } = anchor;

const MEMO_PROGRAM_ID = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
const WORKER_MEMO_STRING = "IRMA_WORKER_SWAP";

// --- CUSTOM WALLET ---
// (Kept for fallback if needed, but we will try to use anchor.Wallet)
class CustomWallet {
  constructor(payer) {
    this.payer = payer;
  }
  async signTransaction(tx) {
    tx.partialSign(this.payer);
    return tx;
  }
  async signAllTransactions(txs) {
    return txs.map((t) => {
      t.partialSign(this.payer);
      return t;
    });
  }
  get publicKey() {
    return this.payer.publicKey;
  }
}

// ==================================================================
// CONFIGURATION
// ==================================================================
const HELIUS_API_KEY = process.env.HELIUS_API_KEY;
const HELIUS_RPC_URL = `https://devnet.helius-rpc.com/?api-key=${HELIUS_API_KEY}`;

// CONSTANTS
const RESERVE_MINT_STR = "J2JAep9untmdaQXXRYB1bxT2eFNWWeR8ApuRdAiY9gni"; 
const RESERVE_SYMBOL = "devUSDT";
const POOL_ADDRESS = "HYeXEBUxLM4aFYSBmHRhMLwMP5wGDXMtEHTtx3VevkTD"; 

// ==================================================================
// HELPER FUNCTIONS
// ==================================================================

/**
 * Get both mint and redemption prices from IRMA program
 */
async function getPrices(program, statePda, corePda, adminPublicKey, quoteToken) {
  try {
    const pricesResult = await program.methods
      .getPrices(quoteToken)
      .accounts({
        state: statePda,
        irmaAdmin: adminPublicKey,
        core: corePda,
        systemProgram: SystemProgram.programId,
      })
      .simulate();
    
    // Look for the "Program return" line in the raw logs
    const returnLine = pricesResult.raw.find((line) => 
      line.includes("Program return:")
    );
    
    if (returnLine) {
      // Extract the base64 data from the return line
      const base64Data = returnLine.split(' ').pop();
      if (base64Data) {
        // Decode the base64 data
        const decodedData = Buffer.from(base64Data, 'base64');
        
        // Read two f64 values (8 bytes each, little-endian)
        if (decodedData.length >= 16) {
          const mintPrice = decodedData.readDoubleLE(0);
          const redemptionPrice = decodedData.readDoubleLE(8);
          return { mintPrice, redemptionPrice };
        }
      }
    }
    
    throw new Error("Failed to parse prices from simulation");
  } catch (err) {
    console.error("Simulation failed:", err);
    console.error("Simulation error message:", err.message);
    if (err.logs) {
      console.error("Simulation logs:", err.logs);
    } else if (err.simulationResponse && err.simulationResponse.logs) {
       console.error("Simulation logs (from response):", err.simulationResponse.logs);
    }
    throw err;
  }
}

// ==================================================================
// WORKER LOGIC
// ==================================================================

export default {
  async fetch(request, env, ctx) {
    return handleRequest(request, env, ctx);
  }
};

async function handleRequest(request, env, ctx) {
  if (request.method !== 'POST') return new Response("Method not allowed", { status: 405 });

  try {
    const data = await request.json();
    const tx = data[0]; 

    // --- BASIC CHECKS ---
    if (!tx) return new Response("Ignored", { status: 200 });

    console.log(`🔔 tx.meta: ${tx.meta}`);
    // --- LOOP PREVENTION ---
    // Check for our specific Memo tag in the logs
    const logs = tx.meta.logMessages || [];
    const isWorkerSwap = logs.some(log => log.includes(WORKER_MEMO_STRING));
    if (isWorkerSwap) {
      console.log("🚫 Ignored Worker-Initiated Swap (Loop Prevention)");
      return new Response("Ignored (Worker Swap)", { status: 200 });
    }

    console.log(`🔍 Is there an error? ${tx.meta.err}`);
    if (!tx.meta || tx.meta.err !== null) return new Response("Ignored", { status: 200 });

    const isSwapInstruction = logs.some(log => 
      log.includes("Instruction: Swap2") || log.includes("Instruction: Swap")
    );
    if (!isSwapInstruction) return new Response("Ignored (Not a Swap)", { status: 200 });

    // Fire and forget - don't wait for completion
    ctx.waitUntil(processRebalance(tx, env));
    
    return new Response("Processing started", { status: 200 });

  } catch (err) {
    console.error("Worker Error:", err);
    return new Response("Error handled", { status: 200 });
  }
}

async function processRebalance(tx, env) {
  try {
    // --- DELTA CALC ---
    const preBalanceEntry = tx.meta.preTokenBalances.find(b => b.mint === RESERVE_MINT_STR && b.owner === POOL_ADDRESS);
    const postBalanceEntry = tx.meta.postTokenBalances.find(b => b.mint === RESERVE_MINT_STR && b.owner === POOL_ADDRESS);

    if (!preBalanceEntry || !postBalanceEntry) {
        console.log("Ignored (No Pool Balance)");
        return;
    }

    const preAmount = parseFloat(preBalanceEntry.uiTokenAmount.uiAmount);
    const postAmount = parseFloat(postBalanceEntry.uiTokenAmount.uiAmount);
    const delta = postAmount - preAmount;
    const decimals = preBalanceEntry.uiTokenAmount.decimals;

    if (delta !== 0) {
      console.log(`🚨 TRIGGER: ${delta > 0 ? "MINT" : "REDEMPTION"} Detected. Delta: ${delta}`);

      // --- SETUP ---
      const secretString = env.ADMIN_PRIVATE_KEY;
      const secretKey = new Uint8Array(JSON.parse(secretString));
      const connection = new Connection(HELIUS_RPC_URL, "confirmed");
      const adminKeypair = Keypair.fromSecretKey(secretKey);
      console.log(`🔑 Admin Public Key: ${adminKeypair.publicKey.toBase58()}`);
      
      // Use anchor.Wallet if available, otherwise CustomWallet
      const wallet = Wallet ? new Wallet(adminKeypair) : new CustomWallet(adminKeypair);
      
      const provider = new AnchorProvider(connection, wallet, { commitment: "confirmed" });
      
      // Mimic script: const program = new Program(idl, provider);
      // Note: The script imports 'idl' from JSON. We have 'IDL' imported.
      const program = new Program(IDL, provider);

      // Derive PDAs with v5 seeds
      const programId = new PublicKey(IDL.address);
      const [statePda] = PublicKey.findProgramAddressSync(
        [Buffer.from("state_v5")],
        programId
      );
      
      const [corePda] = PublicKey.findProgramAddressSync(
        [Buffer.from("core_v5")],
        programId
      );

      const atomicValueString = Math.floor(Math.abs(delta) * (10 ** decimals)).toString();
      const amountAtomic = new BN(atomicValueString);

      // Check minimum amount (e.g., 10 USDC = 10_000_000 atomic units)
      const MIN_AMOUNT = new BN(10_000_000); // 10 USDC minimum
      if (amountAtomic.lt(MIN_AMOUNT)) {
        console.log(`⏭️ Amount too small (${amountAtomic.toString()} < ${MIN_AMOUNT.toString()}), skipping`);
        return;
      }

      // --- GET PRICES FROM IRMA PROGRAM ---
      console.log(`📊 Fetching prices from IRMA program...`);
      let mintPrice, redemptionPrice;
      try {
        const prices = await getPrices(
          program,
          statePda,
          corePda,
          adminKeypair.publicKey,
          RESERVE_SYMBOL
        );
        mintPrice = prices.mintPrice;
        redemptionPrice = prices.redemptionPrice;
        console.log(`📊 Mint Price: ${mintPrice}, Redemption Price: ${redemptionPrice}`);
      } catch (priceError) {
        console.error(`❌ Failed to get prices:`, priceError.message);
        console.error(`Stack:`, priceError.stack);
        throw priceError;
      }

      // --- INITIALIZE DLMM POOL ---
      console.log(`📊 Initializing DLMM pool...`);
      const poolKey = new PublicKey(POOL_ADDRESS);
      let dlmmPool;
      try {
        dlmmPool = await DLMM.create(connection, poolKey);
        console.log(`✅ DLMM pool initialized`);
      } catch (dlmmError) {
        console.error(`❌ Failed to initialize DLMM:`, dlmmError.message);
        console.error(`Stack:`, dlmmError.stack);
        throw dlmmError;
      }

      // --- CONVERT PRICES TO BIN IDs ---
      const mintBinId = dlmmPool.getBinIdFromPrice(mintPrice.toString(), true);
      const redemptionBinId = dlmmPool.getBinIdFromPrice(redemptionPrice.toString(), false);
      const binDistance = Math.abs(mintBinId - redemptionBinId);
      
      console.log(`📍 Mint Bin: ${mintBinId}, Redemption Bin: ${redemptionBinId}, Distance: ${binDistance}`);

      // Check if rebalancing is needed (skip if same bin or adjacent bins)
      if (binDistance < 2) {
        console.log(`⏭️ Bins too close (distance: ${binDistance}), skipping rebalancing`);
        // Still call trade event but no counter-swap
        if (delta > 0) {
          await program.methods
            .saleTradeEvent(RESERVE_SYMBOL, amountAtomic)
            .accounts({
              state: statePda,
              irmaAdmin: adminKeypair.publicKey,
              core: corePda,
              systemProgram: SystemProgram.programId,
            })
            .remainingAccounts([
              { pubkey: poolKey, isSigner: false, isWritable: false }
            ])
            .rpc();
          console.log(`✅ Sale trade event recorded (no rebalancing)`);
        } else {
          await program.methods
            .buyTradeEvent(RESERVE_SYMBOL, amountAtomic)
            .accounts({
              state: statePda,
              irmaAdmin: adminKeypair.publicKey,
              core: corePda,
              systemProgram: SystemProgram.programId,
            })
            .remainingAccounts([
              { pubkey: poolKey, isSigner: false, isWritable: false }
            ])
            .rpc();
          console.log(`✅ Buy trade event recorded (no rebalancing)`);
        }
        return new Response("Processed (No Rebalancing)", { status: 200 });
      }

      // --- GET EXISTING POSITIONS ---
      const { userPositions } = await dlmmPool.getPositionsByUserAndLbPair(adminKeypair.publicKey);
      console.log(`📍 Found ${userPositions.length} position(s)`);
      
      for (const pos of userPositions) {
        console.log(`   Position ${pos.publicKey.toBase58()}: bins ${pos.positionData.lowerBinId} to ${pos.positionData.upperBinId}`);
      }

      // =========================================================
      // LOGIC: MINT EVENT (User bought IRMA with USDC)
      // =========================================================
      if (delta > 0) {
        console.log(`👉 Step 1: Performing counter-swap (IRMA -> USDC)...`);

        // Do counter-swap: Sell IRMA to get USDC back from mint bin
        // We have IRMA (Token X), want USDC (Token Y).
        // swapForY = true (Swap X for Y)
        const swapForY = true; 
        
        console.log("DEBUG: Fetching bin arrays...");
        const binArrays = await dlmmPool.getBinArrayForSwap(swapForY);
        
        console.log("DEBUG: Getting swap quote...");
        // Quote the swap - Use actual IRMA amount available after fees
        // Start with a smaller amount to account for liquidity/fees
        const irmaSwapAmount = amountAtomic.mul(new BN(95)).div(new BN(100)); // 95% of delta to be safe
        console.log(`💰 User delta: ${amountAtomic.toString()}, Counter-swap IRMA amount: ${irmaSwapAmount.toString()}`);
        
        const swapQuote = await dlmmPool.swapQuote(
          irmaSwapAmount, 
          swapForY,
          new BN(1500), // 15% slippage
          binArrays
        );
        
        const usdcOutputAmount = swapQuote.minOutAmount; // USDC we'll get back
        console.log(`💰 Expected USDC output: ${usdcOutputAmount.toString()}`);

        console.log("DEBUG: Creating swap transaction...");
        const swapTx = await dlmmPool.swap({
          inToken: dlmmPool.tokenX.publicKey, // IRMA
          outToken: dlmmPool.tokenY.publicKey, // USDC
          inAmount: irmaSwapAmount, // Use safe amount
          binArraysPubkey: swapQuote.binArraysPubkey,
          lbPair: poolKey,
          user: adminKeypair.publicKey,
          minOutAmount: usdcOutputAmount,
        });

        // Add Memo to prevent self-triggering loop
        if (swapTx.add) {
            swapTx.add(new TransactionInstruction({
                keys: [],
                programId: MEMO_PROGRAM_ID,
                data: Buffer.from(WORKER_MEMO_STRING, "utf-8"),
            }));
        }

        console.log("DEBUG: Sending transaction...");
        swapTx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
        swapTx.feePayer = adminKeypair.publicKey;
        swapTx.sign(adminKeypair);
        const swapSig = await connection.sendRawTransaction(swapTx.serialize(), { skipPreflight: false });
        console.log(`✅ Counter-swap sent: ${swapSig}`);

        console.log(`👉 Step 2: Adding USDC to redemption bin ${redemptionBinId}...`);

        // Find or create position for redemption bin
        let redemptionPosition = userPositions.find(pos => 
          pos.positionData.lowerBinId <= redemptionBinId && pos.positionData.upperBinId >= redemptionBinId
        );
        
        if (!redemptionPosition) {
          console.log(`📍 No position covers redemption bin ${redemptionBinId}, creating new position...`);
          
          const newPositionKeypair = Keypair.generate();
          const createPositionTx = await dlmmPool.initializePositionAndAddLiquidityByStrategy({
            positionPubKey: newPositionKeypair.publicKey,
            user: adminKeypair.publicKey,
            totalXAmount: new BN(0),
            totalYAmount: usdcOutputAmount, // Use actual USDC from counter-swap
            strategy: {
              minBinId: redemptionBinId,
              maxBinId: redemptionBinId,
              strategyType: StrategyType.Spot,
            },
          });
          
          for (const tx of Array.isArray(createPositionTx) ? createPositionTx : [createPositionTx]) {
            tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
            tx.feePayer = adminKeypair.publicKey;
            tx.partialSign(adminKeypair, newPositionKeypair);
            const sig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
            console.log(`✅ Created position and added liquidity: ${sig}`);
          }
        } else {
          console.log(`📍 Using existing position: ${redemptionPosition.publicKey.toBase58()}`);
          
          // Add USDC to redemption bin (Y-side only)
          const addLiquidityTx = await dlmmPool.addLiquidityByStrategy({
            positionPubKey: redemptionPosition.publicKey,
            user: adminKeypair.publicKey,
            totalXAmount: new BN(0), // No IRMA
            totalYAmount: usdcOutputAmount, // USDC we got from counter-swap
            strategy: {
              minBinId: redemptionBinId,
              maxBinId: redemptionBinId,
              strategyType: StrategyType.Spot,
            },
          });

          for (const tx of Array.isArray(addLiquidityTx) ? addLiquidityTx : [addLiquidityTx]) {
            tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
            tx.feePayer = adminKeypair.publicKey;
            tx.sign(adminKeypair);
            const addSig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
            console.log(`✅ Liquidity addition sent to redemption bin: ${addSig}`);
          }
        }

        console.log(`👉 Step 3: Recording sale trade event...`);

        // Call IRMA program with the original user's USDC amount
        console.log(`💰 Recording sale with token: "${RESERVE_SYMBOL}", amount: ${amountAtomic.toString()}`);
        
        await program.methods
          .saleTradeEvent(RESERVE_SYMBOL, amountAtomic)
          .accounts({
            state: statePda,
            irmaAdmin: adminKeypair.publicKey,
            core: corePda,
            systemProgram: SystemProgram.programId,
          })
          .remainingAccounts({
            pubkey: PublicKey(POOL_ADDRESS),
            isSigner: false,
            isWritable: false,
          })
          .rpc();

        console.log(`✅ Sale trade event recorded`);

      } else {
        // =========================================================
        // LOGIC: REDEMPTION EVENT (User sold IRMA for USDC)
        // =========================================================
        console.log(`👉 Step 1: Performing counter-swap (USDC -> IRMA)...`);

        // Do counter-swap: Sell USDC to get IRMA back
        // We have USDC (Token Y), want IRMA (Token X).
        // swapForY = false (Swap Y for X)
        const swapForY = false; 
        
        console.log("DEBUG: Fetching bin arrays...");
        const binArrays = await dlmmPool.getBinArrayForSwap(swapForY);
        
        console.log("DEBUG: Getting swap quote...");
        // Quote the swap - Use a safer amount to account for liquidity/fees
        const usdcSwapAmount = amountAtomic.abs().mul(new BN(95)).div(new BN(100)); // 95% of delta
        console.log(`💰 User delta: ${amountAtomic.toString()}, Counter-swap USDC amount: ${usdcSwapAmount.toString()}`);
        
        const swapQuote = await dlmmPool.swapQuote(
          usdcSwapAmount,
          swapForY,
          new BN(1500), // 15% slippage
          binArrays
        );

        const irmaOutputAmount = swapQuote.minOutAmount; // IRMA we'll get back
        console.log(`💰 Expected IRMA output: ${irmaOutputAmount.toString()}`);
        
        console.log("DEBUG: Creating swap transaction...");
        const swapTx = await dlmmPool.swap({
          inToken: dlmmPool.tokenY.publicKey, // USDC
          outToken: dlmmPool.tokenX.publicKey, // IRMA
          inAmount: usdcSwapAmount, // Use safe amount
          binArraysPubkey: swapQuote.binArraysPubkey,
          lbPair: poolKey,
          user: adminKeypair.publicKey,
          minOutAmount: irmaOutputAmount,
        });

        // Add Memo to prevent self-triggering loop
        if (swapTx.add) {
            swapTx.add(new TransactionInstruction({
                keys: [],
                programId: MEMO_PROGRAM_ID,
                data: Buffer.from(WORKER_MEMO_STRING, "utf-8"),
            }));
        }

        console.log("DEBUG: Sending transaction...");
        swapTx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
        swapTx.feePayer = adminKeypair.publicKey;
        swapTx.sign(adminKeypair);
        const swapSig = await connection.sendRawTransaction(swapTx.serialize(), { skipPreflight: false });
        console.log(`✅ Counter-swap sent: ${swapSig}`);

        console.log(`👉 Step 2: Adding IRMA to mint bin ${mintBinId}...`);

        // Add IRMA to mint bin (X-side only)
        const irmaAmount = irmaOutputAmount; // IRMA we got from counter-swap
        console.log(`💰 IRMA amount from counter-swap: ${irmaAmount.toString()}`);
        
        // Find or create position for mint bin
        let mintPosition = userPositions.find(pos => 
          pos.positionData.lowerBinId <= mintBinId && pos.positionData.upperBinId >= mintBinId
        );
        
        if (!mintPosition) {
          console.log(`📍 No position covers mint bin ${mintBinId}, creating new position...`);
          
          const newPositionKeypair = Keypair.generate();
          const createPositionTx = await dlmmPool.initializePositionAndAddLiquidityByStrategy({
            positionPubKey: newPositionKeypair.publicKey,
            user: adminKeypair.publicKey,
            totalXAmount: irmaAmount,
            totalYAmount: new BN(0),
            strategy: {
              minBinId: mintBinId,
              maxBinId: mintBinId,
              strategyType: StrategyType.Spot,
            },
          });
          
          for (const tx of Array.isArray(createPositionTx) ? createPositionTx : [createPositionTx]) {
            tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
            tx.feePayer = adminKeypair.publicKey;
            tx.partialSign(adminKeypair, newPositionKeypair);
            const sig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
            console.log(`✅ Created position and added liquidity: ${sig}`);
          }
        } else {
          console.log(`📍 Using existing position: ${mintPosition.publicKey.toBase58()}`);
          
          const addLiquidityTx = await dlmmPool.addLiquidityByStrategy({
            positionPubKey: mintPosition.publicKey,
            user: adminKeypair.publicKey,
            totalXAmount: irmaAmount, // IRMA we got
            totalYAmount: new BN(0), // No USDC
            strategy: {
              minBinId: mintBinId,
              maxBinId: mintBinId,
              strategyType: StrategyType.Spot,
            },
          });

          for (const tx of Array.isArray(addLiquidityTx) ? addLiquidityTx : [addLiquidityTx]) {
            tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
            tx.feePayer = adminKeypair.publicKey;
            tx.sign(adminKeypair);
            const addSig = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false });
            console.log(`✅ Liquidity addition sent to mint bin: ${addSig}`);
          }
        }

        console.log(`👉 Step 3: Recording buy trade event...`);

        // Call IRMA program - NOTE: For buy_trade_event, the bought_amount is the IRMA bought back
        await program.methods
          .buyTradeEvent(RESERVE_SYMBOL, irmaAmount)
          .accounts({
            state: statePda,
            irmaAdmin: adminKeypair.publicKey,
            core: corePda,
            systemProgram: SystemProgram.programId,
          })
          .remainingAccounts({
            pubkey: PublicKey(POOL_ADDRESS),
            isSigner: false,
            isWritable: false,
          })
          .rpc();

        console.log(`✅ Buy trade event recorded`);
      }

      console.log(`✅ Workflow Complete`);
    }

  } catch (err) {
    console.error("Worker Error:", err);
    console.error("Error message:", err.message);
    if (err.logs) {
      console.error("Program logs:", err.logs);
    }
  }
}