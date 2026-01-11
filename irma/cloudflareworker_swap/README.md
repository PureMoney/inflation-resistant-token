# IRMA Cloudflare Worker - Automated Market Operations

This Cloudflare Worker monitors the IRMA/devUSDC Meteora DLMM pool and automatically performs counter-swaps and liquidity rebalancing operations in response to user trades.

### Overview

The worker:
1. Receives webhooks from Helius when swaps occur on the Meteora pool
2. Detects MINT (buy IRMA) or REDEMPTION (sell IRMA) events
3. Performs counter-swaps to maintain market balance
4. Adds liquidity to appropriate bins (mint or redemption price bins)
5. Automatically rebalances bins when prices change
6. Records trade events in the IRMA on-chain program
7. Stores all logs permanently in Cloudflare D1 database

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

npm install -g vercel

npm install
```

### 2. Configure Wrangler

Login to Cloudflare (first time only):

```bash
npx wrangler login
```

### 3. Set Up D1 Database (Persistent Logging)

The worker uses Cloudflare D1 to store logs permanently. This is required for the worker to function properly.

**Step 1: Create the D1 database:**

```bash
npx wrangler d1 create irma-logs

Would you like Wrangler to add it on your behalf? … yes
What binding name would you like to use? … DB
For local dev, do you want to connect to the remote resource instead of a local resource? … yes
```

This will output something like:
```
✅ Successfully created DB 'irma-logs' in region WNAM
Created your new D1 database.

[[d1_databases]]
binding = "DB"
database_name = "irma-logs"
database_id = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
```

Ensure that the `wrangler.jsonc` only has one d1_databases entry with the above details.

**Step 2: Initialize the database schema:**

```bash
npx wrangler d1 execute irma-logs --remote --file=./schema.sql
```

This creates the following tables:
- `console_logs` - All console log outputs
- `swap_events` - Structured swap event data
- `price_updates` - Price update history
- `rebalancing_events` - Bin rebalancing operations
- `active_bins` - Current active mint/redemption bins
- `positions` - Tracked liquidity positions

**Step 3: Verify the schema was applied:**

```bash
npx wrangler d1 execute irma-logs --remote --command="SELECT name FROM sqlite_master WHERE type='table'"
```

### 4. Set Environment Variables

Push your admin account's private key in array format to Cloudflare:

```bash
# Your Solana admin keypair as JSON array
$ npx wrangler secret put ADMIN_PRIVATE_KEY
$ Enter a secret value: > # Paste: [123,45,67,...] (your secret key array)
```

Retrieve your [Helius API Key](https://dashboard.helius.dev/api-keys) and push it to wrangler:.

```bash
$ npx wrangler secret put HELIUS_API_KEY
$ Enter a secret value: > # Paste: fc123456-789a...
```

### 5. Update Configuration

Edit `wrangler.jsonc` if needed:
- `name`: Worker name (default: "irma-client"). Change this to the *exact* name of the cloudflare worker being used with your account. If you have not created a separate cloudflare worker, one will be created automatically with your account when this program is deployed. The URL will also change accordingly.

Edit `src/worker.js` constants:
- `POOL_ADDRESS`: This is the pool address of the Meteora DLMM Pool, which offers trading functionality for both IRMA and the reserve stablecoin being offered. The current setup is for devnet, and for `devUSDC` as the reserve stablecoin.
- `RESERVE_MINT_STR`: Mint address of the reserve stablecoin.
- `RESERVE_SYMBOL`: Symbol of the reserve stablecoin.

Note that `MEMO_PROGRAM_ID` remains the same on both devnet and mainnet-beta for Solana and does not need to be changed.

### 6. Deploy

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

## Truflation Integration

The worker integrates with [TRUF.NETWORK](https://truf.network) to fetch real-time US inflation data and automatically update the IRMA mint price.

### Architecture

Due to Cloudflare Workers' limitations with axios (used by the Truflation SDK), we use a **Vercel serverless proxy**:

```
┌─────────────────────┐      ┌─────────────────────┐      ┌─────────────────────┐
│  Cloudflare Worker  │ ───► │   Vercel Proxy      │ ───► │   TRUF.NETWORK      │
│  (fetch only)       │      │   (Node.js/axios)   │      │   (Truflation SDK)  │
└─────────────────────┘      └─────────────────────┘      └─────────────────────┘
```

The proxy is located in `truflation-proxy/` and handles the SDK complexity.

### Data Source: Truflation US Inflation Index

| Property | Value |
|----------|-------|
| **Stream Name** | Truflation US Inflation Index |
| **Stream ID** | `st1e321de22ece39a258bc2588dd2871` |
| **Data Provider** | `0x4710a8d8f0d845da110086812a32de6d90d7ff5c` |
| **Network** | TRUF.NETWORK Mainnet |

> Explore streams at [explorer.truf.network](https://explorer.truf.network)

### Deployment

Deploy in this order:

**1. Sign in to Vercel (first time only):**
```bash
npx vercel login
```

Follow the prompts to create a free Vercel account or sign in with an existing one. You'll visit a URL and enter a code to authenticate.

**2. Deploy the Truflation Proxy (Vercel):**
```bash
cd truflation-proxy
npm install
chmod +x deploy.sh
./deploy.sh
```

This saves the Vercel URL to `.vercel-url` (gitignored).

**3. Deploy the Cloudflare Worker:**
```bash
cd ..
chmod +x deploy.sh
./deploy.sh
```

This reads the URL from `.vercel-url` and updates `wrangler.jsonc` before deploying.

### How Mint Price is Calculated

The mint price adjusts based on inflation above the 2% target:

```javascript
if (inflationRate > 2.0) {
  mintPrice = (1.00 + (inflationRate - 2.0) / 100.0) / quoteTokenPriceUSD;
} else {
  mintPrice = 1.00 / quoteTokenPriceUSD;
}
```

**Example**: If inflation is 2.169% and USDC is $1.00:
- Adjustment = (2.169 - 2.0) / 100 = 0.00169
- Mint price = (1.00 + 0.00169) / 1.00 = **1.00169**

### Automatic Daily Updates (Cron)

The worker runs automatically once per day at **6:00 AM UTC** via Cloudflare's scheduled triggers:

```json
"triggers": {
  "crons": ["0 6 * * *"]
}
```

**What happens during the scheduled run:**
1. Fetches the latest US inflation rate from the Truflation proxy
2. Calculates the new mint price based on the inflation-adjustment formula
3. Updates the IRMA program's mint price on-chain via the `set_mint_price` instruction
4. Logs the update for monitoring

This ensures the IRMA stablecoin's mint price automatically tracks real-world inflation data.

### Manual Endpoints API

| Endpoint | Description |
|----------|-------------|
| `GET /?action=health` | Health check and list available endpoints |
| `GET /?action=fetch-inflation` | Test: Fetch current inflation rate from Truflation |
| `GET /?action=update-mint-price` | Update mint price using current Truflation data |
| `GET /?action=rebalance-bins` | Manual trigger to rebalance mint/redemption bins |
| `GET /?action=view-bins` | View current active mint/redemption bin IDs |
| `GET /?action=view-logs&type=TYPE&limit=N&offset=N` | Query stored logs |

**Log Types for `view-logs`:**
- `console` - All console log outputs
- `swaps` - Swap event history
- `prices` - Price update history
- `rebalancing` - Bin rebalancing events
- `bins` - Current active bins

**Example**:
```bash
# Test fetching inflation data
curl "https://your-worker.workers.dev/?action=fetch-inflation"
# Response: {"success":true,"inflationRate":2.169056,"message":"Current inflation rate: 2.169056%"}

