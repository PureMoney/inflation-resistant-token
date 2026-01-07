# On-Chain Tests for Commons

This directory contains on-chain integration tests that have been converted from the original `solana_program_test` based tests to use actual Solana blockchain infrastructure.

## Overview

The on-chain tests are designed to:
- Test swap functionality against a real or local Solana validator
- Validate Token 2022 integration and transfer fee calculations
- Provide comprehensive coverage of the DLMM (Dynamic Liquidity Market Maker) functionality
- Test real-world scenarios with actual blockchain state

## Test Files

### Core Test Framework
- `main.rs` - Main test configuration and setup utilities
- `utils.rs` - On-chain testing utilities and helper functions

### Test Suites
- `test_swap.rs` - Tests for basic swap operations using SPL tokens
- `test_swap_token2022.rs` - Tests for Token 2022 integration and transfer fees

## Prerequisites

Before running the on-chain tests, ensure you have:

1. **Solana CLI installed**: Download from [https://docs.solana.com/cli/install-solana-cli-tools](https://docs.solana.com/cli/install-solana-cli-tools)
2. **Local validator running** (for local testing):
   ```bash
   solana-test-validator
   ```
3. **Sufficient SOL** in your test wallet for transactions

## Configuration

### Local Testing (Recommended)
The tests are configured to use a local Solana validator by default. This provides:
- Fast execution
- Deterministic results
- No external dependencies
- Full control over blockchain state

### Devnet Testing
To test against Devnet, modify the `Cluster` configuration in `main.rs`:
```rust
let cluster = Cluster::Devnet; // Change from Cluster::Localnet
```

**Note**: Devnet testing requires actual SOL for transaction fees.

## Running the Tests

### Option 1: Using Cargo (Recommended)
```bash
# Navigate to the on-chain tests directory
cd programs/commons/tests/on_chain

# Run all on-chain tests
cargo test

# Run specific test
cargo test test_swap_exact_out_on_chain

# Run with output
cargo test -- --nocapture
```

### Option 2: Using Anchor (if integrated)
```bash
# From the project root
anchor test --skip-lint

# Or run specific test file
anchor test programs/commons/tests/on_chain/test_swap.rs
```

## Test Structure

### OnChainTestConfig
The main configuration struct that handles:
- Connection to Solana cluster (local/devnet/mainnet)
- Payer keypair management
- SOL airdrops for testing
- Token mint and account creation

### OnChainTestPair
A convenience struct that sets up:
- Token X and Token Y mints
- User token accounts
- Mock LB pair configuration
- Reserve accounts

## Key Differences from Integration Tests

| Aspect | Integration Tests | On-Chain Tests |
|--------|------------------|----------------|
| **Environment** | Simulated (`solana_program_test`) | Real blockchain |
| **Speed** | Very fast | Slower (real transactions) |
| **State** | Mocked account data | Actual account creation |
| **Network** | No network calls | Real RPC calls |
| **Fees** | Simulated | Real SOL required |
| **Debugging** | Easier (local state) | More complex (blockchain state) |

## Test Coverage

### Basic Swap Tests (`test_swap.rs`)
- ✅ `test_swap_exact_out_on_chain` - Tests exact output swap calculations
- ✅ `test_swap_exact_in_on_chain` - Tests exact input swap calculations

### Token 2022 Tests (`test_swap_token2022.rs`)
- ✅ `test_swap_token2022_exact_out_on_chain` - Token 2022 swap with extensions
- ✅ `test_token2022_transfer_fee_calculation` - Transfer fee calculations

## Troubleshooting

### Common Issues

1. **"Airdrop failed"**
   - Ensure local validator is running: `solana-test-validator`
   - Check if validator has sufficient funds

2. **"Failed to get blockhash"**
   - Network connectivity issue
   - Validator might be down
   - Check `solana config get` for correct RPC URL

3. **"Transaction failed"**
   - Insufficient SOL for transaction fees
   - Program might not be deployed on target network
   - Check transaction logs for specific error

4. **"Account not found"**
   - Accounts need to be created on-chain first
   - Ensure mint and token accounts are properly initialized

### Debugging Tips

1. **Enable verbose output**:
   ```bash
   RUST_LOG=debug cargo test -- --nocapture
   ```

2. **Check validator logs**:
   ```bash
   solana logs
   ```

3. **Inspect account states**:
   ```bash
   solana account <ACCOUNT_PUBKEY>
   ```

## Performance Considerations

- On-chain tests are slower than integration tests due to real blockchain operations
- Each test creates new accounts, which requires multiple transactions
- Consider using test fixtures or shared setup for large test suites
- Local validator testing is recommended for development

## Future Enhancements

- [ ] Add support for mainnet-fork testing
- [ ] Implement test data persistence between runs
- [ ] Add comprehensive error scenario testing
- [ ] Include governance and admin operation tests
- [ ] Add performance benchmarking tests

## Contributing

When adding new on-chain tests:

1. Follow the existing pattern in test files
2. Use the `OnChainTestConfig` and `OnChainTestPair` utilities
3. Add comprehensive assertions and error handling
4. Document any special setup requirements
5. Test against both local validator and devnet when possible

## License

These tests are part of the Inflation-Resistant Stablecoin project and follow the same license terms.