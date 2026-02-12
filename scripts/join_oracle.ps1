# Ouroboros Network - Oracle Node Setup (Windows)
# Run an oracle node and earn rewards for providing real-world data to the blockchain

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "  Ouroboros Oracle Node - Quick Setup" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Oracle nodes provide real-world data (prices, weather, etc.)" -ForegroundColor Gray
Write-Host "to the Ouroboros blockchain and earn OURO rewards." -ForegroundColor Gray
Write-Host ""

# Detect architecture
$arch = $env:PROCESSOR_ARCHITECTURE
switch ($arch) {
    "AMD64" { $binaryName = "ouro-node-windows-x64.exe" }
    "ARM64" { $binaryName = "ouro-node-windows-arm64.exe" }
    default {
        Write-Host "Unsupported architecture: $arch" -ForegroundColor Red
        Write-Host "   Supported: AMD64 (x64), ARM64" -ForegroundColor Yellow
        exit 1
    }
}

# Create installation directory
$installDir = "$env:USERPROFILE\.ouroboros-oracle"
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Set-Location $installDir

Write-Host "Downloading Ouroboros oracle node..." -ForegroundColor Yellow
Write-Host "   Architecture: $arch" -ForegroundColor Gray
Write-Host ""

# Download the latest release binary
$downloadUrl = "https://github.com/ouroboros-network/ouroboros/releases/latest/download/$binaryName"
$outputPath = "$installDir\ouro-oracle.exe"

try {
    Write-Host "   Downloading from GitHub releases..." -ForegroundColor Gray
    Invoke-WebRequest -Uri $downloadUrl -OutFile $outputPath -UseBasicParsing
    Write-Host "Binary downloaded successfully" -ForegroundColor Green
} catch {
    Write-Host "Download failed: $_" -ForegroundColor Red
    Write-Host ""
    Write-Host "Falling back to build from source..." -ForegroundColor Yellow
    Write-Host "   This requires:" -ForegroundColor Yellow
    Write-Host "   - Rust (https://rustup.rs/)" -ForegroundColor Yellow
    Write-Host "   - Git (https://git-scm.com/download/win)" -ForegroundColor Yellow
    Write-Host ""

    # Check dependencies
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Host "Rust not found. Please install from: https://rustup.rs/" -ForegroundColor Red
        Start-Process "https://rustup.rs/"
        exit 1
    }

    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        Write-Host "Git not found. Please install from: https://git-scm.com/download/win" -ForegroundColor Red
        Start-Process "https://git-scm.com/download/win"
        exit 1
    }

    Write-Host "Building from source (this will take 15-30 minutes)..." -ForegroundColor Yellow

    # Clone and build in the install directory (not %TEMP%)
    $buildDir = "$installDir\_build"
    if (Test-Path $buildDir) {
        Remove-Item -Recurse -Force $buildDir
    }

    git clone https://github.com/ouroboros-network/ouroboros.git $buildDir
    Set-Location "$buildDir\ouro_dag"

    cargo build --release --bin ouro-node -j 2

    Copy-Item "target\release\ouro-node.exe" $outputPath
    Set-Location $installDir

    # Clean up build directory
    Remove-Item -Recurse -Force $buildDir -ErrorAction SilentlyContinue
}

Write-Host ""

# Get seed node address
$seedNode = if ($env:OUROBOROS_SEED) { $env:OUROBOROS_SEED } else { "136.112.101.176:9000" }

# Create oracle configuration
$configPath = "$installDir\oracle_config.json"

# Generate a random node ID for this oracle
$nodeId = [System.Guid]::NewGuid().ToString().Substring(0, 8)
$operatorId = "oracle_$nodeId"

# Default oracle configuration
$oracleConfig = @{
    operator_id = $operatorId
    stake = 0
    data_sources = @("coingecko", "binance", "open-meteo")
    update_interval_ms = 5000
    is_validator = $false
    reward_address = $null
} | ConvertTo-Json -Depth 3

# Write config if it doesn't exist
if (-not (Test-Path $configPath)) {
    $oracleConfig | Out-File -FilePath $configPath -Encoding UTF8
    Write-Host "Created oracle config: $configPath" -ForegroundColor Green
} else {
    Write-Host "Using existing oracle config: $configPath" -ForegroundColor Gray
}