# Update mint price
curl "https://your-worker.workers.dev/?action=update-mint-price"

# Manually rebalance bins
curl "https://your-worker.workers.dev/?action=rebalance-bins"

# View current active bins
curl "https://your-worker.workers.dev/?action=view-bins"

# View last 50 swap events
curl "https://your-worker.workers.dev/?action=view-logs&type=swaps&limit=50"

# View console logs with pagination
curl "https://your-worker.workers.dev/?action=view-logs&type=console&limit=100&offset=100"
```

### Configuration

The Truflation proxy URL is set in `wrangler.jsonc`:

```jsonc
"vars": {
  "TRUFLATION_PROXY_URL": "https://truflation-proxy.vercel.app"
}
```

This is automatically updated by `./deploy.sh` when you deploy.

## Development

Alter the script and run 
```bash
npx wrangler deploy
```
to update it. Run
```bash
npx wrangler tail
```
to observe the output in real-time.

### Viewing Stored Logs

All logs are now permanently stored in the D1 database. You can query them via:

**Using the API:**
```bash
curl "https://your-worker.workers.dev/?action=view-logs&type=console&limit=100"
```

**Using Wrangler CLI:**
```bash
# View recent console logs
npx wrangler d1 execute irma-logs --command="SELECT * FROM console_logs ORDER BY timestamp DESC LIMIT 20"

# View swap events
npx wrangler d1 execute irma-logs --command="SELECT * FROM swap_events ORDER BY timestamp DESC LIMIT 20"

# View price updates
npx wrangler d1 execute irma-logs --command="SELECT * FROM price_updates ORDER BY timestamp DESC LIMIT 20"

# View rebalancing events
npx wrangler d1 execute irma-logs --command="SELECT * FROM rebalancing_events ORDER BY timestamp DESC LIMIT 20"

# View current active bins
npx wrangler d1 execute irma-logs --command="SELECT * FROM active_bins"
```

### Database Maintenance

To clear old logs (if needed):
```bash
# Delete logs older than 30 days
npx wrangler d1 execute irma-logs --command="DELETE FROM console_logs WHERE timestamp < (strftime('%s', 'now') - 2592000) * 1000"
```

## Bin Rebalancing

The worker automatically rebalances liquidity when mint or redemption bins change due to price updates.

### When Rebalancing Occurs

1. **After Daily Price Update**: When the scheduled Truflation price update runs at 6:00 AM UTC, the worker checks if the new prices result in different bin IDs. If bins have changed by 1 or more positions, rebalancing is triggered.

2. **During Swap Processing**: When processing a MINT or REDEMPTION event, if the stored active bins differ from the current calculated bins, the worker will:
   - Remove liquidity from the old bin
   - Add it to the new bin (combined with the counter-swap output)

3. **Manual Trigger**: Use `GET /?action=rebalance-bins` to force a rebalancing check.

### Rebalancing Logic

**Mint Bin Rebalancing** (IRMA tokens):
- Detects if the mint bin ID has changed
- Removes all liquidity from the old mint bin position
- Closes empty positions
- Adds all IRMA to the new mint bin

**Redemption Bin Rebalancing** (USDC tokens):
- Detects if the redemption bin ID has changed
- Removes all liquidity from the old redemption bin position
- Closes empty positions
- Adds all USDC to the new redemption bin

### Monitoring Rebalancing

View rebalancing history:
```bash
curl "https://your-worker.workers.dev/?action=view-logs&type=rebalancing&limit=20"
```

Or via CLI:
```bash
npx wrangler d1 execute irma-logs --command="SELECT * FROM rebalancing_events ORDER BY timestamp DESC LIMIT 10"
```

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