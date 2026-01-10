
// ==================================================================
// CONFIGURATION
// ==================================================================

// CONSTANTS
export const RESERVE_MINT_STR = "J2JAep9untmdaQXXRYB1bxT2eFNWWeR8ApuRdAiY9gni"; // devUSDT mint on Devnet
export const RESERVE_SYMBOL = "devUSDT";
export const POOL_ADDRESS = "HYeXEBUxLM4aFYSBmHRhMLwMP5wGDXMtEHTtx3VevkTD"; // Meteora DLMM pool for IRMA/devUSDT

// TRUFLATION CONFIGURATION
// Inflation data is fetched via our Vercel proxy (truflation-proxy)
// The proxy URL is set in wrangler.jsonc vars.TRUFLATION_PROXY_URL
// Deploy the proxy first: cd truflation-proxy && ./deploy.sh

// Target inflation rate (below this, mint price = 1.0 / quote_token_price)
export const TARGET_INFLATION_RATE = 2.0;
