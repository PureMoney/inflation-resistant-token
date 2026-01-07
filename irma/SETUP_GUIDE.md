# IRMA Protocol Setup Guide for Developers

## Prerequisites

Before you begin, ensure you have the following installed:

### Required Software
- **Node.js** 18+ and npm/yarn
- **Rust** 1.70+ (install via [rustup](https://rustup.rs/))
- **Solana CLI** 1.18+ (install via [solana.com](https://docs.solana.com/cli/install-solana-cli-tools))
- **Anchor** 0.32.1+ (install via `cargo install --locked anchor-cli`)

### Verify Installation
```bash
node --version      # Should be 18+
rustc --version     # Should be 1.70+
solana --version    # Should be 1.18+
anchor --version    # Should be 0.32.1+
```

---

## Initial Setup

### 1. Clone Repository
```bash
git clone https://github.com/PureMoney/inflation-resistant-stablecoin.git
cd inflation-resistant-stablecoin/irma
```

### 2. Install Dependencies
```bash
# Install Node dependencies
npm install
# or
yarn install
```

### 3. Configure Solana CLI
```bash
# Set devnet as default cluster
solana config set --url https://api.devnet.solana.com

# Verify configuration
solana config get
```

### 4. Create or Import Wallet
```bash
# Option A: Create new wallet (for testing only)
solana-keygen new --outfile ~/.config/solana/phantom1.json

# Option B: Import existing wallet
# Place your wallet keypair at ~/.config/solana/phantom1.json

# Verify wallet
solana address --keypair ~/.config/solana/phantom1.json
```

### 5. Request Devnet SOL (if needed)
```bash
# Request 2 SOL for testing
solana airdrop 2 ~/.config/solana/phantom1.json --url devnet

# Check balance
solana balance ~/.config/solana/phantom1.json --url devnet
```

---

## Building the Program

### Build the Anchor Program
```bash
# From the irma/ directory
anchor build
```

Expected output:
```
Compiling irma v0.1.0
Finished `release` profile...
```

### Verify Build Output
```bash
# Check that these files exist:
ls -la target/deploy/irma.so
ls -la target/idl/irma.json
```

---

## Deployment

### Current Deployment Status
The program is already deployed on Solana devnet:
- **Program ID**: `FReBisHtV3Lh1eXSxmg52vuBXetUypD36YYMft7WBvvC`
- **Deployment Slot**: 415961473
- **Explorer Link**: https://explorer.solana.com/address/FReBisHtV3Lh1eXSxmg52vuBXetUypD36YYMft7WBvvC?cluster=devnet

### To Update Deployment (if code changes)
```bash
# Ensure you have enough SOL
solana balance ~/.config/solana/phantom1.json --url devnet

# Deploy the program
anchor deploy --provider.cluster devnet
# or manually:
solana program deploy target/deploy/irma.so \
  --program-id target/deploy/irma-keypair.json \
  --url devnet \
  --keypair ~/.config/solana/phantom1.json
```

---

## Running Tests

### Test Infrastructure
All tests are in `tests/` directory and use Anchor's TypeScript testing framework.

### Available Tests

#### 1. Read Protocol State (No Wallet Required) ✅
```bash
npx ts-node tests/read_protocol_state.ts
```

**Perfect for your client! No wallet needed.**

What it does:
- Connects to devnet anonymously
- Reads protocol state account
- Displays all prices and account info
- Shows time since last update
- **Your client should run this first to verify setup**

Expected output:
```
📖 Reading IRMA Protocol State (Read-Only)

✅ Protocol State Data:
  Mint Price: $1.050004 USDC
  Redemption Price: $1.000003 USDC
  ...
```

#### 2. Test Inflation Mechanism (Requires Your Wallet) 🔒
```bash
npx ts-node tests/test_inflation.ts
```

What it does:
- Applies 5% inflation twice
- Sends 2 transactions to devnet
- Verifies prices compound correctly over time
- **Requires SOL and permission (your wallet only)**

#### 3. Test Mint Price (Requires Your Wallet) 🔒
```bash
npx ts-node tests/test_mint_price.ts
```

What it does:
- Reads current protocol state
- Applies inflation
- Verifies prices updated correctly
- **Requires SOL and permission (your wallet only)**

---

## Configuration Files

### devnet-config.json
Contains all devnet addresses and pool parameters:
```json
{
  "program": {
    "programId": "FReBisHtV3Lh1eXSxmg52vuBXetUypD36YYMft7WBvvC"
  },
  "tokens": {
    "irma": { "mint": "ADqpCiuXTnhDsXVaeZMbTpuriotmjGZUh4sptzzzmFmm" },
    "usdc": { "mint": "BRjpCHtyQLNCo8gqRUr8jtdAj5AjPYQaoqbvcZiHok1k" }
  },
  "meteora_dlmm": {

  }
}
```

**Do not modify** - all addresses are validated on devnet.

---

## Key Directories

```
irma/
├── programs/irma/src/
│   ├── lib.rs                 # Main program entry point
│   ├── protocol_state.rs      # Protocol state & inflation logic
│   ├── crank_market.rs        # Market operations
│   ├── pricing.rs             # Pricing calculations
│   └── ...                    # Other modules
│
├── tests/
│   ├── test_inflation.ts      # Inflation mechanism tests
│   ├── test_mint_price.ts     # Mint price update tests
│   └── ...
│
├── scripts/
│   ├── create_orca_position.ts      # Setup real Orca position
│   └── initialize_protocol_with_position.ts  # Initialize protocol state
│
├── target/
│   ├── deploy/irma.so         # Compiled program (generated)
│   └── idl/irma.json          # Program IDL (generated)
│
├── devnet-config.json         # Devnet addresses
├── Anchor.toml                # Anchor configuration
└── Cargo.toml                 # Rust dependencies
```

---

## Common Tasks

### Build Only (without deploying)
```bash
anchor build
```

### Run Type Checking
```bash
# Check TypeScript in tests
npx tsc --noEmit
```

### Check Program Size
```bash
# Verify program won't exceed Solana size limits
ls -lh target/deploy/irma.so
```

### View Program IDL
```bash
cat target/idl/irma.json
```

### Monitor Devnet Faucet Rate Limiting
If you get "Account X has insufficient funds":
1. Wait 24 hours and try again
2. Or request via: https://faucet.solana.com/

---

## Troubleshooting

### Problem: "Error: expected environment variable `ANCHOR_WALLET` is not set"
**Solution**: Make sure your wallet is at `~/.config/solana/phantom1.json`
```bash
# Verify
ls -la ~/.config/solana/phantom1.json
```

### Problem: "Buffer account data size (XXX) is smaller than the minimum size (YYY)"
**Solution**: This means the program data is corrupted. Redeploy:
```bash
solana program close FReBisHtV3Lh1eXSxmg52vuBXetUypD36YYMft7WBvvC --url devnet
anchor deploy --provider.cluster devnet
```

### Problem: "Custom program error: 0xc"
**Solution**: This is usually an account mismatch. Verify:
1. Program ID matches `Anchor.toml`
2. Protocol state PDA is derived correctly
3. IDL is up to date: `cat target/idl/irma.json | grep address`

### Problem: Test fails with "Account does not exist"
**Solution**: Protocol state hasn't been initialized. Run:
```bash
npx ts-node scripts/initialize_protocol_with_position.ts
```

### Problem: "Panic in ...: assertion failed"
**Solution**: Check the program logs:
```bash
# Set log level
RUST_LOG=debug npx ts-node tests/test_inflation.ts
```

---

## Documentation

### For Implementation Details
See `IMPLEMENTATION_GUIDE_NEXT_STEPS.md` for:
- Phase 1: Inflation mechanism (90% complete)
- Phase 2: Liquidity rebalancing (planned)
- Phase 3: Crank mechanism (planned)
- Phase 4: User operations (planned)

### For Milestone Status
See `MILESTONE_COMPLETION.md` for:
- Detailed completion percentages
- Effort estimates for each phase
- Technical architecture overview

### For Current Session Details
See `SESSION_SUMMARY.md` for:
- What was accomplished
- Test results
- Next immediate steps

---

## Quick Start Command Reference

```bash
# Setup (one time)
npm install
anchor build
npx ts-node scripts/create_orca_position.ts     # (only if needed)
npx ts-node scripts/initialize_protocol_with_position.ts

# Development (daily)
anchor build                 # Build program
npx ts-node tests/test_inflation.ts    # Test inflation
npx ts-node tests/test_mint_price.ts   # Test prices

# Deployment (when ready)
anchor deploy --provider.cluster devnet

# Verification
solana program show FReBisHtV3Lh1eXSxmg52vuBXetUypD36YYMft7WBvvC --url devnet
solana account 9MTnwn1AMBG9oDjMrA5qS5LJqehHG5eHVrynrZakmmJ8 --url devnet
```

---

## Git Workflow

### For Contributors
```bash
# Create feature branch
git checkout -b feature/your-feature

# Make changes
# ...

# Build and test locally
anchor build
npx ts-node tests/test_inflation.ts

# Commit with descriptive message
git add .
git commit -m "feat: add liquidity rebalancing"

# Push to GitHub
git push origin feature/your-feature

# Create Pull Request
```

### Commit Message Format
```
feat: add new feature
fix: fix a bug
docs: update documentation
test: add or update tests
chore: maintenance tasks
```

---

## Support & Questions

If your client encounters issues:

1. **Check Prerequisites**: Ensure all software versions match
2. **Review Logs**: Enable debug logging with `RUST_LOG=debug`
3. **Verify Configuration**: Check that wallet and config files exist
4. **Run Tests**: Execute test suite to verify setup
5. **Check Documentation**: See relevant `.md` files in root directory

For Solana-specific issues, see:
- Solana Docs: https://docs.solana.com
- Anchor Book: https://book.anchor-lang.com
- Meteora Docs: https://

---

**Last Updated**: October 21, 2025
**Tested On**: Solana Devnet, Anchor 0.32.1
**Status**: Ready for production use
