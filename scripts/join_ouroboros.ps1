# Ouroboros Network - Node Setup (Windows)
# Join the decentralized network with the new CLI

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "  Ouroboros Network - Quick Join" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host ""

# Detect architecture
$arch = $env:PROCESSOR_ARCHITECTURE
switch ($arch) {
    "AMD64" { $binaryName = "ouro_dag-windows-x64.exe" }
    "ARM64" { $binaryName = "ouro_dag-windows-arm64.exe" }
    default {
        Write-Host "Unsupported architecture: $arch" -ForegroundColor Red
        Write-Host "   Supported: AMD64 (x64), ARM64" -ForegroundColor Yellow
        exit 1
    }
}

# Create installation directory
$installDir = "$env:USERPROFILE\.ouroboros"
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Set-Location $installDir

Write-Host "Downloading Ouroboros node..." -ForegroundColor Yellow
Write-Host "   Architecture: $arch" -ForegroundColor Gray
Write-Host ""

# Download the latest release binary
$downloadUrl = "https://github.com/ouroboros-network/ouroboros/releases/latest/download/$binaryName"
$outputPath = "$installDir\ouro.exe"

try {
    Write-Host "   Downloading from GitHub releases..." -ForegroundColor Gray
    Invoke-WebRequest -Uri $downloadUrl -OutFile $outputPath -UseBasicParsing
    Write-Host "Binary downloaded successfully" -ForegroundColor Green
} catch {
    Write-Host "Download failed - building from source..." -ForegroundColor Yellow
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

    Write-Host "Building from source (this may take 15-30 minutes)..." -ForegroundColor Yellow

    # Clone and build
    Set-Location $env:TEMP
    if (Test-Path "ouroboros") {
        Remove-Item -Recurse -Force ouroboros
    }

    git clone https://github.com/ouroboros-network/ouroboros.git
    Set-Location ouroboros\ouro_dag

    cargo build --release --bin ouro_dag -j 2

    Copy-Item "target\release\ouro_dag.exe" $outputPath
    Set-Location $installDir
}

Write-Host ""

# Get seed node address
$seedNode = if ($env:OUROBOROS_SEED) { $env:OUROBOROS_SEED } else { "136.112.101.176:9001" }

Write-Host "Configuration:" -ForegroundColor Yellow
Write-Host "   Storage: RocksDB (lightweight, no database needed)" -ForegroundColor Gray
Write-Host "   Data directory: $installDir\data" -ForegroundColor Gray
Write-Host "   Seed node: $seedNode" -ForegroundColor Gray
Write-Host ""

# Create data directory
New-Item -ItemType Directory -Force -Path "$installDir\data" | Out-Null

# Set environment variables
[Environment]::SetEnvironmentVariable("DATABASE_PATH", "$installDir\data", "User")
[Environment]::SetEnvironmentVariable("API_ADDRESS", "0.0.0.0:8000", "User")
[Environment]::SetEnvironmentVariable("P2P_ADDRESS", "0.0.0.0:9001", "User")
$env:DATABASE_PATH = "$installDir\data"
$env:API_ADDRESS = "0.0.0.0:8000"
$env:P2P_ADDRESS = "0.0.0.0:9001"

# Create batch file for easy management
$batchContent = @"
@echo off
cd /d "$installDir"
set DATABASE_PATH=$installDir\data
set API_ADDRESS=0.0.0.0:8000
set P2P_ADDRESS=0.0.0.0:9001
ouro.exe join --peer $seedNode --storage rocksdb --rocksdb-path "$installDir\data"
"@
$batchContent | Out-File -FilePath "$installDir\start-node.bat" -Encoding ASCII

# Create status script
$statusContent = @"
@echo off
cd /d "$installDir"
ouro.exe status
"@
$statusContent | Out-File -FilePath "$installDir\status.bat" -Encoding ASCII

Write-Host "Starting Ouroboros node..." -ForegroundColor Yellow

# Start the node in background
$processArgs = "join --peer $seedNode --storage rocksdb --rocksdb-path `"$installDir\data`""

# Start process and capture it
$nodeProcess = Start-Process -FilePath $outputPath -ArgumentList $processArgs -PassThru -WindowStyle Hidden -RedirectStandardOutput "$installDir\node.log" -RedirectStandardError "$installDir\node_error.log"

Write-Host "   Node started with PID: $($nodeProcess.Id)" -ForegroundColor Gray

Start-Sleep -Seconds 5

# Check if still running
$process = Get-Process -Id $nodeProcess.Id -ErrorAction SilentlyContinue
if ($process) {
    Write-Host ""
    Write-Host "==========================================" -ForegroundColor Green
    Write-Host "Node started successfully!" -ForegroundColor Green
    Write-Host "==========================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "Connected to: $seedNode" -ForegroundColor Cyan
    Write-Host "Storage: RocksDB" -ForegroundColor Cyan
    Write-Host "Data directory: $installDir\data" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Commands:" -ForegroundColor Yellow
    Write-Host "   Status:    $installDir\status.bat" -ForegroundColor White
    Write-Host "   Logs:      Get-Content $installDir\node.log -Tail 50 -Wait" -ForegroundColor White
    Write-Host "   Restart:   $installDir\start-node.bat" -ForegroundColor White
    Write-Host "   Stop:      Get-Process ouro | Stop-Process" -ForegroundColor White
    Write-Host ""
    Write-Host "CLI commands:" -ForegroundColor Yellow
    Write-Host "   $installDir\ouro.exe status" -ForegroundColor White
    Write-Host "   $installDir\ouro.exe peers" -ForegroundColor White
    Write-Host "   $installDir\ouro.exe diagnose" -ForegroundColor White
    Write-Host ""
    Write-Host "You're now part of the Ouroboros network!" -ForegroundColor Green
    Write-Host "==========================================" -ForegroundColor Green
} else {
    Write-Host ""
    Write-Host "Error: Node stopped unexpectedly" -ForegroundColor Red
    Write-Host ""
    Write-Host "=== Error Log ===" -ForegroundColor Yellow
    if (Test-Path "$installDir\node_error.log") {
        Get-Content "$installDir\node_error.log" -Tail 20
    }
    Write-Host ""
    Write-Host "=== Output Log ===" -ForegroundColor Yellow
    if (Test-Path "$installDir\node.log") {
        Get-Content "$installDir\node.log" -Tail 20
    }
    Write-Host ""
    Write-Host "To run manually: $installDir\ouro.exe join --peer $seedNode --storage rocksdb --rocksdb-path `"$installDir\data`"" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Press any key to exit..."
    $null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
    exit 1
}
