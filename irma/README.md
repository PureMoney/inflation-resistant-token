# IRMA: Inflation-Resistant Stablecoin on Solana

An innovative stablecoin protocol built on Solana that maintains value during inflationary periods through automated liquidity management and dynamic pricing.

## Quick Links

- 🚀 **[Setup Guide](./SETUP_GUIDE.md)** - Get started in 10 minutes
- 📊 **[Milestone Completion](./MILESTONE_COMPLETION.md)** - Track progress (35% complete)
- 📋 **[Session Summary](./SESSION_SUMMARY.md)** - Latest updates and test results
- 🔧 **[Implementation Guide](./IMPLEMENTATION_GUIDE_NEXT_STEPS.md)** - Detailed technical roadmap

## Overview

IRMA is a Solana-based stablecoin that uses:
- **Time-based inflation adjustment** to maintain purchasing power
- **Dynamic redemption pricing** based on protocol reserves
- **On-chain governance** through protocol PDAs

### Current Status: Phase 1 ✅ (90% Complete)
- ✅ Core inflation mechanism implemented and tested
- ✅ Protocol state initialized on devnet
- ✅ Real DLMM position created and linked
- ✅ Time-based compounding formula verified
- ⏳ Awaiting redeploy with corrected pricing initialization

### Deployment Info
- **Network**: Solana Devnet
- **Program ID**: `FReBisHtV3Lh1eXSxmg52vuBXetUypD36YYMft7WBvvC`
- **Protocol State**: `9MTnwn1AMBG9oDjMrA5qS5LJqehHG5eHVrynrZakmmJ8`
- **Explorer**: https://explorer.solana.com/address/FReBisHtV3Lh1eXSxmg52vuBXetUypD36YYMft7WBvvC?cluster=devnet

---

## Getting Started

### Prerequisites
- Node.js 18+
- Rust 1.70+
- Solana CLI 1.18+
- Anchor 0.32.1+

### 5-Minute Setup
```bash
# Clone and install
git clone https://github.com/PureMoney/inflation-resistant-stablecoin.git
cd inflation-resistant-stablecoin/irma
npm install

# Build
anchor build

# Test
npx ts-node tests/test_inflation.ts
```

For detailed setup, see [SETUP_GUIDE.md](./SETUP_GUIDE.md).

---

## Project Structure

```
irma/
├── programs/irma/src/
│   ├── lib.rs                    # Main program entry point
│   ├── protocol_state.rs         # Protocol state & inflation logic ⭐
│   ├── pricing.rs                # Pricing formulas
│   ├── crank_market.rs           # Market operations
│   └── ...
│
├── tests/                        # Integration tests
│   ├── test_inflation.ts         # Inflation mechanism ✅
│   └── test_mint_price.ts        # Price updates ✅
│
├── scripts/
│   ├── create_DLMM_position.ts   # Setup DLMM position
│   └── initialize_protocol_with_position.ts
│
└── [Documentation files]
    ├── SETUP_GUIDE.md            # Setup instructions
    ├── IMPLEMENTATION_GUIDE_NEXT_STEPS.md
    ├── MILESTONE_COMPLETION.md   # Progress tracking
    └── SESSION_SUMMARY.md        # Latest updates
```

---

## Key Features

### 1. Time-Based Inflation Adjustment
Mint prices increase via compounding formula:
$$\text{price}_{new} = \text{price}_{current} \times (1 + r)^{t/\text{year}}$$

Where:
- `r` = annual inflation rate (e.g., 5%)
- `t` = time elapsed in seconds
- Tested and verified on devnet ✅

### 2. Dynamic Redemption Pricing
Redemption price automatically updates based on:
$$\text{redemption\_price} = \frac{\text{backing\_reserves}}{\text{irma\_in\_circulation}}$$

- Never falls below 1.0 USD
- Increases as stablecoins are minted/swapped
- Ensures protocol solvency

### 3. DLMM Whirlpool Integration
- **Pool**: 5A9fbjFRDFqaeb1PYns2PMi11jCV1nkfwrHVm4W3T7pQ
- **Position**: 2RB4Bi3awKj7tkucGfB75KH1hxXANNwYRft6vGZ8pPTV
- **Fee Rate**: 40 bps (0.4%)
- **Liquidity**: 7.8B+ tokens

---

## Development Workflow

### Build the Program
```bash
anchor build
```

### Run Tests
```bash
# Test inflation mechanism
npx ts-node tests/test_inflation.ts

# Test price updates
npx ts-node tests/test_mint_price.ts
```

### Deploy to Devnet
```bash
# Rebuild
anchor build

# Deploy (requires ~3.2 SOL)
anchor deploy --provider.cluster devnet

# Reinitialize
npx ts-node scripts/initialize_protocol_with_position.ts
```

---

## Testing

### Test Coverage
- ✅ **Inflation Mechanism**: Applied 5% inflation, verified compounding
- ✅ **Price Updates**: Confirmed mint price increases, redemption calculated correctly
- ✅ **Protocol State**: Verified on-chain account initialization

### Running Tests
```bash
# Standard test
npx ts-node tests/test_inflation.ts

# With debug logging
RUST_LOG=debug npx ts-node tests/test_inflation.ts

# All tests
for test in tests/test_*.ts; do npx ts-node "$test"; done
```

---

## Roadmap

