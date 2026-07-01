
// All pool/token configuration is read from Cloudflare Worker environment variables.
// Set these in wrangler.toml [vars] or as Cloudflare Worker secrets:
//
//   POOL_ADDRESS          — Meteora DLMM LbPair address for this environment
//   RESERVE_SYMBOL        — Human-readable symbol, e.g. "devUSDT"
//   RESERVE_MINT_STR      — Reserve (stablecoin) token mint address
//   IRMA_MINT_STR         — IRMA token mint address
//   TARGET_INFLATION_RATE — Inflation threshold above which mint price is adjusted (default "2.0")
//   ENABLE_TEST_SCAFFOLDING — Set "true" to expose GET test endpoints (default "false")
//   ADMIN_PRIVATE_KEY     — JSON array of the admin keypair bytes (Cloudflare secret)
//   HELIUS_API_KEY        — Helius RPC API key (Cloudflare secret)
//   TRUFLATION_PROXY_URL  — URL of the deployed Truflation Vercel proxy
