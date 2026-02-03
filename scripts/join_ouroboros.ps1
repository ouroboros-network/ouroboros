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

# Download the latest release binary - use direct version URL to avoid redirect issues
$downloadUrl = "https://github.com/ouroboros-network/ouroboros/releases/download/v0.4.1/$binaryName"
$outputPath = "$installDir\ouro-bin.exe"

# Force TLS 1.2 for all .NET requests
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$downloadSuccess = $false

# Method 1: bitsadmin (built into all Windows, handles redirects well)
try {
    Write-Host "      Downloading from GitHub releases..." -ForegroundColor Gray
    $null = & bitsadmin /transfer "OuroDownload" /download /priority high $downloadUrl $outputPath 2>&1
    if ((Test-Path $outputPath) -and (Get-Item $outputPath).Length -gt 1000000) {
        $downloadSuccess = $true
        Write-Host "      Download successful (bitsadmin)" -ForegroundColor Green
    }
} catch { }

# Method 2: certutil (also built into Windows)
if (-not $downloadSuccess) {
    try {
        Write-Host "      Trying certutil..." -ForegroundColor Gray
        $null = & certutil -urlcache -split -f $downloadUrl $outputPath 2>&1
        if ((Test-Path $outputPath) -and (Get-Item $outputPath).Length -gt 1000000) {
            $downloadSuccess = $true
            Write-Host "      Download successful (certutil)" -ForegroundColor Green
        }
    } catch { }
}

# Method 3: Start-BitsTransfer PowerShell cmdlet
if (-not $downloadSuccess) {
    try {
        Write-Host "      Trying BitsTransfer..." -ForegroundColor Gray
        Import-Module BitsTransfer -ErrorAction SilentlyContinue
        Start-BitsTransfer -Source $downloadUrl -Destination $outputPath -ErrorAction Stop
        if ((Test-Path $outputPath) -and (Get-Item $outputPath).Length -gt 1000000) {
            $downloadSuccess = $true
            Write-Host "      Download successful (BitsTransfer)" -ForegroundColor Green
        }
    } catch { }
}

# Method 4: Invoke-WebRequest
if (-not $downloadSuccess) {
    try {
        Write-Host "      Trying WebRequest..." -ForegroundColor Gray
        $ProgressPreference = 'SilentlyContinue'
        Invoke-WebRequest -Uri $downloadUrl -OutFile $outputPath -UseBasicParsing -MaximumRedirection 10 -ErrorAction Stop
        if ((Test-Path $outputPath) -and (Get-Item $outputPath).Length -gt 1000000) {
            $downloadSuccess = $true
            Write-Host "      Download successful (WebRequest)" -ForegroundColor Green
        }
    } catch { }
}

if (-not $downloadSuccess) {
    Write-Host ""
    Write-Host "ERROR: All download methods failed." -ForegroundColor Red
    Write-Host ""
    Write-Host "Please download manually:" -ForegroundColor Yellow
    Write-Host "  $downloadUrl" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Save it to:" -ForegroundColor Yellow
    Write-Host "  $outputPath" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Then run this script again." -ForegroundColor Yellow
    exit 1
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
