# IRMA Devnet Operations Runbook

End-to-end reference for setting up and operating IRMA on Solana devnet. Follow phases in order for a fresh deployment; jump to individual sections for maintenance.

---

## Prerequisites

| Tool | Version | Check |
|------|---------|-------|
| Solana CLI | ≥ 1.18 | `solana --version` |
| spl-token CLI | ≥ 5.3.0 | `spl-token --version` |
| Node.js | ≥ 20 | `node --version` |
| Anchor CLI | 0.32.1 | `anchor --version` |
| npx wrangler | ≥ 3 | `npx wrangler --version` |

All commands in `irma/` unless noted. Create a `.env` from `.env.example` before running TypeScript scripts:

```
ANCHOR_PROVIDER_URL=https://api.devnet.solana.com
SOLANA_PRIVATE_KEY=<phantom1 private key as JSON byte array>
```

> **RPC note**: Anchor's `.rpc()` method uses WebSockets for confirmation. Use `https://api.devnet.solana.com` (not Alchemy) in `ANCHOR_PROVIDER_URL`. Scripts that call `sendAndConfirmTransaction` directly work with any HTTP endpoint.

---

## Devnet Addresses (Source of Truth: `devnet-config.json`)

### Program
| | Address |
|--|--|
| IRMA Program | `E15v5VirGqdbH4fYhxxxZHNiLAP3t3y1SPonhrQxoTcs` |
| Admin wallet (phantom1) | `Bp45s9xUrXhR62256ThJLgHVMna5AAYLzVW1xzdbtK8q` |

### Token Mints
| Token | Mint | Token Program |
|-------|------|---------------|
| IRMA | `EwotD7KQ8TgdvrKWaFVLPRgQnt98Ltjq98UggzFcKbDY` | Token-2022 |
| devUSDC | `4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU` | Classic SPL |
| devUSDT | `4YKp5kpZCL7NLsrwu5vieTg2zhwdP5WZr8aLPdFTgawW` | Classic SPL |
| devPYUSD | `6o3jQd8APphmBdRS8umCaMnivZxV9HbdymBVmrwMAT6d` | Classic SPL |
| devUSDS | `CHD776oFbAhqDQ2iMwiZdScka5dy5FdLkouttsvFmoJD` | Classic SPL |
| devUSDG | `9oqY7wkh19PrJ7fCtthKqRqa41A4uXHv67Pc3WBZaGSx` | Classic SPL |
| devFDUSD | `4rDY847iWoNsbrHm1FNGzEyNBb6zJZT9gJMQL6FKQXfo` | Classic SPL |

### Meteora DLMM Pools (bin step 5 bps, fee 25 bps, active bin 0 = price 1.0)
| Pool | Address |
|------|---------|
| IRMA/USDC | `DyweFxgq7VzoViT6EWHw9o1Z9rmwifJ82bpLqh1rF6z3` |
| IRMA/USDT | `BCvW192j75p5ocrEddvDXPJBtt19xVaedzUR3tXHUBK1` |
| IRMA/PYUSD | `82tugvnxW6AwkRm2Ntf4YRK4he1acuyosazR57LWJjU6` |
| IRMA/USDS | `FQbLMLreVZkokwWubMSSXgjsiGRJjfDHDM7WH3Jg8DYh` |
| IRMA/USDG | `4f3VoNFFvhKHdQmtDzQVB6jXVcRFz5FFR4gPQDuYoxMZ` |
| IRMA/FDUSD | `hpmVtDBZxgm295nxFFuX5znVoo978GvntDjLoajLTS9` |

> Devnet pools are visible on the Meteora devnet UI: `https://devnet.meteora.ag`. You can also verify any pool directly on Solana Explorer: `https://explorer.solana.com/address/<POOL_ADDRESS>?cluster=devnet`

---

## Phase 1 — Token Setup

> Completed on devnet. Run this phase only when bootstrapping a fresh environment (e.g. mainnet).

### 1a. Create the admin wallet