Write-Host ""
Write-Host "Oracle Configuration:" -ForegroundColor Yellow
Write-Host "   Operator ID: $operatorId" -ForegroundColor Gray
Write-Host "   Data Sources: CoinGecko, Binance, Open-Meteo" -ForegroundColor Gray
Write-Host "   Update Interval: 5 seconds" -ForegroundColor Gray
Write-Host "   Config File: $configPath" -ForegroundColor Gray
Write-Host ""

# Create data directory
New-Item -ItemType Directory -Force -Path "$installDir\data" | Out-Null

# Create batch file for easy oracle management
$batchContent = @"
@echo off
cd /d "$installDir"
echo Starting Ouroboros Oracle Node...
echo Operator ID: $operatorId
echo.
ouro-oracle.exe oracle --peer $seedNode --config "$configPath" --storage rocksdb --rocksdb-path "$installDir\data" --api-port 8002
"@
$batchContent | Out-File -FilePath "$installDir\start-oracle.bat" -Encoding ASCII

# Create stop script
$stopContent = @"
@echo off
taskkill /IM ouro-oracle.exe /F 2>nul
echo Oracle node stopped.
"@
$stopContent | Out-File -FilePath "$installDir\stop-oracle.bat" -Encoding ASCII

# Start the oracle node
Write-Host "Starting Ouroboros oracle node..." -ForegroundColor Yellow

$processArgs = "oracle --peer $seedNode --config `"$configPath`" --storage rocksdb --rocksdb-path `"$installDir\data`" --api-port 8002"
Start-Process -FilePath $outputPath -ArgumentList $processArgs -WindowStyle Hidden -RedirectStandardOutput "$installDir\oracle.log" -RedirectStandardError "$installDir\oracle_error.log"

Start-Sleep -Seconds 3

# Check if running
$process = Get-Process ouro-oracle -ErrorAction SilentlyContinue
if ($process) {
    Write-Host ""
    Write-Host "==========================================" -ForegroundColor Green
    Write-Host "Oracle node started successfully!" -ForegroundColor Green
    Write-Host "==========================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "Operator ID: $operatorId" -ForegroundColor Cyan
    Write-Host "Connected to: $seedNode" -ForegroundColor Cyan
    Write-Host "Data directory: $installDir\data" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Data Feeds Being Provided:" -ForegroundColor Yellow
    Write-Host "   - BTC/USD price (from CoinGecko, Binance)" -ForegroundColor White
    Write-Host "   - ETH/USD price (from CoinGecko, Binance)" -ForegroundColor White
    Write-Host "   - Weather data (from Open-Meteo)" -ForegroundColor White
    Write-Host ""
    Write-Host "Check logs:" -ForegroundColor Yellow
    Write-Host "   Get-Content $installDir\oracle.log -Tail 50 -Wait" -ForegroundColor White
    Write-Host ""
    Write-Host "Check oracle status:" -ForegroundColor Yellow
    Write-Host "   curl http://localhost:8002/oracle/status" -ForegroundColor White
    Write-Host ""
    Write-Host "Manage oracle:" -ForegroundColor Yellow
    Write-Host "   Start: $installDir\start-oracle.bat" -ForegroundColor White
    Write-Host "   Stop: $installDir\stop-oracle.bat" -ForegroundColor White
    Write-Host ""
    Write-Host "Edit configuration:" -ForegroundColor Yellow
    Write-Host "   notepad $configPath" -ForegroundColor White
    Write-Host ""
    Write-Host "==========================================" -ForegroundColor Green
    Write-Host "EARNING REWARDS" -ForegroundColor Green
    Write-Host "==========================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "To earn OURO rewards for providing oracle data:" -ForegroundColor Yellow
    Write-Host "1. Stake at least 5,000 OURO (edit oracle_config.json)" -ForegroundColor White
    Write-Host "2. Set your reward_address in the config" -ForegroundColor White
    Write-Host "3. Restart the oracle node" -ForegroundColor White
    Write-Host ""
    Write-Host "Your oracle is now providing data to the network!" -ForegroundColor Green
    Write-Host "==========================================" -ForegroundColor Green
} else {
    Write-Host ""
    Write-Host "Error: Oracle node failed to start" -ForegroundColor Red
    Write-Host "Check logs: Get-Content $installDir\oracle.log -Tail 50" -ForegroundColor Yellow
    Write-Host "Check errors: Get-Content $installDir\oracle_error.log -Tail 50" -ForegroundColor Yellow
    exit 1
}
