# Test Results
This doc expounds on some of the more interesting tests done on the IRMA program.

## Minting Test
Minting is very simple and un-interesting. The mint price is dictated by two outside measures, the reserve backing 
stablecoin market price and Truflation's inflation measure. While a reserve backing's market price should not deviate much 
from 1.0 USD, during times of high inflation we expect it to veer away from 1.0 USD, and this will vary depending on 
how much a stablecoin's users trust it. 

Assuming that inflation is below 2% when we set the IRMA loose in Solana mainnet, then IRMA will start out just like any other 
stablecoin and its mint price will be equal to 1.0 times every stablecoin in its reserve. (All reserves start out at zero at time = 0; 
therefore redemption price is zero and there is nothing to redeem.) The first buyers or miners of IRMA will start building up the reserves. 
The very first buyer of IRMA for USDT, for example, for 100 USDT say, would receive 100 IRMA and would therefore also cause the 
redemption price to be set at 1.0 USDT. As time goes, therefore, as long as inflation stays below 2%, IRMA price with respect 
to a backing stablecoin will be 1.0 unit of that stablecoin.

The functional test we want to do for minting starts with initial conditions such that minting price equals redemption price for 
every backing stablecoin. While mints and redemptions are going on, we would then suddenly raise the minting price, simulating 
a sudden rise of inflation beyond 2%. We expect the redemption price to go up as orders continue to come in, thereby approaching 
the mint price in time. That's it, that would be the functionality test for minting. However, at this point, we have only run 
unit tests on the mint_irma() function. This section of the doc will add more info later.

## "Total Redemption" Test
Note: these results were obtained by running test_redeem_irma_normal in 

https://github.com/PureMoney/inflation-resistant-stablecoin/blob/carlos/quick_demo/irma/programs/irma/tests/unit.rs

The redemption functionality test is much more interesting than any minting test we could think of. 

The IRMA program is designed to be fungible with respect to all backing stablecoins. In other words, once minted, each IRMA does 
not really care which stablecoin it is backed by, and it shouldn't. This is an important requirement. This implies that it should be OK 
to mint IRMA using USDT and then use the newly minted IRMA to redeem ANOTHER stablecoin, say USDC.

An arbitrageur, having detected a price difference between, say, USDT and USDC out there in the market that does not match the 
implied exchange rates of USDT and USDC in the IRMA system, can take advantage of the discrepancy. This arbitrageur can essentially 
use the IRMA system to exchange USDT for USDC, for example.

The IRMA system keeps track of the reserve backing total for each reserve stablecoin and also the amount of total IRMA in circulation 
for that stablecoin. When minting, the amount of stablecoin paid is added to the reserve backing total and the amount of IRMA minted is added 
to the total count of IRMA in circulation specific to that stablecoin. When redeeming, on the other hand, the naive way is to simply 
subtract the redeemed stablecoin amount from that stablecoin's reserve backing total, and also subtract the amount of IRMA returned 
from the total count of IRMA in circulation (specific to that stablecoin). This allows minting to adjust the redemption price towards 
the mint price; but redemptions would not affect the redemption price (because redemptions subtract from the IRMA in circulation 
according to the redemption price rather than the mint price).

The fungibility requirement can devastate IRMA if the redemption function simply subtracted both the input IRMA amount 
(from the total amount in circulation) and the redeemed stablecoin amount (from the reserve backing total). The IRMA program has to protect 
IRMA from possible runs because of the fungibility requirement. The naive way to redeem does not affect the redemption price, 
therefore redemptions can continue unabated at the same redemption price. To protect IRMA, the IRMA program must provide 
a dis-incentive for excessive redemptions. The following items provide this dis-incentive:

1. Redemption price is less than the mint price most of the time. This dis-incentive disappears at times.
2. The redemption function limits the per-redemption amount to 100K IRMA, but does not restrict the number of times redemptions can occur.
3. The overall total IRMA in circulation is tracked, but not necessarily for each reserve stablecoin.

What item number 3 means is that a "run" can still occur for a reserve stablecoin (the IRMA system can run out of a particular stablecoin), 
but users can continue to redeem another reserve stablecoin. This protects IRMA by calculating all deviations from the mint prices and determining 
the redemption price with the most deviation from the mint price. The IRMA program then "assigns" the amount of IRMA for redemption to this 
stablecoin with the most deviation.

The redemption test results illustrated below show how the IRMA redemption function works. 

When inflation hits above 2%, the mint prices are adjusted according to inflation and according to the price of each reserve stablecoin. Here
we simply pick mint prices vastly different from each other in order to see clearly what would happen with the redemption prices in the face
of vastly different target mint prices. 

