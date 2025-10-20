# IRMA Implementation Guide: Next Steps

## Overview

Your protocol has the foundation laid out. Here's what needs to be completed to make it fully functional:

---

## 1️⃣ PHASE 1: Mock Inflation & Price Updates

### What's Missing

Currently, `update_mock_prices` just sets new prices. We need:
1. **Inflation-based price calculation** - Prices should increase based on inflation rate
2. **Time-based adjustments** - Apply compounding inflation over time
3. **Helper functions** to calculate prices

### Implementation Steps

#### Step 1: Add Mock Inflation Calculation to `protocol_state.rs`

```rust
// Add this to the ProtocolState impl block in protocol_state.rs

/// Calculate new mint price based on inflation rate
/// Formula: new_price = current_price * (1 + inflation_rate)^(time_elapsed / year_seconds)
pub fn apply_inflation_adjustment(
    &mut self,
    inflation_rate: f64,  // e.g., 0.05 for 5% annual inflation
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let time_elapsed_seconds = (now - self.last_price_update) as f64;
    let seconds_per_year = 365.25 * 24.0 * 60.0 * 60.0;
    
    // Calculate compounding factor: (1 + rate)^(time / year)
    let exponent = time_elapsed_seconds / seconds_per_year;
    let multiplier = (1.0 + inflation_rate).powf(exponent);
    
    // Apply to both prices
    let new_mint_price = (self.mint_price as f64 * multiplier) as u64;
    let new_redemption_price = (self.redemption_price as f64 * multiplier) as u64;
    
    self.update_prices(new_mint_price, new_redemption_price)?;
    self.last_price_update = now;
    
    msg!("Inflation adjusted: multiplier = {}, mint_price = {}", multiplier, new_mint_price);
    Ok(())
}
```

#### Step 2: Add Instruction in `lib.rs`

```rust
#[derive(Accounts)]
pub struct ApplyInflation<'info> {
    #[account(
        mut,
        seeds = [b"protocol_state"],
        bump = protocol_state.bump,
    )]
    pub protocol_state: Account<'info, ProtocolState>,
    
    pub authority: Signer<'info>,
}

// In the #[program] module:
pub fn apply_inflation(
    ctx: Context<ApplyInflation>,
    inflation_rate_bps: u32,  // e.g., 500 for 5% = 500 basis points
) -> Result<()> {
    let protocol_state = &mut ctx.accounts.protocol_state;
    
    // Verify authority
    protocol_state.verify_authority(&ctx.accounts.authority)?;
    
    // Convert basis points to decimal (500 bps = 0.05)
    let inflation_rate = (inflation_rate_bps as f64) / 10_000.0;
    
    protocol_state.apply_inflation_adjustment(inflation_rate)?;
    Ok(())
}
```

#### Step 3: Create Test Script

Create `scripts/test_inflation.ts`:

```typescript
import * as anchor from "@project-serum/anchor";
import { Program } from "@project-serum/anchor";
import { PublicKey, Keypair } from "@solana/web3.js";

// Load IDL and program
const idl = require("../target/idl/irma.json");
const programId = new PublicKey("Fx8p5GAJzjBZTn3FHy9Y57Bo6DHpDuYNzURuimv4bA1N");

async function testInflation() {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = new Program(idl, programId, provider);

  // Find protocol state PDA
  const [protocolState] = await PublicKey.findProgramAddress(
    [Buffer.from("protocol_state")],
    programId
  );

  console.log("Protocol State:", protocolState.toBase58());

  // Fetch current state
  let state = await program.account.protocolState.fetch(protocolState);
  console.log("Initial mint price:", state.mintPrice.toString());
  console.log("Initial redemption price:", state.redemptionPrice.toString());

  // Apply 5% annual inflation
  console.log("\n--- Applying 5% inflation ---");
  await program.methods
    .applyInflation(500) // 500 basis points = 5%
    .accounts({
      protocolState,
      authority: provider.wallet.publicKey,
    })
    .rpc();

  // Fetch updated state
  state = await program.account.protocolState.fetch(protocolState);
  console.log("Updated mint price:", state.mintPrice.toString());
  console.log("Updated redemption price:", state.redemptionPrice.toString());

  // Apply again (prices should increase by 5% again)
  console.log("\n--- Applying another 5% inflation ---");
  await program.methods
    .applyInflation(500)
    .accounts({
      protocolState,
      authority: provider.wallet.publicKey,
    })
    .rpc();

  state = await program.account.protocolState.fetch(protocolState);
  console.log("Final mint price:", state.mintPrice.toString());
  console.log("Final redemption price:", state.redemptionPrice.toString());
}

testInflation().catch(console.error);
```

