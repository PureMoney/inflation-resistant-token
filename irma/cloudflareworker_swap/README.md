# IRMA Cloudflare Worker - Automated Market Operations

This Cloudflare Worker monitors the IRMA/devUSDC Meteora DLMM pool and automatically performs counter-swaps and liquidity rebalancing operations in response to user trades.

### Overview

The worker:
1. Receives webhooks from Helius when swaps occur on the Meteora pool
2. Detects MINT (buy IRMA) or REDEMPTION (sell IRMA) events
3. Performs counter-swaps to maintain market balance
4. Adds liquidity to appropriate bins (mint or redemption price bins)
5. Records trade events in the IRMA on-chain program

### Prerequisites

- [Node.js](https://nodejs.org/) v18 or higher
- [Cloudflare account](https://dash.cloudflare.com/sign-up)
- [Wrangler CLI](https://developers.cloudflare.com/workers/wrangler/install-and-update/)
- [Helius Account](https://www.helius.dev/)
- Solana wallet with devnet SOL and admin access to IRMA program

# Setup

### 1. Install Dependencies

```bash
npm install -g wrangler

npm install
```

### 2. Configure Wrangler

Login to Cloudflare (first time only):

```bash
npx wrangler login
```

### 3. Set Environment Variables

Push your admin account's private key in array format to Cloudflare:

```bash
# Your Solana admin keypair as JSON array
npx wrangler secret put ADMIN_PRIVATE_KEY
# Paste: [123,45,67,...] (your secret key array)
```

Retrieve your [Helius API Key](https://dashboard.helius.dev/api-keys) and add it to the `.env` file in this directory.

### 4. Update Configuration

Edit `wrangler.jsonc` if needed:
- `name`: Worker name (default: "irma-client"). Change this to the *exact* name of the cloudflare worker being used with your account. If you have not created a separate cloudflare worker, one will be created automatically with your account when this program is deployed. The URL will also change accordingly.

Edit `src/worker.js` constants:
- `POOL_ADDRESS`: This is the pool address of the Meteora DLMM Pool, which offers trading functionality for both IRMA and the reserve stablecoin being offered. The current setup is for devnet, and for `devUSDC` as the reserve stablecoin.
- `RESERVE_MINT_STR`: Mint address of the reserve stablecoin.
- `RESERVE_SYMBOL`: Symbol of the reserve stablecoin.

Note that `MEMO_PROGRAM_ID` remains the same on both devnet and mainnet-beta for Solana and does not need to be changed.

### 5. Deploy

```bash
npx wrangler deploy
```

Your worker will be deployed to: `https://<worker-name>.<your-subdomain>.workers.dev`

## Helius Webhook Setup

### Create Webhook

1. Go to [Helius Dashboard](https://dashboard.helius.dev)
2. Navigate to **Webhooks** → **Create Webhook**
3. Configure:
   - **Type**: Raw Transactions
   - **Webhook URL**: `https://<worker-name>.<your-subdomain>.workers.dev`
   - **Transaction Types**: Select "Swap"
   - **Accounts**: Add the corresponding Meteora `POOL_ADDRESS` used in `src/worker.js
   - **Network**: Devnet/Mainnet-beta

### Test Webhook

Perform a test swap on the Meteora pool and monitor logs:

```bash
npx wrangler tail
```

You should see:
```
🚨 TRIGGER: MINT Detected. Delta: 5
📊 Fetching prices from IRMA program...
✅ Counter-swap sent: <signature>
✅ Liquidity addition sent to redemption bin: <signature>
✅ Sale trade event recorded
```

## Development

Alter the script and run 
```bash
npx wrangler deploy
```
to update it. Run
```bash
npx wrangler tail
```
to observe the output. For further testing, `src/worker.js` may be changed to store all logs of the worker and store them in a remote database for inspection, as Cloudflare's free plan only saves logs for up to 3 days.

## How It Works

### MINT Event (User buys IRMA with USDC)

1. User swaps USDC → IRMA (pool gains USDC)
2. Worker detects positive delta in pool's USDC balance
3. **Counter-swap**: Worker swaps IRMA → USDC (to get USDC back)
4. **Add Liquidity**: Adds USDC to redemption bin (bin at redemption price)
5. **Record**: Calls `sale_trade_event()` in IRMA program

### REDEMPTION Event (User sells IRMA for USDC)

1. User swaps IRMA → USDC (pool loses USDC)
2. Worker detects negative delta in pool's USDC balance
3. **Counter-swap**: Worker swaps USDC → IRMA (to get IRMA back)
4. **Add Liquidity**: Adds IRMA to mint bin (bin at mint price)
5. **Record**: Calls `buy_trade_event()` in IRMA program

### Bin Distance Check

If mint and redemption prices are within 1 bin of each other, no counter-swap is performed (only trade event is recorded).

### Loop Prevention

The worker adds a memo (`IRMA_WORKER_SWAP`) to its own swaps. When processing webhooks, it checks for this memo and ignores its own transactions to prevent infinite loops.

### Fee Handling

Counter-swaps use 95% of the user's swap amount to account for fees and ensure sufficient liquidity is available.

## Troubleshooting

### "No positions found" Error

Ensure your admin wallet has at least one position in the Meteora pool. The worker will auto-create positions for specific bins as needed.

### Timeout Errors

Cloudflare Workers have execution time limits. The worker uses `ctx.waitUntil()` for background processing and sends transactions without waiting for confirmation to stay within limits.

### Webhook Not Triggering

- Verify webhook is configured in Helius dashboard
- Check that the correct pool address is monitored
- Use `npx wrangler tail` to view real-time logs