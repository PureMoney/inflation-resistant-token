#!/bin/bash
# Deploy truflation-proxy to Vercel and save the URL

echo "Deploying truflation-proxy to Vercel..."

# Run vercel deploy and capture output
OUTPUT=$(npx vercel --prod 2>&1)
echo "$OUTPUT"

# Extract the aliased/production URL (contains "Aliased:" in the output)
# This is the stable public URL, not the unique deployment URL
URL=$(echo "$OUTPUT" | grep "Aliased:" | grep -o 'https://[a-zA-Z0-9-]*\.vercel\.app' | head -1)

if [ -z "$URL" ]; then
  echo "⚠️  Could not find aliased URL, trying to extract any production URL..."
  # Fallback: look for any vercel.app URL that doesn't contain a hash
  URL=$(echo "$OUTPUT" | grep -o 'https://[a-zA-Z0-9-]*\.vercel\.app' | grep -v '\-[a-z0-9]\{7,\}\-' | head -1)
fi

if [ -z "$URL" ]; then
  echo "❌ Failed to extract Vercel URL from deployment output"
  exit 1
fi

# Save to config file
echo "$URL" > .vercel-url
echo ""
echo "Saved Vercel URL to .vercel-url: $URL"
echo ""
echo "Now deploy the Cloudflare Worker:"
echo "   cd .. && ./deploy.sh"
