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
New-Item -ItemType Directory -Force -Path "$installDir\data" | Out-Null

Write-Host "[1/4] Downloading Ouroboros node..." -ForegroundColor Yellow
Write-Host "      Architecture: $arch" -ForegroundColor Gray

# Download the latest release binary
$downloadUrl = "https://github.com/ouroboros-network/ouroboros/releases/latest/download/$binaryName"
$outputPath = "$installDir\ouro-bin.exe"

try {
    Write-Host "      Downloading from GitHub releases..." -ForegroundColor Gray
    # Use curl.exe (built into Windows 10+) which handles redirects properly
    $curlResult = & curl.exe -L -s -o $outputPath -w "%{http_code}" $downloadUrl 2>$null
    if ($curlResult -ne "200" -or -not (Test-Path $outputPath) -or (Get-Item $outputPath).Length -lt 1000000) {
        # Fallback to .NET WebClient with TLS 1.2
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $webClient = New-Object System.Net.WebClient
        $webClient.DownloadFile($downloadUrl, $outputPath)
    }
    if (-not (Test-Path $outputPath) -or (Get-Item $outputPath).Length -lt 1000000) {
        throw "Download incomplete or file too small"
    }
    Write-Host "      Binary downloaded successfully" -ForegroundColor Green
} catch {
    Write-Host "      Download failed - building from source..." -ForegroundColor Yellow
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

# Stop any existing node process first
Write-Host "[2/4] Checking for existing node..." -ForegroundColor Yellow
$existingProcess = Get-Process -Name "ouro-bin" -ErrorAction SilentlyContinue
if ($existingProcess) {
    Write-Host "      Stopping existing node (PID: $($existingProcess.Id))..." -ForegroundColor Gray
    $existingProcess | Stop-Process -Force
    Start-Sleep -Seconds 2
}

# Remove stale lock file if exists
$lockFile = "$installDir\data\LOCK"
if (Test-Path $lockFile) {
    Remove-Item -Force $lockFile -ErrorAction SilentlyContinue
}

# Configuration
Write-Host "[3/4] Configuring node..." -ForegroundColor Yellow
$seedNode = if ($env:OUROBOROS_SEED) { $env:OUROBOROS_SEED } else { "136.112.101.176:9000" }
$envFile = "$installDir\.env"

# Check if existing config has required keys
$needsNewConfig = $true
if (Test-Path $envFile) {
    $envContent = Get-Content $envFile -Raw
    if ($envContent -match "API_KEYS=" -and $envContent -match "BFT_SECRET_SEED=") {
        Write-Host "      Using existing configuration" -ForegroundColor Gray
        $needsNewConfig = $false
    } else {
        Write-Host "      Upgrading configuration (adding required keys)..." -ForegroundColor Gray
    }
}

if ($needsNewConfig) {
    # Generate random secrets
    $bftSecret = -join ((1..64) | ForEach-Object { "{0:x}" -f (Get-Random -Maximum 16) })
    $nodeId = "node-" + -join ((1..8) | ForEach-Object { "{0:x}" -f (Get-Random -Maximum 16) })
    $apiKey = -join ((1..32) | ForEach-Object { "{0:x}" -f (Get-Random -Maximum 16) })
    Write-Host "      Generated new node identity: $nodeId" -ForegroundColor Gray

    # Save to .env file - USE CONSISTENT PORTS: API=8000, P2P=9000
    @"
# Ouroboros Node Configuration
DATABASE_PATH=$installDir\data
API_ADDR=0.0.0.0:8000
LISTEN_ADDR=0.0.0.0:9000
PEER_ADDRS=$seedNode
NODE_ID=$nodeId
BFT_SECRET_SEED=$bftSecret
API_KEYS=$apiKey
RUST_LOG=info
"@ | Out-File -FilePath $envFile -Encoding ASCII
}

# Load environment variables for current session
Get-Content $envFile | ForEach-Object {
    if ($_ -match "^([^#][^=]+)=(.*)$") {
        $name = $matches[1].Trim()
        $value = $matches[2].Trim()
        [Environment]::SetEnvironmentVariable($name, $value, "Process")
    }
}

Write-Host "      API Port: 8000" -ForegroundColor Gray
Write-Host "      P2P Port: 9000" -ForegroundColor Gray
Write-Host "      Seed node: $seedNode" -ForegroundColor Gray

# Create helper scripts
Write-Host "[4/4] Creating helper scripts..." -ForegroundColor Yellow

# Create start script
@"
@echo off
cd /d "$installDir"
for /f "usebackq eol=# tokens=1,* delims==" %%a in (".env") do set "%%a=%%b"
ouro-bin.exe start
"@ | Out-File -FilePath "$installDir\start-node.bat" -Encoding ASCII

# Create status script
@"
@echo off
cd /d "$installDir"
for /f "usebackq eol=# tokens=1,* delims==" %%a in (".env") do set "%%a=%%b"
ouro-bin.exe status
"@ | Out-File -FilePath "$installDir\status.bat" -Encoding ASCII

# Create stop script
@"
@echo off
echo Stopping Ouroboros node...
taskkill /IM ouro-bin.exe /F 2>nul
if %ERRORLEVEL% EQU 0 (
    echo Node stopped successfully.
) else (
    echo No running node found.
)
"@ | Out-File -FilePath "$installDir\stop-node.bat" -Encoding ASCII

# Create wrapper batch file for ouro command
@"
@echo off
cd /d "$installDir"
for /f "usebackq eol=# tokens=1,* delims==" %%a in (".env") do set "%%a=%%b"
"$installDir\ouro-bin.exe" %*
"@ | Out-File -FilePath "$installDir\ouro.bat" -Encoding ASCII

# Create PowerShell wrapper
@'
$envFile = "$env:USERPROFILE\.ouroboros\.env"
if (Test-Path $envFile) {
    Get-Content $envFile | ForEach-Object {
        if ($_ -match "^([^#][^=]+)=(.*)$") {
            [Environment]::SetEnvironmentVariable($matches[1].Trim(), $matches[2].Trim(), "Process")
        }
    }
}
& "$env:USERPROFILE\.ouroboros\ouro-bin.exe" $args
'@ | Out-File -FilePath "$installDir\ouro.ps1" -Encoding UTF8

# Add to PATH
$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($currentPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$installDir;$currentPath", "User")
    $env:Path = "$installDir;$env:Path"
}

Write-Host ""
Write-Host "Starting Ouroboros node..." -ForegroundColor Yellow

# Start the node using cmd.exe to run the batch file (ensures env vars are loaded)
$nodeProcess = Start-Process -FilePath "cmd.exe" -ArgumentList "/c", "`"$installDir\start-node.bat`"" -WorkingDirectory $installDir -PassThru -WindowStyle Hidden -RedirectStandardOutput "$installDir\node.log" -RedirectStandardError "$installDir\node_error.log"

Write-Host "   Node started with PID: $($nodeProcess.Id)" -ForegroundColor Gray

# Wait for node to initialize
Start-Sleep -Seconds 5

# Check if node is running by looking for ouro-bin process
$ouroProcess = Get-Process -Name "ouro-bin" -ErrorAction SilentlyContinue
$nodeRunning = $null -ne $ouroProcess
$apiResponding = $false

if ($nodeRunning) {
    # Try to connect to API
    try {
        $response = Invoke-WebRequest -Uri "http://localhost:8000/health" -UseBasicParsing -TimeoutSec 5 -ErrorAction Stop
        $apiResponding = $true
    } catch {
        # API might still be starting up
        Start-Sleep -Seconds 3
        try {
            $response = Invoke-WebRequest -Uri "http://localhost:8000/health" -UseBasicParsing -TimeoutSec 5 -ErrorAction Stop
            $apiResponding = $true
        } catch {}
    }
}

if ($nodeRunning) {
    Write-Host ""
    Write-Host "==========================================" -ForegroundColor Green
    Write-Host "  Node started successfully!" -ForegroundColor Green
    Write-Host "==========================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "  Seed node: $seedNode" -ForegroundColor Cyan
    Write-Host "  API: http://localhost:8000" -ForegroundColor Cyan
    Write-Host "  Data: $installDir\data" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Commands:" -ForegroundColor Yellow
    Write-Host "  ouro status     - View node dashboard" -ForegroundColor White
    Write-Host "  ouro peers      - List connected peers" -ForegroundColor White
    Write-Host "  ouro diagnose   - Run diagnostics" -ForegroundColor White
    Write-Host ""
    Write-Host "Management:" -ForegroundColor Yellow
    Write-Host "  $installDir\stop-node.bat   - Stop the node" -ForegroundColor White
    Write-Host "  $installDir\start-node.bat  - Start the node" -ForegroundColor White
    Write-Host "  $installDir\status.bat      - Quick status check" -ForegroundColor White
    Write-Host ""
    Write-Host "Logs:" -ForegroundColor Yellow
    Write-Host "  Get-Content $installDir\node.log -Tail 50 -Wait" -ForegroundColor White
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
    Write-Host "Try running manually: $installDir\start-node.bat" -ForegroundColor Cyan
    exit 1
}
