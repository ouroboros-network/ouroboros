# Ouroboros Network - Node Setup (Windows)
# Join the decentralized network

$ErrorActionPreference = "Continue"

Write-Host ""
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "  Ouroboros Network - Quick Join" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host ""

# --- Configuration ---
$repo = "ouroboros-network/ouroboros"
$installDir = "$env:USERPROFILE\.ouroboros"
$binaryPath = "$installDir\ouro-bin.exe"
$envFile = "$installDir\.env"
$defaultSeeds = "136.112.101.176:9000,34.57.121.217:9000"

# Force TLS 1.2
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# --- Step 1: Check for existing installation ---
Write-Host "[1/5] Checking for existing installation..." -ForegroundColor Yellow

$existingVersion = $null
if (Test-Path $binaryPath) {
    try {
        $versionOutput = & $binaryPath --version 2>&1
        if ($versionOutput -match "(\d+\.\d+\.\d+)") {
            $existingVersion = $matches[1]
            Write-Host "      Found existing: v$existingVersion" -ForegroundColor Gray
        }
    } catch {}
}

# Get latest release version from GitHub
$latestVersion = $null
$latestAssetUrl = $null
try {
    $ProgressPreference = 'SilentlyContinue'
    $releaseInfo = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -UseBasicParsing -TimeoutSec 10
    if ($releaseInfo.tag_name -match "v?(\d+\.\d+\.\d+)") {
        $latestVersion = $matches[1]
    }
    # Find the Windows x64 asset
    $asset = $releaseInfo.assets | Where-Object { $_.name -eq "ouro-windows-x64.exe" } | Select-Object -First 1
    if ($asset) {
        $latestAssetUrl = $asset.browser_download_url
    }
} catch {
    Write-Host "      Warning: Could not check latest version" -ForegroundColor Yellow
}

if ($latestVersion) {
    Write-Host "      Latest release: v$latestVersion" -ForegroundColor Gray
}

# Decide whether to download
$needsDownload = $true
if ($existingVersion -and $latestVersion -and ($existingVersion -eq $latestVersion)) {
    Write-Host "      Already up to date!" -ForegroundColor Green
    $needsDownload = $false
} elseif ($existingVersion -and $latestVersion) {
    Write-Host "      Upgrading v$existingVersion -> v$latestVersion" -ForegroundColor Cyan
}

# --- Step 2: Stop existing node if running ---
Write-Host "[2/5] Checking for running node..." -ForegroundColor Yellow
$existingProcess = Get-Process -Name "ouro-bin" -ErrorAction SilentlyContinue
if ($existingProcess) {
    $nodePid = $existingProcess.Id
    Write-Host "      Stopping existing node (PID: $nodePid)..." -ForegroundColor Gray

    $stopped = $false

    # Method 1: Graceful API shutdown (works without admin privileges)
    if (Test-Path $envFile) {
        $apiKey = $null
        $apiAddr = "127.0.0.1:8000"
        Get-Content $envFile | ForEach-Object {
            if ($_ -match "^API_KEYS=(.+)$") { $apiKey = ($matches[1].Trim() -split ",")[0] }
            if ($_ -match "^API_ADDR=(.+)$") { $apiAddr = $matches[1].Trim().Replace("0.0.0.0", "127.0.0.1") }
        }
        if ($apiKey) {
            try {
                $ProgressPreference = 'SilentlyContinue'
                $headers = @{ "Authorization" = "Bearer $apiKey" }
                Invoke-RestMethod -Uri "http://$apiAddr/shutdown" -Method POST -Headers $headers -TimeoutSec 5 -ErrorAction Stop | Out-Null
                Write-Host "      Graceful shutdown requested." -ForegroundColor Gray
                Start-Sleep -Seconds 3
                if (-not (Get-Process -Id $nodePid -ErrorAction SilentlyContinue)) { $stopped = $true }
            } catch {}
        }
    }

    # Method 2: Stop-Process
    if (-not $stopped) {
        try {
            Stop-Process -Id $nodePid -Force -ErrorAction Stop
            Start-Sleep -Seconds 2
            if (-not (Get-Process -Id $nodePid -ErrorAction SilentlyContinue)) { $stopped = $true }
        } catch {}
    }

    # Method 3: taskkill
    if (-not $stopped) {
        Write-Host "      Trying taskkill..." -ForegroundColor Yellow
        & "$env:SystemRoot\System32\taskkill.exe" /F /PID $nodePid 2>$null | Out-Null
        Start-Sleep -Seconds 2
        if (-not (Get-Process -Id $nodePid -ErrorAction SilentlyContinue)) { $stopped = $true }
    }

    if ($stopped) {
        Write-Host "      Stopped." -ForegroundColor Gray
    } else {
        Write-Host ""
        Write-Host "ERROR: Could not stop the running node (PID: $nodePid)." -ForegroundColor Red
        Write-Host "       Please stop it manually or run this script as Administrator:" -ForegroundColor Yellow
        Write-Host "         taskkill /F /PID $nodePid" -ForegroundColor Cyan
        Write-Host "       Then re-run this installer." -ForegroundColor Yellow
        Write-Host ""
        return
    }
} else {
    Write-Host "      No running node found." -ForegroundColor Gray
}