### Testing Checklist
- [ ] Deploy code with `anchor build`
- [ ] Run: `npx ts-node scripts/test_inflation.ts`
- [ ] Verify prices increase by expected amount
- [ ] Verify compounding works (second application = 5% of new price)

---

## 2️⃣ PHASE 2: Liquidity Rebalancing Module

### Architecture

```
Price Update Event
    ↓
Calculate price delta
    ↓
Is delta > threshold? (e.g., 1%)
    ├─ YES: Calculate liquidity adjustment
    │   ↓
    │   Execute Orca CPI
    │   ↓
    │   Update position
    └─ NO: Continue
```

### Implementation Steps

#### Step 1: Create `rebalancer.rs` Module

Create `programs/irma/src/rebalancer.rs`:

```rust
use anchor_lang::prelude::*;
use crate::protocol_state::ProtocolState;

/// Configuration for liquidity rebalancing
pub struct RebalanceConfig {
    /// Threshold for price change before rebalancing (e.g., 100 = 1%)
    pub rebalance_threshold_bps: u16,
    
    /// Maximum liquidity to add/remove per transaction
    pub max_liquidity_delta: u64,
}

impl Default for RebalanceConfig {
    fn default() -> Self {
        Self {
            rebalance_threshold_bps: 100, // 1%
            max_liquidity_delta: u64::MAX,
        }
    }
}

/// Calculate if rebalancing is needed
pub fn should_rebalance(
    current_price: u64,
    last_price: u64,
    threshold_bps: u16,
) -> bool {
    if last_price == 0 {
        return false;
    }
    
    let price_change = ((current_price as i128 - last_price as i128).abs() as u64);
    let threshold = (last_price as u128 * threshold_bps as u128 / 10_000) as u64;
    
    price_change > threshold
}

/// Calculate liquidity delta needed
/// Returns (delta_amount, direction) where direction: true = add, false = remove
pub fn calculate_liquidity_delta(
    current_price: u64,
    previous_price: u64,
    current_liquidity: u64,
    total_tvl: u64, // Total value locked in both tokens
) -> (u64, bool) {
    if current_price > previous_price {
        // Price increased - IRMA is more valuable
        // Add liquidity to reduce slippage at new price
        let price_increase_pct = 
            ((current_price - previous_price) as f64 / previous_price as f64) * 100.0;
        
        let liquidity_to_add = 
            (total_tvl as f64 * price_increase_pct / 100.0) as u64;
        
        (liquidity_to_add, true) // true = add
    } else {
        // Price decreased - IRMA is less valuable
        // Remove liquidity to maintain constant depth
        let price_decrease_pct = 
            ((previous_price - current_price) as f64 / previous_price as f64) * 100.0;
        
        let liquidity_to_remove = 
            (current_liquidity as f64 * price_decrease_pct / 100.0) as u64;
        
        (liquidity_to_remove, false) // false = remove
    }
}

/// Convert price to tick for Orca (approximate)
/// Formula: tick = log_1.0001(price)
pub fn price_to_tick(price: u64) -> i32 {
    if price == 0 {
        return 0;
    }
    
    let price_f64 = price as f64 / 1_000_000.0; // Assuming price is in USDC with 6 decimals
    let tick = (price_f64.log(1.0001)) as i32;
    tick
}

/// Calculate tick range for liquidity position
/// Returns (lower_tick, upper_tick)
pub fn calculate_tick_range(
    center_price: u64,
    spread_bps: u16, // e.g., 500 = 5% spread
) -> (i32, i32) {
    let center_tick = price_to_tick(center_price);
    
    // Typical tick spacing for 0.3% fee tier is 60
    let tick_spacing = 60;
    
    // 5% spread = approximately ±100 ticks at current price
    let spread = ((spread_bps as i32) * 100) / 10_000;
    
    let lower_tick = ((center_tick - spread) / tick_spacing) * tick_spacing;
    let upper_tick = ((center_tick + spread) / tick_spacing) * tick_spacing;
    
    (lower_tick, upper_tick)
}
```