```bash
solana-keygen new -o ~/.config/solana/phantom1.json --no-bip39-passphrase
solana config set --keypair ~/.config/solana/phantom1.json
solana config set --url devnet

# Fund it (airdrop is often rate-limited; transfer from an existing wallet if needed)
solana airdrop 2 --keypair ~/.config/solana/phantom1.json
# or: solana transfer --keypair ~/.config/solana/id.json <phantom1-pubkey> 1 --allow-unfunded-recipient
```

### 1b. Generate mint keypairs

```bash
mkdir -p irma/keypairs && cd irma/keypairs
solana-keygen new -o irma-mint.json     --no-bip39-passphrase --force
solana-keygen new -o devusdt-mint.json  --no-bip39-passphrase --force
solana-keygen new -o devpyusd-mint.json --no-bip39-passphrase --force
solana-keygen new -o devusds-mint.json  --no-bip39-passphrase --force
solana-keygen new -o devusdg-mint.json  --no-bip39-passphrase --force
solana-keygen new -o devfdusd-mint.json --no-bip39-passphrase --force
cd ../..
```

### 1c. Create IRMA mint (Token-2022)

```bash
spl-token create-token \
  --program-id TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb \
  --decimals 6 \
  --fee-payer ~/.config/solana/phantom1.json \
  --url devnet \
  irma/keypairs/irma-mint.json
```

### 1d. Create mock stablecoin mints (Classic SPL)

```bash
KDIR=irma/keypairs
FP=~/.config/solana/phantom1.json

spl-token create-token --decimals 6 --fee-payer $FP --url devnet $KDIR/devusdt-mint.json
spl-token create-token --decimals 6 --fee-payer $FP --url devnet $KDIR/devpyusd-mint.json
spl-token create-token --decimals 6 --fee-payer $FP --url devnet $KDIR/devusds-mint.json
spl-token create-token --decimals 6 --fee-payer $FP --url devnet $KDIR/devusdg-mint.json
spl-token create-token --decimals 6 --fee-payer $FP --url devnet $KDIR/devfdusd-mint.json
```

