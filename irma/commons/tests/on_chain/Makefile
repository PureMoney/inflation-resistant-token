# Makefile for On-Chain Tests (Ubuntu/Linux)
# Provides convenient targets for running tests and managing the environment

.PHONY: test setup clean validator logs help install-deps

# Default target
test: setup
	@echo "🧪 Running on-chain tests..."
	@./run_tests_ubuntu.sh

# Setup environment and dependencies
setup:
	@echo "🔧 Setting up test environment..."
	@chmod +x run_tests_ubuntu.sh run_tests.sh 2>/dev/null || true
	@if [ ! -f ~/.local/share/solana/install/active_release/bin/solana ]; then \
		echo "Installing Solana CLI..."; \
		sh -c "$$(curl -sSfL https://release.solana.com/v1.17.0/install)"; \
	fi

# Install system dependencies
install-deps:
	@echo "📦 Installing system dependencies..."
	@sudo apt update
	@sudo apt install -y build-essential pkg-config libudev-dev libssl-dev curl

# Start local validator
validator:
	@echo "🚀 Starting local Solana validator..."
	@pkill -f solana-test-validator || true
	@sleep 2
	@solana-test-validator --reset --quiet &
	@echo "Validator started. Use 'make logs' to view logs."

# View validator logs
logs:
	@echo "📋 Validator logs:"
	@tail -f validator.log 2>/dev/null || echo "No validator logs found"

# Clean up processes and temporary files
clean:
	@echo "🧹 Cleaning up..."
	@pkill -f solana-test-validator || true
	@rm -f validator.log
	@cargo clean 2>/dev/null || true

# Run tests with debug output
test-debug: setup
	@echo "🐛 Running tests with debug output..."
	@RUST_LOG=debug ./run_tests_ubuntu.sh

# Run specific test
test-swap: setup
	@echo "🔄 Running swap tests only..."
	@cargo test test_swap

test-token2022: setup
	@echo "🪙 Running Token 2022 tests only..."
	@cargo test test_token2022

# Check system requirements
check:
	@echo "✅ Checking system requirements..."
	@echo "Rust: $$(cargo --version 2>/dev/null || echo 'Not installed')"
	@echo "Solana: $$(solana --version 2>/dev/null || echo 'Not installed')"
	@echo "Build tools: $$(gcc --version 2>/dev/null | head -1 || echo 'Not installed')"
	@echo "Disk space: $$(df -h . | tail -1 | awk '{print $$4}') available"

# Help target
help:
	@echo "🚀 On-Chain Test Makefile"
	@echo "========================"
	@echo ""
	@echo "Available targets:"
	@echo "  test          - Run all on-chain tests (default)"
	@echo "  setup         - Setup test environment and dependencies"
	@echo "  install-deps  - Install system dependencies via apt"
	@echo "  validator     - Start local Solana validator"
	@echo "  logs          - View validator logs"
	@echo "  clean         - Clean up processes and temporary files"
	@echo "  test-debug    - Run tests with debug output"
	@echo "  test-swap     - Run only swap tests"
	@echo "  test-token2022- Run only Token 2022 tests"
	@echo "  check         - Check system requirements"
	@echo "  help          - Show this help message"
	@echo ""
	@echo "Examples:"
	@echo "  make test           # Run all tests"
	@echo "  make setup          # Setup environment"
	@echo "  make test-debug     # Run with debug output"
	@echo "  make clean          # Clean up"

# Install Rust if not present
install-rust:
	@if ! command -v cargo >/dev/null 2>&1; then \
		echo "🦀 Installing Rust..."; \
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y; \
		. ~/.cargo/env; \
	else \
		echo "✅ Rust already installed"; \
	fi

# Full setup including all dependencies
full-setup: install-deps install-rust setup
	@echo "🎉 Full setup completed!"

# Quick test for CI environments
ci-test: install-deps setup
	@echo "🤖 Running CI tests..."
	@DEBIAN_FRONTEND=noninteractive ./run_tests_ubuntu.sh