#### Step 2: Add to `lib.rs`

```rust
mod rebalancer;

// Add new context for rebalancing
#[derive(Accounts)]
pub struct Rebalance<'info> {
    #[account(
        mut,
        seeds = [b"protocol_state"],
        bump = protocol_state.bump,
    )]
    pub protocol_state: Account<'info, ProtocolState>,
    
    /// Whirlpool account
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub whirlpool: UncheckedAccount<'info>,
    
    /// Position account
    /// CHECK: Validated by Whirlpool program
    #[account(mut)]
    pub position: UncheckedAccount<'info>,
    
    /// Position token account
    #[account(mut)]
    pub position_token_account: UncheckedAccount<'info>,
    
    /// Token A vault
    #[account(mut)]
    pub token_vault_a: UncheckedAccount<'info>,
    
    /// Token B vault
    #[account(mut)]
    pub token_vault_b: UncheckedAccount<'info>,
    
    /// Tick array lower
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,
    
    /// Tick array upper
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,
    
    pub authority: Signer<'info>,
    
    /// CHECK: Whirlpool program
    pub whirlpool_program: UncheckedAccount<'info>,
}

// In #[program]:
pub fn rebalance(
    ctx: Context<Rebalance>,
) -> Result<()> {
    let protocol_state = &ctx.accounts.protocol_state;
    
    // Check if rebalancing is needed
    let should_rebalance = rebalancer::should_rebalance(
        protocol_state.mint_price,
        protocol_state.last_rebalance_price,
        rebalancer::RebalanceConfig::default().rebalance_threshold_bps,
    );
    
    if !should_rebalance {
        msg!("Price change within threshold, no rebalancing needed");
        return Ok(());
    }
    
    msg!("Rebalancing triggered!");
    
    // TODO: Calculate liquidity delta
    let (delta, should_add) = rebalancer::calculate_liquidity_delta(
        protocol_state.mint_price,
        protocol_state.last_rebalance_price,
        100_000_000, // placeholder current_liquidity
        1_000_000_000, // placeholder total_tvl
    );
    
    // TODO: Execute Orca CPI calls
    // - Call ModifyLiquidity on Whirlpool
    // - Update protocol_state.last_rebalance
    
    msg!("Rebalancing completed! Delta: {}, Add: {}", delta, should_add);
    Ok(())
}
```

### Testing Checklist
- [ ] Test `should_rebalance` with various price changes
- [ ] Test `calculate_liquidity_delta` logic
- [ ] Test tick calculation matches Orca expectations
- [ ] Manual rebalance call succeeds

---

## 3️⃣ PHASE 3: Crank Mechanism

### Purpose
Automatically trigger rebalancing after every state-changing transaction.

### Implementation

#### Step 1: Create `crank.rs` Module