> **USDC**: devnet USDC uses the Circle faucet address (`4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU`). We cannot mint it; obtain from [faucet.circle.com](https://faucet.circle.com). We do not create a keypair for it.

### 1e. Update devnet-config.json

Record all mint addresses in `irma/devnet-config.json` under the `tokens` key. All subsequent scripts read from this file.

**Full context**: `docs/phase1-token-setup.md`

---

## Phase 2 — DLMM Pool Creation & Liquidity Seeding

> Completed on devnet. Run this phase only for a fresh environment or after closing all positions.

### 2a. Install dependencies

```bash
cd irma && npm install
```

### 2b. Create 6 DLMM pools

```bash
node scripts/create_dlmm_pools.cjs
```

The script is idempotent — if a pool already exists it records the address and moves on. Pool addresses are written back to `devnet-config.json` under `pools`.

> **Why `.cjs`**: The Meteora DLMM SDK's ESM build has directory import issues on Node.js v24. The `.cjs` extension forces CommonJS mode, which works cleanly.

### 2c. Seed initial liquidity

```bash
node scripts/seed_dlmm_liquidity.cjs
```

Opens exactly **one single-bin mint position per pool** at the active bin (price 1.0), depositing only IRMA. Do **not** open a redemption position manually — the IRMA on-chain program opens that automatically after the first swap.

**Full context**: `docs/phase2-pool-creation.md`

---

## Phase 3 — Program Registration

> Completed on devnet. Run only for a fresh environment.

### 3a. Build the program (generates IDL)

```bash
anchor build
```

The IDL at `target/idl/irma.json` is required by all TypeScript tests and scripts.

### 3b. Register each stablecoin as a reserve

```bash
npx ts-node tests/add_reserve.ts devUSDC  4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU 6
npx ts-node tests/add_reserve.ts devUSDT  4YKp5kpZCL7NLsrwu5vieTg2zhwdP5WZr8aLPdFTgawW 6
npx ts-node tests/add_reserve.ts devPYUSD 6o3jQd8APphmBdRS8umCaMnivZxV9HbdymBVmrwMAT6d 6
npx ts-node tests/add_reserve.ts devUSDS  CHD776oFbAhqDQ2iMwiZdScka5dy5FdLkouttsvFmoJD 6
npx ts-node tests/add_reserve.ts devUSDG  9oqY7wkh19PrJ7fCtthKqRqa41A4uXHv67Pc3WBZaGSx 6
npx ts-node tests/add_reserve.ts devFDUSD 4rDY847iWoNsbrHm1FNGzEyNBb6zJZT9gJMQL6FKQXfo 6
```

### 3c. Verify on-chain state

```bash
npx ts-node tests/read_protocol_state.ts
```

All 6 symbols should appear with correct mint addresses.

### 3d. Link each reserve to its Meteora LbPair

```bash
npx ts-node tests/update_reserve_lbpair.ts all
```

This calls `update_reserve_lbpair` for all 6 reserves. The program silently no-ops on `sale_trade_event`/`buy_trade_event` until this step is complete.

> **Known issue (devnet)**: `devUSDC` fails with `InvalidLbPairState` (error 6028) because it was mis-registered with a stale mint address. The root cause is a `partition_point` bug in `pricing.rs::remove_reserve` that prevents fixing the entry. Carlos has been notified. The other 5 reserves link successfully.

**Full context**: `docs/phase3-program-registration.md`

---

## Phase 4 — Integration Testing

### 4a. Run a mint (sale_trade_event)

```bash
npx ts-node tests/test_swap.ts mo <symbol>
# Example:
npx ts-node tests/test_swap.ts mo devUSDT
```

### 4b. Run a redeem (buy_trade_event)

```bash
npx ts-node tests/test_swap.ts ro <symbol>
# Example:
npx ts-node tests/test_swap.ts ro devUSDT
```

Valid symbols: `devUSDT`, `devPYUSD`, `devUSDS`, `devUSDG`, `devFDUSD` (`devUSDC` blocked — see Phase 3 known issue).

### 4c. Full test suite

```bash
yarn test
```

**Full context & test results**: `docs/phase4-integration-testing.md`

### 4d. Manual swap smoke test (Meteora devnet UI)

Verify each pool is live and swappable via the UI:

1. Import `phantom1.json` into Phantom wallet and switch to Devnet
2. Open each pool link below and perform a small swap (e.g. 0.01 stablecoin → IRMA)
3. Confirm the transaction succeeds and the pool state updates

| Pool | Meteora Devnet UI |
|------|-------------------|
| IRMA/USDC | https://devnet.meteora.ag/dlmm/DyweFxgq7VzoViT6EWHw9o1Z9rmwifJ82bpLqh1rF6z3 |
| IRMA/USDT | https://devnet.meteora.ag/dlmm/BCvW192j75p5ocrEddvDXPJBtt19xVaedzUR3tXHUBK1 |
| IRMA/PYUSD | https://devnet.meteora.ag/dlmm/82tugvnxW6AwkRm2Ntf4YRK4he1acuyosazR57LWJjU6 |
| IRMA/USDS | https://devnet.meteora.ag/dlmm/FQbLMLreVZkokwWubMSSXgjsiGRJjfDHDM7WH3Jg8DYh |
| IRMA/USDG | https://devnet.meteora.ag/dlmm/4f3VoNFFvhKHdQmtDzQVB6jXVcRFz5FFR4gPQDuYoxMZ |
| IRMA/FDUSD | https://devnet.meteora.ag/dlmm/hpmVtDBZxgm295nxFFuX5znVoo978GvntDjLoajLTS9 |

> **Token names not visible?** Token names and symbols require a separate on-chain Metaplex metadata account — `spl-token create-token` only creates the mint, not the metadata. The pools are fully functional; the UI just shows raw mint addresses instead of names until metadata is added. This is a known gap and can be addressed by running a metadata-creation script for each of the 7 mints (IRMA + 6 stablecoins).

> **Note**: devUSDC swaps may silently no-op due to the known mint mis-registration bug — see Phase 3 known issue.

---

## Phase 5 — Cloudflare Worker

### 5a. Install worker dependencies

```bash
cd irma/cloudflareworker_swap && npm install
```

### 5b. Set secrets (one-time, per environment)

```bash
for env in usdc usdt pyusd usds usdg fdusd; do
  npx wrangler secret put ADMIN_PRIVATE_KEY --env $env   # JSON byte array of phantom1.json
  npx wrangler secret put HELIUS_API_KEY    --env $env
done
```

`ADMIN_PRIVATE_KEY` is the admin keypair as a JSON byte array (e.g. `[1,2,3,...]`). Read it from the keypair file:
```bash
cat ~/.config/solana/phantom1.json
```

### 5c. (Optional) Deploy Truflation proxy

```bash
cd truflation-proxy && ./deploy.sh
# Writes the Vercel URL to .vercel-url so deploy.sh picks it up automatically
```

### 5d. Deploy

```bash
# All 6 environments
./deploy.sh

# Single environment
./deploy.sh usdt
```

### 5e. Smoke-test

```bash
WORKER_URL=https://irma-client-usdt.<account>.workers.dev

curl "$WORKER_URL/?action=health"
curl "$WORKER_URL/?action=fetch-inflation"
curl "$WORKER_URL/?action=update-mint-price"
```

### 5f. End-to-end webhook test

1. Trigger a swap via `npx ts-node tests/test_swap.ts mo devUSDT`
2. Watch worker logs: Cloudflare dashboard → Workers → `irma-client-usdt` → Logs (or Observability)
3. Expect: `sale_trade_event` → counter-swap → `check_shift_price_ranges`

**Full context**: `docs/phase5-cloudflare-worker.md`

---

## Maintenance Operations

### List open positions on a pool

```bash
node scripts/list_dlmm_positions.cjs
```

### Close all positions on a pool (withdraw liquidity first)

```bash
node scripts/close_dlmm_positions.cjs
```

Withdraws all liquidity from and closes every position the admin wallet owns across all 6 pools.

### Close a single position by address

```bash
node scripts/close_single_position.cjs <POSITION_ADDRESS>
```

### Re-seed liquidity after closing

```bash
node scripts/seed_dlmm_liquidity.cjs
```

### Remove and re-register a reserve (if mint address needs fixing)

```bash
npx ts-node tests/remove_reserve.ts <symbol>
npx ts-node tests/add_reserve.ts    <symbol> <mint_address> <decimals>
npx ts-node tests/update_reserve_lbpair.ts <symbol>
```

> `remove_reserve` has a known `partition_point` bug (`pricing.rs` ~line 370) when the `StateMap` has certain entry counts. If it fails, flag to Carlos.

### Check protocol state

```bash
npx ts-node tests/read_protocol_state.ts   # StateMap (reserves, prices, circulation)
npx ts-node tests/read_core_positions.ts   # Core positions / LbPair linkages
```

---

## Keypair Files

All keypairs are in `irma/keypairs/` (gitignored — never committed).

| File | Address | Description |
|------|---------|-------------|
| `~/.config/solana/phantom1.json` | `Bp45s9xUrXhR62256ThJLgHVMna5AAYLzVW1xzdbtK8q` | Admin/payer wallet |
| `irma-mint.json` | `EwotD7KQ8TgdvrKWaFVLPRgQnt98Ltjq98UggzFcKbDY` | IRMA Token-2022 mint |
| `devusdt-mint.json` | `4YKp5kpZCL7NLsrwu5vieTg2zhwdP5WZr8aLPdFTgawW` | devUSDT mint |
| `devpyusd-mint.json` | `6o3jQd8APphmBdRS8umCaMnivZxV9HbdymBVmrwMAT6d` | devPYUSD mint |
| `devusds-mint.json` | `CHD776oFbAhqDQ2iMwiZdScka5dy5FdLkouttsvFmoJD` | devUSDS mint |
| `devusdg-mint.json` | `9oqY7wkh19PrJ7fCtthKqRqa41A4uXHv67Pc3WBZaGSx` | devUSDG mint |
| `devfdusd-mint.json` | `4rDY847iWoNsbrHm1FNGzEyNBb6zJZT9gJMQL6FKQXfo` | devFDUSD mint |

Seed phrases for all keypairs are in `docs/phase1-token-setup.md`. Keep that document private.
