#!/bin/bash

# On-Chain Test Runner Script
# This script sets up the environment and runs the on-chain tests

set -e

echo "🚀 Starting On-Chain Tests for Commons"
echo "======================================="

# Check if solana CLI is installed
if ! command -v solana &> /dev/null; then
    echo "❌ Solana CLI not found. Please install it first:"
    echo "   https://docs.solana.com/cli/install-solana-cli-tools"
    exit 1
fi

# Check if local validator is running
if ! solana cluster-version &> /dev/null; then
    echo "⚠️  Local validator not detected. Starting one..."
    echo "   If you want to use devnet instead, update main.rs to use Cluster::Devnet"
    
    # Start validator in background
    solana-test-validator --reset --quiet &
    VALIDATOR_PID=$!
    
    # Wait for validator to start
    echo "   Waiting for validator to start..."
    sleep 5
    
    # Set cleanup trap
    trap "echo '🧹 Cleaning up...'; kill $VALIDATOR_PID 2>/dev/null || true" EXIT
else
    echo "✅ Solana validator detected"
fi

# Display current configuration
echo ""
echo "Current Solana Configuration:"
echo "-----------------------------"
solana config get

# Set to localhost for local testing
echo ""
echo "🔧 Setting up local configuration..."
solana config set --url localhost

# Check validator health
echo ""
echo "🏥 Checking validator health..."
if solana ping -c 1; then
    echo "✅ Validator is healthy"
else
    echo "❌ Validator health check failed"
    exit 1
fi

# Run the tests
echo ""
echo "🧪 Running On-Chain Tests..."
echo "=============================="

cd "$(dirname "$0")"

# Run tests with output
if cargo test -- --nocapture; then
    echo ""
    echo "✅ All on-chain tests passed!"
else
    echo ""
    echo "❌ Some tests failed. Check the output above for details."
    exit 1
fi

echo ""
echo "🎉 On-chain testing completed successfully!"
echo ""
echo "📊 Test Summary:"
echo "  - Environment: $(solana config get | grep 'RPC URL' | awk '{print $3}')"
echo "  - Tests: Swap operations, Token 2022 integration"
echo "  - Status: All tests passed ✅"