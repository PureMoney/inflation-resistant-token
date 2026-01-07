# Truflation Proxy

A simple Vercel serverless function that fetches inflation data from Truflation's TRUF.NETWORK.

## Why?

The Truflation SDK uses axios, which doesn't work in Cloudflare Workers. This proxy runs on Vercel (Node.js environment) where axios works fine.

## Endpoint

```
GET /api/inflation
```

### Response

```json
{
  "success": true,
  "data": {
    "inflationRate": 2.169056,
    "eventTime": "1766880000",
    "timestamp": 1735443600000,
    "source": "Truflation US Inflation Index",
    "streamId": "st1e321de22ece39a258bc2588dd2871"
  }
}
```

## Deployment

1. Install dependencies:
   ```bash
   cd truflation-proxy
   npm install
   ```

2. Deploy to Vercel (saves URL to `.vercel-url`):
   ```bash
   chmod +x deploy.sh
   ./deploy.sh
   ```

3. Deploy the Cloudflare Worker (reads URL from `.vercel-url`):
   ```bash
   cd ..
   chmod +x deploy.sh
   ./deploy.sh
   ```

## Local Development

```bash
npm run dev
```

Then test at: http://localhost:3000/api/inflation
