#!/bin/bash
# Deploy IRMA Cloudflare Worker — all 6 pool environments
# Usage:
#   ./deploy.sh            — deploy all 6 environments
#   ./deploy.sh usdt       — deploy a single environment
#
# Secrets must be set before first deploy (per environment):
#   npx wrangler secret put ADMIN_PRIVATE_KEY --env <env>
#   npx wrangler secret put HELIUS_API_KEY --env <env>

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
VERCEL_URL_FILE="$SCRIPT_DIR/truflation-proxy/.vercel-url"
WRANGLER_CONFIG="$SCRIPT_DIR/wrangler.toml"
ENVS=(usdc usdt pyusd usds usdg fdusd)

# Optionally update TRUFLATION_PROXY_URL in wrangler.toml from the Vercel deploy
if [ -f "$VERCEL_URL_FILE" ]; then
  TRUFLATION_URL=$(cat "$VERCEL_URL_FILE")
  echo "Using Truflation proxy URL: $TRUFLATION_URL"
  if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s|TRUFLATION_PROXY_URL = \"[^\"]*\"|TRUFLATION_PROXY_URL = \"$TRUFLATION_URL\"|g" "$WRANGLER_CONFIG"
  else
    sed -i "s|TRUFLATION_PROXY_URL = \"[^\"]*\"|TRUFLATION_PROXY_URL = \"$TRUFLATION_URL\"|g" "$WRANGLER_CONFIG"
  fi
else
  echo "⚠️  No .vercel-url found — using default TRUFLATION_PROXY_URL from wrangler.toml"
  echo "   To update, run: cd truflation-proxy && ./deploy.sh"
fi

# Deploy one or all environments
if [ -n "$1" ]; then
  echo "Deploying environment: $1"
  npx wrangler deploy --env "$1"
  echo "✅ Deployed irma-client-$1"
else
  echo "Deploying all ${#ENVS[@]} environments..."
  for env in "${ENVS[@]}"; do
    echo ""
    echo "--- $env ---"
    npx wrangler deploy --env "$env"
    echo "✅ Deployed irma-client-$env"
  done
  echo ""
  echo "✅ All environments deployed."
fi
