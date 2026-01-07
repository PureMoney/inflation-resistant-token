#!/bin/bash
# Deploy IRMA Cloudflare Worker
# Reads Truflation proxy URL from truflation-proxy/.vercel-url if it exists

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
VERCEL_URL_FILE="$SCRIPT_DIR/truflation-proxy/.vercel-url"
WRANGLER_CONFIG="$SCRIPT_DIR/wrangler.jsonc"

# Check if truflation-proxy has been deployed
if [ -f "$VERCEL_URL_FILE" ]; then
  TRUFLATION_URL=$(cat "$VERCEL_URL_FILE")
  echo "Using Truflation proxy URL: $TRUFLATION_URL"
  
  # Update wrangler.jsonc with the URL using sed
  # This replaces the TRUFLATION_PROXY_URL value in the vars section
  if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS sed requires empty string for -i
    sed -i '' "s|\"TRUFLATION_PROXY_URL\": \"[^\"]*\"|\"TRUFLATION_PROXY_URL\": \"$TRUFLATION_URL\"|" "$WRANGLER_CONFIG"
  else
    # Linux sed
    sed -i "s|\"TRUFLATION_PROXY_URL\": \"[^\"]*\"|\"TRUFLATION_PROXY_URL\": \"$TRUFLATION_URL\"|" "$WRANGLER_CONFIG"
  fi
else
  echo "⚠️  No .vercel-url found. Using default URL from wrangler.jsonc"
  echo "To update, run: cd truflation-proxy && ./deploy.sh"
fi

echo "Deploying Cloudflare Worker..."
npx wrangler deploy
echo "✅ Deployment complete!"