# Remove stale lock file
$lockFile = "$installDir\data\LOCK"
if (Test-Path $lockFile) {
    Remove-Item -Force $lockFile -ErrorAction SilentlyContinue
}

# --- Step 3: Download binary ---
Write-Host "[3/5] Downloading Ouroboros node..." -ForegroundColor Yellow

# Create install directory
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
New-Item -ItemType Directory -Force -Path "$installDir\data" | Out-Null

if ($needsDownload) {
    # Remove old binary before downloading new one
    if (Test-Path $binaryPath) {
        Remove-Item -Force $binaryPath -ErrorAction SilentlyContinue
        if (Test-Path $binaryPath) {
            Write-Host ""
            Write-Host "ERROR: Cannot replace the existing binary (file is locked)." -ForegroundColor Red
            Write-Host "       Stop the running node first, then re-run this installer." -ForegroundColor Yellow
            Write-Host ""
            return
        }
        Write-Host "      Removed outdated binary." -ForegroundColor Gray
    }

    # Also clean up old-named binaries from previous versions
    $oldBinaries = @("$installDir\ouro_dag.exe", "$installDir\ouro_dag-windows-x64.exe")
    foreach ($old in $oldBinaries) {
        if (Test-Path $old) {
            Remove-Item -Force $old -ErrorAction SilentlyContinue
            Write-Host "      Removed legacy binary: $(Split-Path $old -Leaf)" -ForegroundColor Gray
        }
    }

    if (-not $latestAssetUrl) {
        $latestAssetUrl = "https://github.com/$repo/releases/latest/download/ouro-windows-x64.exe"
    }

    $downloadSuccess = $false

    # Method 1: Invoke-WebRequest (most reliable in modern PowerShell)
    try {
        Write-Host "      Downloading from GitHub releases..." -ForegroundColor Gray
        $ProgressPreference = 'SilentlyContinue'
        Invoke-WebRequest -Uri $latestAssetUrl -OutFile $binaryPath -UseBasicParsing -MaximumRedirection 10 -ErrorAction Stop
        if ((Test-Path $binaryPath) -and (Get-Item $binaryPath).Length -gt 1000000) {
            $downloadSuccess = $true
            $sizeMB = [math]::Round((Get-Item $binaryPath).Length / 1MB, 1)
            Write-Host "      Downloaded successfully ($($sizeMB) MB)" -ForegroundColor Green
        }
    } catch {}

    # Method 2: Start-BitsTransfer
    if (-not $downloadSuccess) {
        try {
            Write-Host "      Trying BitsTransfer..." -ForegroundColor Gray
            Import-Module BitsTransfer -ErrorAction SilentlyContinue
            Start-BitsTransfer -Source $latestAssetUrl -Destination $binaryPath -ErrorAction Stop
            if ((Test-Path $binaryPath) -and (Get-Item $binaryPath).Length -gt 1000000) {
                $downloadSuccess = $true
                Write-Host "      Download successful (BitsTransfer)" -ForegroundColor Green
            }
        } catch {}
    }

    # Method 3: certutil
    if (-not $downloadSuccess) {
        try {
            Write-Host "      Trying certutil..." -ForegroundColor Gray
            $null = & certutil -urlcache -split -f $latestAssetUrl $binaryPath 2>&1
            if ((Test-Path $binaryPath) -and (Get-Item $binaryPath).Length -gt 1000000) {
                $downloadSuccess = $true
                Write-Host "      Download successful (certutil)" -ForegroundColor Green
            }
        } catch {}
    }

    if (-not $downloadSuccess) {
        Write-Host ""
        Write-Host "ERROR: All download methods failed." -ForegroundColor Red
        Write-Host ""
        Write-Host "Please download manually from:" -ForegroundColor Yellow
        Write-Host "  https://github.com/$repo/releases/latest" -ForegroundColor Cyan
        Write-Host ""
        Write-Host "Save 'ouro-windows-x64.exe' as:" -ForegroundColor Yellow
        Write-Host "  $binaryPath" -ForegroundColor Cyan
        Write-Host ""
        return
    }
} else {
    Write-Host "      Skipping download (already up to date)." -ForegroundColor Gray
}

