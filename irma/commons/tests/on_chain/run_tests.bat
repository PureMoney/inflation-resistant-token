@echo off
setlocal enabledelayedexpansion

echo 🚀 Starting On-Chain Tests for Commons
echo =======================================

REM Check if solana CLI is installed
where solana >nul 2>nul
if %errorlevel% neq 0 (
    echo ❌ Solana CLI not found. Please install it first:
    echo    https://docs.solana.com/cli/install-solana-cli-tools
    exit /b 1
)

REM Check if local validator is running
solana cluster-version >nul 2>nul
if %errorlevel% neq 0 (
    echo ⚠️  Local validator not detected. Starting one...
    echo    If you want to use devnet instead, update main.rs to use Cluster::Devnet
    
    REM Start validator in background
    start /b solana-test-validator --reset --quiet
    
    REM Wait for validator to start
    echo    Waiting for validator to start...
    timeout /t 5 >nul
) else (
    echo ✅ Solana validator detected
)

REM Display current configuration
echo.
echo Current Solana Configuration:
echo -----------------------------
solana config get

REM Set to localhost for local testing
echo.
echo 🔧 Setting up local configuration...
solana config set --url localhost

REM Check validator health
echo.
echo 🏥 Checking validator health...
solana ping -c 1
if %errorlevel% neq 0 (
    echo ❌ Validator health check failed
    exit /b 1
) else (
    echo ✅ Validator is healthy
)

REM Run the tests
echo.
echo 🧪 Running On-Chain Tests...
echo ==============================

cd /d "%~dp0"

REM Run tests with output
cargo test -- --nocapture
if %errorlevel% equ 0 (
    echo.
    echo ✅ All on-chain tests passed!
) else (
    echo.
    echo ❌ Some tests failed. Check the output above for details.
    exit /b 1
)

echo.
echo 🎉 On-chain testing completed successfully!
echo.
echo 📊 Test Summary:
for /f "tokens=3" %%i in ('solana config get ^| findstr "RPC URL"') do set RPC_URL=%%i
echo   - Environment: !RPC_URL!
echo   - Tests: Swap operations, Token 2022 integration
echo   - Status: All tests passed ✅

pause