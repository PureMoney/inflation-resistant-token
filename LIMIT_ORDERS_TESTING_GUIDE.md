# IRMA Limit Orders Testing Guide

## Overview

This document provides comprehensive guidance for testing the IRMA limit orders feature, which is implemented in the `feature/remove-counter-swaps-add-limit-orders` branch.

**Current Status**: Tests are being developed in the `agents/test-limit-orders` branch.

## What Are Limit Orders in IRMA?

Limit orders in IRMA allow users to place, cancel, and manage orders on Meteora DLMM (Dynamic Liquidity Market Maker) pools. The three main instructions are:

1. **`place_limit_order`** - Places a new limit order on a DLMM pool
2. **`cancel_limit_order`** - Cancels an existing limit order and claims proceeds
3. **`close_limit_order_if_empty`** - Closes an empty limit order account and reclaims rent

## Feature Branch Details

**Branch**: `feature/remove-counter-swaps-add-limit-orders`
**Commits**:
- `8bf657e` - feat: migrate IRMA to Meteora DLMM 0.12.0 limit orders and remove counter-swaps
- `2f57f55` - added Meteora DLMM Liquidity Integration limit order
- `0820d13` - added Meteora DLMM Liquidity Integration limit order
- `14e451b` - resolve the requested commit changes

## Setup Instructions

### Prerequisites

- Node.js v20+ 
- Rust (via `rustup`)
- Anchor Framework 0.32.1
- Solana CLI tools
- Access to Solana devnet

### Installation

```bash
# Navigate to the project directory
cd irma

# Install Node dependencies
npm install
# or
yarn install

# Verify Anchor installation
anchor --version
# Should output: anchor-cli 0.32.1 or compatible

# Verify Rust installation
rustc --version
solana --version
```

### Configuration

The test suite requires environment variables to be set:

1. **Create or update `.env` file** in the `irma/` directory:

```env
# RPC URL for devnet
ANCHOR_PROVIDER_URL=https://api.devnet.solana.com

# Or use local validator
# ANCHOR_PROVIDER_URL=http://localhost:8899

# Commitment level
ANCHOR_COMMITMENT=confirmed

# (Optional) Private key as JSON array
# SOLANA_PRIVATE_KEY=[174,100,53,...]
```

2. **Ensure devnet-config.json** contains the reserve configurations:

```json
{
  "tokens": {
    "USDT": { "name": "devUSDT", "mint": "..." },
    "USDC": { "name": "devUSDC", "mint": "..." },
    // ... other tokens
  }
}
```

3. **Ensure you have SOL on devnet**:

```bash
# Get your public key
solana-keygen pubkey ~/.config/solana/id.json

# Request airdrop (repeat if needed)
solana airdrop 5 <YOUR_PUBLIC_KEY> --url devnet
```

## Building the Project

### Compile the Rust Program

```bash
# Build the IRMA program
anchor build

# Build a specific program
anchor build irma

# Build with specific Solana version
anchor build --solana-version 1.18.4
```

### Generate IDL

```bash
# Generate IDL from Rust code
anchor idl build -o target/idl

# Fetch IDL from deployed program
anchor idl fetch <PROGRAM_ID> -o target/idl/irma.json
```

### TypeScript Code Generation

```bash
# The IDL is automatically used to generate TypeScript types
# Files are generated in:
# - target/types/irma.ts (or similar)
```

## Test File Structure

The main test file is located at: `tests/test_limit_orders.ts`

### Test Organization

The test suite is organized into the following test groups:

#### 1. **place_limit_order Tests**
   - Basic placement on the first configured reserve
   - Reserve validation (invalid reserve handling)
   - Support for both ask and bid sides

#### 2. **cancel_limit_order Tests**
   - Basic cancellation of limit orders
   - Handling multiple bin IDs in a single cancellation
   - Edge case: empty bin ID list

#### 3. **close_limit_order_if_empty Tests**
   - Closing single limit order
   - Multiple consecutive close operations

#### 4. **Integration Tests**
   - Sequential workflow: place → cancel → close
   - Concurrent order placements
   - Transaction construction validation

#### 5. **Edge Cases**
   - Maximum bin ID value (i32::MAX)
   - Minimum bin ID value (i32::MIN)
   - Large amount values (near u64::MAX)
   - Zero amount handling

## Running Tests

### Run All Tests

```bash
# Using npm/yarn test script
npm test

# Or with ts-mocha directly
npx ts-mocha -p ./tsconfig.json -t 1000000 tests/test_limit_orders.ts
```

### Run Specific Test Suite

```bash
# Run only place_limit_order tests
npx ts-mocha -p ./tsconfig.json -t 1000000 tests/test_limit_orders.ts --grep "place_limit_order"

# Run only integration tests
npx ts-mocha -p ./tsconfig.json -t 1000000 tests/test_limit_orders.ts --grep "Integration"

# Run only edge case tests
npx ts-mocha -p ./tsconfig.json -t 1000000 tests/test_limit_orders.ts --grep "Edge Cases"
```

### Run Specific Individual Test

```bash
# Run a single test
npx ts-mocha -p ./tsconfig.json -t 1000000 tests/test_limit_orders.ts --grep "should place a limit order on the first reserve"
```

## Test Coverage

### What the Tests Cover