### Phase 1: Inflation & Pricing ✅ (90%)
- [x] Inflation calculation with time-based compounding
- [x] Dynamic redemption pricing formula
- [x] Protocol state initialization
- [x] Test infrastructure
- ⏳ Redeploy with corrected initialization

### Phase 2: Liquidity Rebalancing ⏳ (0%)
- [ ] Rebalancer module (4-5 hours)
- [ ] Threshold detection
- [ ] DLMM CPI integration
- [ ] Automated position updates

### Phase 3: Crank Mechanism ⏳ (0%)
- [ ] Crank orchestration (3-4 hours)
- [ ] Event-driven rebalancing
- [ ] State synchronization

### Phase 4: User Operations ⏳ (0%)
- [ ] Mint instruction (user issues IRMA for USDC)
- [ ] Redeem instruction (user gets USDC for IRMA)
- [ ] LP operations (deposit/withdraw)

**Overall Progress**: 35% complete (11 of 32 milestones)
**Estimated Completion**: 12-15 hours of development

For detailed breakdown, see [MILESTONE_COMPLETION.md](./MILESTONE_COMPLETION.md).

---

## Configuration

### Devnet Addresses (devnet-config.json)
```json
{
  "program": {
    "programId": "FReBisHtV3Lh1eXSxmg52vuBXetUypD36YYMft7WBvvC"
  },
  "tokens": {
    "irma": { "mint": "ADqpCiuXTnhDsXVaeZMbTpuriotmjGZUh4sptzzzmFmm" },
    "usdc": { "mint": "BRjpCHtyQLNCo8gqRUr8jtdAj5AjPYQaoqbvcZiHok1k" }
  },
  "DLMM": {
    "whirlpool": "5A9fbjFRDFqaeb1PYns2PMi11jCV1nkfwrHVm4W3T7pQ",
    "position": "2RB4Bi3awKj7tkucGfB75KH1hxXANNwYRft6vGZ8pPTV"
  }
}
```

**Note**: Do not modify these addresses - they are validated on devnet.

---

## Troubleshooting

### "Account does not exist"
Ensure protocol state is initialized:
```bash
npx ts-node scripts/initialize_protocol_with_position.ts
```

### "Buffer account data size error"
Program data is corrupted. Redeploy:
```bash
anchor deploy --provider.cluster devnet
```

### Insufficient SOL
Request devnet airdrop:
```bash
solana airdrop 2 ~/.config/solana/phantom1.json --url devnet
```

For more help, see [SETUP_GUIDE.md](./SETUP_GUIDE.md#troubleshooting).

---

## Architecture

```
┌─────────────────────────────────────────┐
│         User Interface (Future)         │
│  (Mint / Redeem / LP Operations)        │
└──────────────────┬──────────────────────┘
                   │
┌──────────────────┴──────────────────────┐
│        IRMA Protocol Program            │
├─────────────────────────────────────────┤
│ • Inflation Adjustment ✅               │
│ • Price Calculations ✅                 │
│ • Protocol State Management ✅          │
│ • Liquidity Rebalancing ⏳              │
│ • Crank Mechanism ⏳                    │
└──────────────────┬──────────────────────┘
                   │
       ┌───────────┴────────────┐
       │                        │
       ▼                        ▼
┌──────────────┐        ┌─────────────────┐
│ Token2022    │        │  DLMM Whirlpool │
│ (IRMA/USDC)  │        │  (Liquidity)    │
└──────────────┘        └─────────────────┘
```

---

## Key Formulas

### Inflation Adjustment
```
multiplier = (1 + inflation_rate) ^ (time_elapsed_seconds / seconds_per_year)
new_mint_price = current_mint_price * multiplier
```

Example: 5% annual inflation after 1 second:
```
multiplier = (1.05) ^ (1 / 31,557,600) ≈ 1.0000000015789
```

### Redemption Price
```
redemption_price = backing_reserves_usd / irma_in_circulation
```

This is calculated dynamically from the StateMap in pricing.rs and increases as users mint/swap stablecoins for IRMA.

---

## Contributing

1. Create a feature branch: `git checkout -b feature/your-feature`
2. Make changes and test: `anchor build && npm test`
3. Commit: `git commit -m "feat: description"`
4. Push: `git push origin feature/your-feature`
5. Create Pull Request on GitHub

---

## Technology Stack

| Component | Version | Purpose |
|-----------|---------|---------|
| Solana | Devnet | Blockchain network |
| Anchor | 0.32.1 | Smart contract framework |
| Rust | 1.70+ | Program language |
| TypeScript | Latest | Testing & scripts |
| SPL Token2022 | Latest | Token standard |
| DLMM Whirlpools | V2 | DEX integration |

---

## License

[Specify your license here]

---

## Support

- 📖 See [SETUP_GUIDE.md](./SETUP_GUIDE.md) for setup issues
- 📊 Check [MILESTONE_COMPLETION.md](./MILESTONE_COMPLETION.md) for progress
- 🔧 Review [IMPLEMENTATION_GUIDE_NEXT_STEPS.md](./IMPLEMENTATION_GUIDE_NEXT_STEPS.md) for technical details
- 📝 See [SESSION_SUMMARY.md](./SESSION_SUMMARY.md) for latest updates

---

**Last Updated**: October 21, 2025
**Status**: Phase 1 (90% complete) - Ready for Phase 2 implementation
**Network**: Solana Devnet