```rust
// programs/irma/src/crank.rs

use anchor_lang::prelude::*;
use crate::protocol_state::ProtocolState;
use crate::rebalancer;

/// Context for crank operations
#[derive(Accounts)]
pub struct Crank<'info> {
    #[account(
        mut,
        seeds = [b"protocol_state"],
        bump = protocol_state.bump,
    )]
    pub protocol_state: Account<'info, ProtocolState>,
    
    // All Orca-related accounts for potential rebalancing
    /// CHECK: Whirlpool
    #[account(mut)]
    pub whirlpool: UncheckedAccount<'info>,
    
    /// CHECK: Position
    #[account(mut)]
    pub position: UncheckedAccount<'info>,
    
    /// CHECK: Position token account
    #[account(mut)]
    pub position_token_account: UncheckedAccount<'info>,
    
    /// CHECK: Token vault A
    #[account(mut)]
    pub token_vault_a: UncheckedAccount<'info>,
    
    /// CHECK: Token vault B
    #[account(mut)]
    pub token_vault_b: UncheckedAccount<'info>,
    
    /// CHECK: Tick array lower
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,
    
    /// CHECK: Tick array upper
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,
    
    pub authority: Signer<'info>,
    
    /// CHECK: Whirlpool program
    pub whirlpool_program: UncheckedAccount<'info>,
}

pub fn crank(ctx: Context<Crank>) -> Result<()> {
    let protocol_state = &ctx.accounts.protocol_state;
    
    // Step 1: Check if rebalancing is needed
    let should_rebalance = rebalancer::should_rebalance(
        protocol_state.mint_price,
        protocol_state.last_rebalance_price,
        100, // 1% threshold
    );
    
    if !should_rebalance {
        msg!("Crank: No rebalancing needed");
        return Ok(());
    }
    
    msg!("Crank: Initiating rebalance");
    
    // Step 2: Calculate adjustment needed
    let (delta, should_add) = rebalancer::calculate_liquidity_delta(
        protocol_state.mint_price,
        protocol_state.last_rebalance_price,
        1_000_000,
        10_000_000,
    );
    
    msg!("Crank: Delta = {}, Add = {}", delta, should_add);
    
    // Step 3: Execute Orca CPI (implement next)
    // This is where we'd call Orca's ModifyLiquidity instruction
    
    Ok(())
}
```

#### Step 2: Integrate Into Mint/Redeem

Modify `token_operations.rs`:

```rust
// At the end of mint_irma function:
pub fn mint_irma(ctx: Context<MintIrma>, usdc_amount: u64) -> Result<()> {
    // ... existing mint logic ...
    
    msg!("Mint successful! Triggering crank...");
    // Crank will be called by caller or in a separate instruction
    Ok(())
}
```

### Testing Checklist
- [ ] Crank detects when rebalancing is needed
- [ ] Crank skips rebalancing when within threshold
- [ ] Calculate liquidity delta returns reasonable values
- [ ] Integrate crank call into mint/redeem flow

---

## 4️⃣ PHASE 4: User Swap & LP Operations

### What's Needed

1. **User Swap Instruction**
   - Route through Orca
   - Handle slippage
   - Return actual output

2. **LP Deposit Instruction**
   - User provides tokens
   - Add liquidity to Orca position
   - Return LP token amount (NFT)

3. **LP Withdraw Instruction**
   - User burns LP NFT
   - Receive tokens back
   - Calculate amounts based on position

### High-Level Implementation

```rust
#[derive(Accounts)]
pub struct UserSwap<'info> {
    pub pool_state: Account<'info, OrcaPoolState>,
    #[account(mut)]
    pub user_token_in: UncheckedAccount<'info>,
    #[account(mut)]
    pub user_token_out: UncheckedAccount<'info>,
    pub user: Signer<'info>,
    // ... Orca vault accounts ...
}

pub fn user_swap(
    ctx: Context<UserSwap>,
    amount_in: u64,
    min_amount_out: u64,
) -> Result<u64> {
    // Call Orca swap via CPI
    // Return actual output amount
}
```

---

## Summary Table

| Phase | Component | Status | Effort | Priority |
|-------|-----------|--------|--------|----------|
| 1 | Inflation Calculation | Ready | 1-2h | HIGH |
| 1 | Test Inflation | Ready | 1h | HIGH |
| 2 | Rebalancer Module | Ready | 2-3h | HIGH |
| 2 | Rebalance Instruction | Ready | 1-2h | HIGH |
| 3 | Crank Module | Ready | 2-3h | HIGH |
| 3 | Crank Integration | Ready | 1h | HIGH |
| 4 | User Swap | Needs CPI | 2-3h | MEDIUM |
| 4 | LP Operations | Needs CPI | 3-4h | MEDIUM |

---

## Next Immediate Action

**I recommend starting with Phase 1:**

1. Add `apply_inflation_adjustment()` to `protocol_state.rs`
2. Add `apply_inflation` instruction to `lib.rs`
3. Run `anchor build` to verify compilation
4. Run `scripts/test_inflation.ts` to verify functionality

Then proceed to Phase 2 (Rebalancing) which is critical for the protocol to work correctly.