1. **Instruction Construction**
   - ✅ Building valid `place_limit_order` transactions
   - ✅ Building valid `cancel_limit_order` transactions
   - ✅ Building valid `close_limit_order_if_empty` transactions

2. **Parameter Validation**
   - ✅ Valid reserve symbols
   - ✅ Invalid reserve symbols (should be rejected)
   - ✅ Various bin ID values (low, high, edge cases)
   - ✅ Various amount values (zero, small, large)

3. **Operational Workflows**
   - ✅ Single order placement and cancellation
   - ✅ Sequential operations (place → cancel → close)
   - ✅ Concurrent order placements
   - ✅ Multiple cancellations with different bin ID sets

4. **Account Management**
   - ✅ State PDA derivation
   - ✅ Core PDA derivation
   - ✅ Limit order keypair generation

### What the Tests DON'T Cover (Requires Full DevNet)

1. **On-Chain Execution**
   - Full transaction execution and confirmation
   - State changes on the blockchain
   - DLMM integration validation

2. **DLMM Account Requirements**
   - Actual LbPair account validation
   - Position creation and management
   - Token account interactions
   - Event emission

3. **Price Validation**
   - Actual bin ID to price conversion
   - Order execution validation
   - Slippage calculations

### Note on Test Scope

The current test suite performs **transaction construction and parameter validation** at the SDK level. Full on-chain testing would require:
- A running Solana validator
- DLMM setup on devnet
- Actual token accounts and pools
- Position accounts for each reserve

## Debugging Failed Tests

### Common Issues and Solutions

#### Issue: "Cannot find module '@coral-xyz/anchor'"
```bash
# Solution: Install dependencies
npm install
```

#### Issue: "ANCHOR_PROVIDER_URL not found"
```bash
# Solution: Set environment variable or create .env file
export ANCHOR_PROVIDER_URL=https://api.devnet.solana.com
```

#### Issue: "Insufficient funds"
```bash
# Solution: Airdrop SOL to your wallet
solana airdrop 2 <YOUR_PUBLIC_KEY> --url devnet
```

#### Issue: "Program not found"
```bash
# Solution: Build and deploy the program first
anchor build
anchor deploy
```

#### Issue: "IDL not found at target/idl/irma.json"
```bash
# Solution: Generate the IDL
anchor idl build -o target/idl

# OR fetch from deployed program
anchor idl fetch E15v5VirGqdbH4fYhxxxZHNiLAP3t3y1SPonhrQxoTcs -o target/idl/irma.json
```

## Merging Feature Branch

To integrate limit orders into the current branch:

```bash
# Fetch the latest changes
git fetch origin

# Merge the feature branch
git merge feature/remove-counter-swaps-add-limit-orders

# Or rebase if preferred
git rebase feature/remove-counter-swaps-add-limit-orders

# Resolve any conflicts
git status

# After resolving conflicts
git add .
git commit -m "Merge limit orders feature"
```

## Extending Tests

### Adding More Test Cases

To add new tests for specific scenarios:

```typescript
describe("Limit Orders Custom Tests", () => {
  it("should handle specific scenario X", async function() {
    this.timeout(30000);
    
    // Test implementation
    console.log("Testing scenario X");
    
    // Assertions
    expect(result).to.be.defined;
  });
});
```

### Testing with Real Accounts

For testing against actual pools:

1. Obtain real LbPair accounts from devnet
2. Create limit order accounts using DLMM standards
3. Pass remaining_accounts with proper structure
4. Execute actual transactions with proper signers

## Test Metrics

### Expected Results

When running the full test suite:
- **Total Tests**: ~20+
- **Pass Rate**: 100% (for parameter validation)
- **Execution Time**: ~5-10 seconds per test
- **Total Time**: ~2-3 minutes for full suite

### Test Output Format

```
🆔 Using Program ID from IDL: E15v5VirGqdbH4fYhxxxZHNiLAP3t3y1SPonhrQxoTcs

  Limit Orders Tests
    🌐 Test Environment Setup:
       Payer: ...
       State PDA: ...
       Core PDA: ...
    
    ✓ should place a limit order on the first reserve (123ms)
    ✓ should validate reserve exists (45ms)
    ✓ should support both ask and bid sides (89ms)
    
    place_limit_order
      ✓ should place a limit order on the first reserve
      ✓ should validate reserve exists
      
    ...
    
    25 passing (2.5s)
```

## Resources

- [IRMA Implementation Documentation](./Implementation.md)
- [IRMA Test Results](./TestResults.md)
- [Anchor Documentation](https://book.anchor-lang.com/)
- [Solana Programming Model](https://www.helius.dev/blog/the-solana-programming-model-an-introduction-to-developing-on-solana)
- [Meteora DLMM Documentation](https://meteora.ag/docs)

## Contributing

When adding new tests:

1. Follow the existing test structure
2. Include descriptive console logging (with emojis for clarity)
3. Add appropriate timeout values (typically 30,000ms for devnet tests)
4. Include both happy path and edge case tests
5. Document assumptions and limitations
6. Add comments for complex test logic

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2024 | Initial limit orders test suite |

## Support

For issues or questions:
1. Check existing test documentation
2. Review git commits in `feature/remove-counter-swaps-add-limit-orders`
3. Consult IRMA implementation docs
4. Contact the IRMA development team

---

**Status**: Ready for testing and iteration
**Last Updated**: 2024
**Maintained By**: IRMA Development Team