# --- Step 3b: Download Python tier files (for Medium/Light roles) ---
$pyDir = "$installDir\ouro_py"
$pyFiles = @(
    @{ Remote = "ouro_py/requirements.txt"; Local = "$pyDir\requirements.txt" },
    @{ Remote = "ouro_py/ouro_medium/main.py"; Local = "$pyDir\ouro_medium\main.py" },
    @{ Remote = "ouro_py/ouro_light/main.py"; Local = "$pyDir\ouro_light\main.py" }
)
$rawBase = "https://raw.githubusercontent.com/$repo/main"

$pyNeedsUpdate = $needsDownload -or -not (Test-Path "$pyDir\ouro_medium\main.py")
if ($pyNeedsUpdate) {
    Write-Host "      Downloading Python tier files..." -ForegroundColor Gray
    foreach ($f in $pyFiles) {
        $dir = Split-Path $f.Local -Parent
        if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
        try {
            $ProgressPreference = 'SilentlyContinue'
            Invoke-WebRequest -Uri "$rawBase/$($f.Remote)" -OutFile $f.Local -UseBasicParsing -ErrorAction Stop
        } catch {
            Write-Host "      Warning: Could not download $($f.Remote)" -ForegroundColor Yellow
        }
    }
    Write-Host "      Python tier files installed." -ForegroundColor Gray
}

# --- Step 4: Configure node ---
Write-Host "[4/5] Configuring node..." -ForegroundColor Yellow

$seedNodes = if ($env:OUROBOROS_SEED) { $env:OUROBOROS_SEED } else { $defaultSeeds }

# Check if existing config is valid
$needsNewConfig = $true
if (Test-Path $envFile) {
    $envContent = Get-Content $envFile -Raw
    if ($envContent -match "API_KEYS=" -and $envContent -match "BFT_SECRET_SEED=") {
        Write-Host "      Using existing configuration." -ForegroundColor Gray
        $needsNewConfig = $false
    }
}

if ($needsNewConfig) {
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()

    $bftBytes = New-Object byte[] 32
    $rng.GetBytes($bftBytes)
    $bftSecret = [System.BitConverter]::ToString($bftBytes).Replace("-", "").ToLower()

    $idBytes = New-Object byte[] 8
    $rng.GetBytes($idBytes)
    $nodeId = "ouro_" + [System.BitConverter]::ToString($idBytes).Replace("-", "").ToLower()

    $keyBytes = New-Object byte[] 16
    $rng.GetBytes($keyBytes)
    $apiKey = "ouro_" + [System.BitConverter]::ToString($keyBytes).Replace("-", "").ToLower()

    Write-Host "      Generated node identity: $nodeId" -ForegroundColor Gray

    @"
# Ouroboros Node Configuration
ROCKSDB_PATH=$installDir\data
API_ADDR=0.0.0.0:8000
LISTEN_ADDR=0.0.0.0:9000
PEER_ADDRS=$seedNodes
NODE_ID=$nodeId
BFT_SECRET_SEED=$bftSecret
API_KEYS=$apiKey
RUST_LOG=info
STORAGE_MODE=rocksdb
"@ | Out-File -FilePath $envFile -Encoding ASCII
}

# Load env vars for current session
Get-Content $envFile | ForEach-Object {
    if ($_ -match "^([^#][^=]+)=(.*)$") {
        [Environment]::SetEnvironmentVariable($matches[1].Trim(), $matches[2].Trim(), "Process")
    }
}

Write-Host "      API: http://localhost:8000" -ForegroundColor Gray
Write-Host "      P2P: 0.0.0.0:9000" -ForegroundColor Gray

# --- Step 5: Scan for peers and start node ---
Write-Host "[5/5] Starting node..." -ForegroundColor Yellow

# Scan seed nodes for availability
$seeds = $seedNodes -split ","
$reachableSeeds = @()
foreach ($seed in $seeds) {
    $seedHost = ($seed.Trim() -split ":")[0]
    $seedPort = ($seed.Trim() -split ":")[1]
    try {
        $tcp = New-Object System.Net.Sockets.TcpClient
        $result = $tcp.BeginConnect($seedHost, [int]$seedPort, $null, $null)
        $wait = $result.AsyncWaitHandle.WaitOne(2000, $false)
        if ($wait -and $tcp.Connected) {
            $reachableSeeds += $seed.Trim()
            Write-Host "      Seed $seed - reachable" -ForegroundColor Green
        } else {
            Write-Host "      Seed $seed - unreachable" -ForegroundColor Gray
        }
        $tcp.Close()
    } catch {
        Write-Host "      Seed $seed - unreachable" -ForegroundColor Gray
    }
}

