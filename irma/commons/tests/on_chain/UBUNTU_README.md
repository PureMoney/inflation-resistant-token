# Ubuntu Test Runner Instructions

This directory contains Ubuntu-optimized scripts for running on-chain tests.

## Files

- `run_tests_ubuntu.sh` - Main test runner script for Ubuntu/Debian systems
- `run_tests.sh` - Generic Unix/Linux script (also works on Ubuntu)
- `run_tests.bat` - Windows batch script

## Ubuntu-Specific Features

The Ubuntu script (`run_tests_ubuntu.sh`) includes:

### Automatic Dependency Installation
- Solana CLI installation via official installer
- Rust/Cargo installation if missing
- System packages: `build-essential`, `pkg-config`, `libudev-dev`, `libssl-dev`

### Enhanced Error Handling
- Colored output for better readability
- Comprehensive error messages with troubleshooting tips
- Automatic retry mechanisms for validator startup

### System Checks
- Disk space verification
- Memory usage monitoring
- Process cleanup on exit

### Ubuntu Optimizations
- APT package manager integration
- Systemd-aware process management
- Ubuntu-specific path configurations

## Usage

### Option 1: Direct Execution
```bash
# Make executable (first time only)
chmod +x run_tests_ubuntu.sh

# Run tests
./run_tests_ubuntu.sh
```

### Option 2: With Bash
```bash
bash run_tests_ubuntu.sh
```

### Option 3: One-liner Setup and Run
```bash
chmod +x run_tests_ubuntu.sh && ./run_tests_ubuntu.sh
```

## Prerequisites Handling

The script automatically handles missing prerequisites:

### If Solana CLI is missing:
- Downloads and installs Solana CLI v1.17.0
- Adds to PATH automatically
- Verifies installation

### If Rust is missing:
- Downloads and installs Rust via rustup
- Sources cargo environment
- Verifies installation

### If system packages are missing:
- Updates APT package lists
- Installs required development packages
- Verifies installation

## Troubleshooting

### Permission Denied
```bash
chmod +x run_tests_ubuntu.sh
```

### Script Won't Start Validator
```bash
# Check if port 8899 is in use
sudo netstat -tlnp | grep :8899

# Kill existing processes
pkill -f solana-test-validator
```

### Rust/Cargo Not Found After Install
```bash
# Manually source cargo environment
source ~/.cargo/env

# Or restart terminal
```

### Package Installation Fails
```bash
# Update package lists
sudo apt update

# Install manually
sudo apt install build-essential pkg-config libudev-dev libssl-dev
```

### Tests Timeout
```bash
# Run with extended timeout
RUST_LOG=debug timeout 600s ./run_tests_ubuntu.sh
```

## Environment Variables

The script supports these environment variables:

```bash
# Enable debug logging
export RUST_LOG=debug

# Increase test timeout
export TEST_TIMEOUT=600

# Use different Solana version
export SOLANA_VERSION=v1.17.0

# Custom validator args
export VALIDATOR_ARGS="--reset --quiet --bpf-program ..."
```

## Advanced Usage

### Custom Solana Installation
```bash
# Set custom install path
export SOLANA_INSTALL_PATH="$HOME/solana"
./run_tests_ubuntu.sh
```

### Using System Packages Instead of Downloads
```bash
# Install Solana via system package manager (if available)
sudo snap install solana --classic
./run_tests_ubuntu.sh
```

### Running Against Devnet
Edit `main.rs` before running:
```rust
let cluster = Cluster::Devnet; // Change from Cluster::Localnet
```

Then run:
```bash
./run_tests_ubuntu.sh
```

## Performance Tips

### For Faster Test Execution
```bash
# Use ramdisk for temporary files (if you have enough RAM)
sudo mkdir -p /mnt/ramdisk
sudo mount -t tmpfs -o size=2G tmpfs /mnt/ramdisk
export TMPDIR=/mnt/ramdisk

./run_tests_ubuntu.sh
```

### For CI/CD Integration
```bash
# Non-interactive mode
export DEBIAN_FRONTEND=noninteractive
sudo apt-get update -qq
sudo apt-get install -y -qq build-essential pkg-config libudev-dev libssl-dev

./run_tests_ubuntu.sh
```

## Docker Support

You can also run in Docker:

```dockerfile
FROM ubuntu:22.04

RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libudev-dev \
    libssl-dev \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY . /app
WORKDIR /app/programs/commons/tests/on_chain

RUN chmod +x run_tests_ubuntu.sh
CMD ["./run_tests_ubuntu.sh"]
```

## Integration with IDEs

### VS Code
Add to `.vscode/tasks.json`:
```json
{
    "label": "Run On-Chain Tests (Ubuntu)",
    "type": "shell",
    "command": "./programs/commons/tests/on_chain/run_tests_ubuntu.sh",
    "group": "test",
    "presentation": {
        "echo": true,
        "reveal": "always",
        "focus": false,
        "panel": "shared"
    }
}
```

### IntelliJ/CLion
Add as an external tool with:
- Program: `bash`
- Arguments: `programs/commons/tests/on_chain/run_tests_ubuntu.sh`
- Working Directory: `$ProjectFileDir$`