![IRMA_MintPrice_with_labels](https://github.com/user-attachments/assets/fe13f5a2-4ee5-471a-97d7-6faf1f0b04b9)

The graph below shows exactly what happens when USDT is redeemed millions of times at 100K each redemption. Note that there is no minting
going on at the same time; in fact, no other transactions are being fed to the IRMA program except the large number of redemptions, one after 
another in a loop, until an error occurs. Notice how the redemption price for USDT goes down as the redemptions occur, while the redemption
prices for the other stablecoins increase. This should be an effective dis-incentive.

![IRMA_RedemptionPrice_with_labels](https://github.com/user-attachments/assets/8d71dfd8-d008-4455-83f5-c3c169dfae95)

We are redeeming USDT only, so only the USDT reserve total goes down:

![IRMA_Reserve_with_labels](https://github.com/user-attachments/assets/26a72865-34d9-4a45-bfc1-d080df739f18)

On the other hand, IRMA in circulation for each stablecoin reserve changes for every redemption. Because redemption price is simply
reserve total divided by IRMA in circulation for a stablecoin, notice how the redemption price in the second graph above changes
to approach the mint price for each stablecoin, even when no minting is going on.

![IRMA_Circulation_with_labels](https://github.com/user-attachments/assets/f73d90a7-80ae-43be-b07d-2154eaf3a732)

---

## Phase 4 — Devnet Integration Test Matrix (IRMA-11)

Ran the parameterized `tests/test_swap.ts` (see `tests/update_reserve_lbpair.ts` and
`docs/phase4-integration-testing.md` for the linking prerequisite) against every linkable
reserve on devnet, exercising both `sale_trade_event` (mint) and `buy_trade_event` (redeem)
for each:

```bash
npx ts-node tests/test_swap.ts mo <symbol>   # mint  — sale_trade_event(symbol, 110_000_000)
npx ts-node tests/test_swap.ts ro <symbol>   # redeem — buy_trade_event(symbol, 10_000_000)
```

`devUSDC` is excluded from this matrix — it remains blocked on the mint mis-registration bug
documented in `docs/phase4-integration-testing.md` (`update_reserve_lbpair` rejects it with
`InvalidLbPairState` before any trade event can run).

### Results

| Reserve | Mint (`sale_trade_event`) | Redeem (`buy_trade_event`) | Mint Price | Redemption Price |
|---|---|---|---|---|
| devUSDT | ✅ confirmed `pvByz8BU...sChksiMhSM` | ✅ confirmed `u2rXbTMc...EKjoByVb` | 1.001 | 1.0 |
| devPYUSD | ✅ confirmed `4MrUa9qg...2MKVm5ttAL` | ✅ confirmed `2Ehb2x4m...JarYbAXr7k5U` | 1.0 | 1.0 |
| devUSDS | ✅ confirmed `4UjUMrLR...c1ogsZbSYm` | ✅ confirmed `3NEwiLwp...RbL8B2pxL` | 1.0 | 1.0 |
| devUSDG | ✅ confirmed `214Jxhrp...6i9xM8h` | ✅ confirmed `1ZSSN7LY...iwBad5yHv1` | 1.0 | 1.0 |
| devFDUSD | ✅ confirmed `pjjZTdrC...kJ9ZpPK` | ✅ confirmed `45ReCoKF...Rpp7dtJVE` | 1.0 | 1.0 |
| devUSDC | ⛔ skipped — blocked by `InvalidLbPairState` (see docs/phase4-integration-testing.md) | ⛔ skipped | — | — |

### Reserve / Decimals check (post-run `stateMap` snapshot)

All 6 reserves report `backingDecimals: 6`, `mintPrice` near 1.0 (consistent with devnet's
near-zero inflation conditions described above), and `backingReserves == irmaInCirculation`
for every pool we exercised — confirming the mint and redeem paths update both counters in
lockstep as expected for the "Total Redemption" fungibility model described earlier in this
doc:

| Reserve | Backing Reserves | IRMA in Circulation | Mint Price | Decimals |
|---|---|---|---|---|
| devUSDT | 211 | 211 | 1.001 | 6 |
| devPYUSD | 101 | 101 | 1.0 | 6 |
| devUSDS | 101 | 101 | 1.0 | 6 |
| devUSDG | 101 | 101 | 1.0 | 6 |
| devFDUSD | 101 | 101 | 1.0 | 6 |
| devUSDC | 1979 | 1976 | 1.0 | 6 |

Note: `devUSDC`'s figures predate this test run (carried over from earlier registration-phase
activity) and were not touched here, since its trades remain blocked.

### Reserve Ratio

Reserve ratio (= `backingReserves ÷ irmaInCirculation`, i.e. redemption price) holds at 1.0
for every reserve we exercised, matching the "mint price ≈ redemption price under low
inflation" expectation laid out at the top of this doc — `devUSDT` shows the only deviation
(`mintPrice = 1.001` vs `redemptionPrice = 1.0`), reflecting a slightly elevated inflation
input recorded for that reserve at test time; the redemption price is already tracking
toward it as designed.



# Meteora DLMM 0.12.0 Limit Order Test Results

This describes the test verification process and results for the native Meteora DLMM 0.12.0 limit order instructions on Solana Devnet.

## How to Test the Whole Limit Order Lifecycle Yourself

To execute the full limit order lifecycle on Devnet, follow these steps:

### Step 1: Synchronize Configuration (Optional)
If the on-chain reserves state changes or has been reinitialized, pull the latest on-chain pool and token configurations directly using:
```bash
node scripts/reconstruct_config.cjs
```

### Step 2: Place a Limit Order
Run the following command to place a limit order. This will automatically generate a new Limit Order keypair and save its private key locally (e.g., `limit_order_XXXXXXXX.json`).

* **Syntax**: `npx tsx tests/test_limit_order.ts place <tokenSymbol> <ask|bid> <amount> [binId]`
* **Example**:
  ```bash
  npx tsx tests/test_limit_order.ts place usdt ask 0.1
  ```
* **Expected Output**:
  - Displays generated Limit Order account address.
  - Saves private key file locally.
  - Broadcasts transaction and confirms placement.

### Step 3: Cancel the Limit Order
Cancel the placed limit order (which also claims any filled/unfilled proceeds) using the generated limit order pubkey and the bin ID target.

* **Syntax**: `npx tsx tests/test_limit_order.ts cancel <tokenSymbol> <limitOrderPubkey> <binId1,binId2,...> [limitOrderPrivateKeyJsonPath]`
* **Example**:
  ```bash
  npx tsx tests/test_limit_order.ts cancel usdt Sf4TRRtMHQe7BLJz51R2voy4RKSTExAXgdGcDyqTNMW 11
  ```
* **Expected Output**:
  - Broadcasts transaction.
  - Confirms cancellation and claims proceeds.

### Step 4: Close the Limit Order Account
Once the limit order is empty, close the account to recover the allocated SOL rent lamports.

* **Syntax**: `npx tsx tests/test_limit_order.ts close <limitOrderPubkey> [limitOrderPrivateKeyJsonPath]`
* **Example**:
  ```bash
  npx tsx tests/test_limit_order.ts close Sf4TRRtMHQe7BLJz51R2voy4RKSTExAXgdGcDyqTNMW
  ```
* **Expected Output**:
  - Broadcasts transaction.
  - Confirms account closure and rent recovery to the admin wallet.

---

## Live Devnet Test Run Results

The instructions were successfully executed and validated on-chain using the following runs:

### 1. Place Limit Order
* **Command**: `npx tsx tests/test_limit_order.ts place usdt ask 0.1`
* **Target Bin**: `11` (Active bin `1` + offset `10`)
* **Created Limit Order**: `Sf4TRRtMHQe7BLJz51R2voy4RKSTExAXgdGcDyqTNMW`
* **Transaction Signature**: `284T5BG76gANihH2FGxSXXSToq4dXqm5mVAZ6ioRkpsHNEeho7VYu7zyHZV4MjpWZ5NVAYqWCFYNL1CWnSbVdHnw`
* **Status**: ✅ **SUCCESS**

### 2. Cancel Limit Order
* **Command**: `npx tsx tests/test_limit_order.ts cancel usdt Sf4TRRtMHQe7BLJz51R2voy4RKSTExAXgdGcDyqTNMW 11`
* **Transaction Signature**: `3WVQe9oPjsZpbsUygHc7sEkiZxEd7f36tGAz62NwWyz5wNjqmvFiaCZoSFWs5TLDWdtSJ8L3xCUSGK4iL7x8jM9F`
* **Status**: ✅ **SUCCESS**

### 3. Close Limit Order
* **Command**: `npx tsx tests/test_limit_order.ts close Sf4TRRtMHQe7BLJz51R2voy4RKSTExAXgdGcDyqTNMW`
* **Transaction Signature**: `59SvfsknmhnWCCxKqNsmKDfRsqU3g85normcnZARSJDhhfc95AJWaVbmsmrsQ3MePWnFdcbMTVFm9UqCCdnMFA49`
* **Status**: ✅ **SUCCESS**
