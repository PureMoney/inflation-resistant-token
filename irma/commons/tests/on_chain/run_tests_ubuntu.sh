#!/bin/bash

# On-Chain Test Runner Script for Ubuntu/Debian
# This script sets up the environment and runs the on-chain tests

set -e

echo "🚀 Starting On-Chain Tests for Commons (Ubuntu)"
echo "==============================================="

# Colors for better output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}✅${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠️${NC} $1"
}

print_error() {
    echo -e "${RED}❌${NC} $1"
}

print_info() {
    echo -e "${BLUE}ℹ️${NC} $1"
}

# Check if running on Ubuntu/Debian
if ! command -v apt &> /dev/null && ! command -v apt-get &> /dev/null; then
    print_warning "This script is optimized for Ubuntu/Debian systems"
    print_info "For other distributions, please ensure dependencies are installed manually"
fi

# Check if solana CLI is installed
if ! command -v solana &> /dev/null; then
    print_error "Solana CLI not found. Installing..."
    
    # Install Solana CLI for Ubuntu
    echo "📦 Installing Solana CLI..."
    
    # Download and install Solana
    if command -v curl &> /dev/null; then
        sh -c "$(curl -sSfL https://release.solana.com/v1.17.0/install)"
    elif command -v wget &> /dev/null; then
        sh -c "$(wget -qO- https://release.solana.com/v1.17.0/install)"
    else
        print_error "Neither curl nor wget found. Please install one of them first:"
        echo "   sudo apt update && sudo apt install curl"
        exit 1
    fi
    
    # Add to PATH
    export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
    
    # Verify installation
    if command -v solana &> /dev/null; then
        print_status "Solana CLI installed successfully"
    else
        print_error "Solana CLI installation failed"
        print_info "Please add Solana to your PATH manually:"
        echo '   export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"'
        echo "   Then restart your terminal and run this script again"
        exit 1
    fi
else
    print_status "Solana CLI found"
fi

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    print_error "Rust/Cargo not found. Installing..."
    
    # Install Rust
    echo "🦀 Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    
    # Source cargo environment
    source "$HOME/.cargo/env"
    
    if command -v cargo &> /dev/null; then
        print_status "Rust installed successfully"
    else
        print_error "Rust installation failed"
        exit 1
    fi
else
    print_status "Rust/Cargo found"
fi

# Check system dependencies
print_info "Checking system dependencies..."

# Check for required packages
REQUIRED_PACKAGES=("build-essential" "pkg-config" "libudev-dev" "libssl-dev")
MISSING_PACKAGES=()

for package in "${REQUIRED_PACKAGES[@]}"; do
    if ! dpkg -l | grep -q "^ii  $package "; then
        MISSING_PACKAGES+=("$package")
    fi
done

if [ ${#MISSING_PACKAGES[@]} -ne 0 ]; then
    print_warning "Missing required packages: ${MISSING_PACKAGES[*]}"
    echo "🔧 Installing missing packages..."
    
    # Update package list
    sudo apt update
    
    # Install missing packages
    sudo apt install -y "${MISSING_PACKAGES[@]}"
    
    print_status "System dependencies installed"
else
    print_status "All system dependencies are satisfied"
fi

# Check if local validator is running
print_info "Checking validator status..."
if ! solana cluster-version &> /dev/null; then
    print_warning "Local validator not detected. Starting one..."
    print_info "If you want to use devnet instead, update main.rs to use Cluster::Devnet"
    
    # Kill any existing validator processes
    pkill -f solana-test-validator || true
    sleep 2
    
    # Start validator in background
    nohup solana-test-validator --reset --quiet > validator.log 2>&1 &
    VALIDATOR_PID=$!
    
    # Wait for validator to start
    echo "   Waiting for validator to start..."
    sleep 8
    
    # Check if validator started successfully
    if ! solana cluster-version &> /dev/null; then
        print_error "Failed to start local validator"
        print_info "Check validator.log for details"
        exit 1
    fi
    
    # Set cleanup trap
    trap "echo '🧹 Cleaning up...'; kill $VALIDATOR_PID 2>/dev/null || true; pkill -f solana-test-validator || true" EXIT
    
    print_status "Local validator started successfully"
else
    print_status "Solana validator detected and running"
fi

# Display current configuration
echo ""
print_info "Current Solana Configuration:"
echo "-----------------------------"
solana config get

# Set to localhost for local testing
echo ""
print_info "Setting up local configuration..."
solana config set --url localhost

# Verify solana configuration
if ! solana config get | grep -q "localhost"; then
    print_warning "Failed to set localhost configuration, trying 127.0.0.1:8899"
    solana config set --url http://127.0.0.1:8899
fi

# Check validator health
echo ""
print_info "Checking validator health..."
if timeout 10s solana ping -c 1; then
    print_status "Validator is healthy"
else
    print_error "Validator health check failed"
    print_info "Trying to restart validator..."
    
    # Kill existing validator
    pkill -f solana-test-validator || true
    sleep 2
    
    # Start fresh validator
    nohup solana-test-validator --reset --quiet > validator.log 2>&1 &
    VALIDATOR_PID=$!
    sleep 5
    
    if timeout 10s solana ping -c 1; then
        print_status "Validator restarted successfully"
    else
        print_error "Unable to establish healthy validator connection"
        print_info "Please check your firewall settings and ensure no other processes are using port 8899"
        exit 1
    fi
fi

# Check available disk space
AVAILABLE_SPACE=$(df . | tail -1 | awk '{print $4}')
if [ "$AVAILABLE_SPACE" -lt 1000000 ]; then  # Less than 1GB
    print_warning "Low disk space detected. Ensure you have at least 1GB free space"
fi

# Run the tests
echo ""
print_info "Running On-Chain Tests..."
echo "=============================="

# Change to script directory
cd "$(dirname "$0")"

# Set Rust environment variables for better performance
export CARGO_INCREMENTAL=1
export RUST_BACKTRACE=1

# Run tests with timeout to prevent hanging
echo "🧪 Executing test suite..."
if timeout 300s cargo test -- --nocapture --test-threads=1; then
    echo ""
    print_status "All on-chain tests passed!"
    
    # Display test summary
    echo ""
    echo "🎉 On-chain testing completed successfully!"
    echo ""
    echo "📊 Test Summary:"
    echo "  - Environment: $(solana config get | grep 'RPC URL' | awk '{print $3}')"
    echo "  - Platform: Ubuntu/Linux"
    echo "  - Tests: Swap operations, Token 2022 integration"
    echo "  - Status: All tests passed ✅"
    echo ""
    print_info "Test logs are available in the current directory"
    
else
    echo ""
    print_error "Some tests failed. Check the output above for details."
    echo ""
    print_info "Troubleshooting tips:"
    echo "  1. Check validator.log for validator issues"
    echo "  2. Ensure sufficient disk space is available"
    echo "  3. Verify no firewall is blocking port 8899"
    echo "  4. Try rerunning with: RUST_LOG=debug cargo test -- --nocapture"
    exit 1
fi

# Optional: Display system resource usage
if command -v free &> /dev/null; then
    echo ""
    print_info "System Resource Usage:"
    echo "Memory:"
    free -h | grep -E "Mem|Swap"
fi

print_status "Test execution completed. Check above for results."