if ($reachableSeeds.Count -eq 0) {
    Write-Host "      No seeds reachable (will run as standalone node)" -ForegroundColor Yellow
} else {
    Write-Host "      Found $($reachableSeeds.Count) reachable seed(s)" -ForegroundColor Green
}

# Create helper scripts
@"
@echo off
cd /d "$installDir"
for /f "usebackq eol=# tokens=1,* delims==" %%a in (".env") do set "%%a=%%b"
ouro-bin.exe start
"@ | Out-File -FilePath "$installDir\start-node.bat" -Encoding ASCII

@"
@echo off
cd /d "$installDir"
for /f "usebackq eol=# tokens=1,* delims==" %%a in (".env") do set "%%a=%%b"
ouro-bin.exe status
"@ | Out-File -FilePath "$installDir\status.bat" -Encoding ASCII

@"
@echo off
echo Stopping Ouroboros node...
taskkill /IM ouro-bin.exe /F 2>nul
if %ERRORLEVEL% EQU 0 (echo Node stopped.) else (echo No running node found.)
"@ | Out-File -FilePath "$installDir\stop-node.bat" -Encoding ASCII

# Add to PATH
$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($currentPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$installDir;$currentPath", "User")
    $env:Path = "$installDir;$env:Path"
}

# Update stale cargo bin copy if it exists (cargo/bin/ouro.exe takes PATH priority)
$cargoBin = "$env:USERPROFILE\.cargo\bin\ouro.exe"
if (Test-Path $cargoBin) {
    try {
        Copy-Item -Path $binaryPath -Destination $cargoBin -Force
        Write-Host "      Updated $cargoBin" -ForegroundColor Gray
    } catch {
        Write-Host "      Warning: Could not update $cargoBin" -ForegroundColor Yellow
    }
}

# Start the node
$nodeProcess = Start-Process -FilePath "cmd.exe" -ArgumentList "/c", "`"$installDir\start-node.bat`"" -WorkingDirectory $installDir -PassThru -WindowStyle Hidden -RedirectStandardOutput "$installDir\node.log" -RedirectStandardError "$installDir\node_error.log"

Write-Host "      Node started (PID: $($nodeProcess.Id))" -ForegroundColor Gray

# Wait for node to initialize
Start-Sleep -Seconds 5

# Check if running
$ouroProcess = Get-Process -Name "ouro-bin" -ErrorAction SilentlyContinue

if ($ouroProcess) {
    # Use the version we already know (either latest download or existing)
    $displayVersion = if ($latestVersion) { "v$latestVersion" } elseif ($existingVersion) { "v$existingVersion" } else { "" }

    Write-Host ""
    Write-Host "==========================================" -ForegroundColor Green
    Write-Host "  Node started successfully! $displayVersion" -ForegroundColor Green
    Write-Host "==========================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "  API:  http://localhost:8000" -ForegroundColor Cyan
    Write-Host "  Data: $installDir\data" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Commands:" -ForegroundColor Yellow
    Write-Host "  ouro status     - View node dashboard" -ForegroundColor White
    Write-Host "  ouro peers      - List connected peers" -ForegroundColor White
    Write-Host "  ouro diagnose   - Run diagnostics" -ForegroundColor White
    Write-Host ""
    Write-Host "Management:" -ForegroundColor Yellow
    Write-Host "  $installDir\stop-node.bat   - Stop" -ForegroundColor White
    Write-Host "  $installDir\start-node.bat  - Start" -ForegroundColor White
    Write-Host ""
    Write-Host "You're now part of the Ouroboros network!" -ForegroundColor Green
    Write-Host "==========================================" -ForegroundColor Green
} else {
    Write-Host ""
    Write-Host "Warning: Node may have failed to start." -ForegroundColor Red
    Write-Host ""
    if (Test-Path "$installDir\node_error.log") {
        Write-Host "Error log:" -ForegroundColor Yellow
        Get-Content "$installDir\node_error.log" -Tail 10
    }
    Write-Host ""
    Write-Host "Try running manually: $installDir\start-node.bat" -ForegroundColor Cyan
}

Write-Host ""
