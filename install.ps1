# Ouroboros Node Installer (Windows)
# Downloads precompiled binary from GitHub releases

$ErrorActionPreference = "Stop"

# Configuration
$Repo = "ouroboros-network/ouroboros"
$BinaryName = "ouro"
$AssetName = "ouro-windows-x64.exe"
$InstallDir = "$env:USERPROFILE\.cargo\bin"
$ConfigDir = "$env:USERPROFILE\.ouroboros"

Write-Host ""
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "  Ouroboros Network - Quick Install"
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host ""

# 1. Check for existing installation
Write-Host "[1/5] Checking for existing installation..." -ForegroundColor Green
$CurrentVersion = $null
$ExistingBin = Join-Path $InstallDir "$BinaryName.exe"
if (Test-Path $ExistingBin) {
    try {
        $VersionOutput = & $ExistingBin --version 2>&1
        if ($VersionOutput -match '(\d+\.\d+\.\d+)') {
            $CurrentVersion = $Matches[1]
            Write-Host "      Found existing: v$CurrentVersion"
        }
    } catch {}
}

# 2. Get latest release version
Write-Host "[2/5] Checking latest release..." -ForegroundColor Green
$LatestVersion = $null
try {
    $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ "User-Agent" = "Ouroboros-Installer" }
    $LatestVersion = $Release.tag_name -replace '^v', ''
    Write-Host "      Latest: v$LatestVersion"

    if ($CurrentVersion -eq $LatestVersion) {
        Write-Host "      Already up to date!" -ForegroundColor Green
    } elseif ($CurrentVersion) {
        Write-Host "      Upgrading v$CurrentVersion -> v$LatestVersion"
    }
} catch {
    Write-Host "      Could not check latest version, proceeding with download..." -ForegroundColor Yellow
}

# 3. Stop running node if upgrading
if ($CurrentVersion -and $CurrentVersion -ne $LatestVersion) {
    $NodeProc = Get-Process -Name $BinaryName -ErrorAction SilentlyContinue
    if ($NodeProc) {
        Write-Host "[2b]  Stopping existing node (PID: $($NodeProc.Id))..." -ForegroundColor Yellow
        try {
            Invoke-RestMethod -Uri "http://localhost:8000/shutdown" -Method Post -Headers @{ "Authorization" = "Bearer admin" } -TimeoutSec 5 2>$null
            Start-Sleep -Seconds 3
        } catch {}
        $NodeProc = Get-Process -Name $BinaryName -ErrorAction SilentlyContinue
        if ($NodeProc) { $NodeProc | Stop-Process -Force }
        Write-Host "      Stopped."
    }
}

# 4. Download binary
Write-Host "[3/5] Downloading $AssetName..." -ForegroundColor Green
$DownloadUrl = "https://github.com/$Repo/releases/latest/download/$AssetName"

if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

$DestPath = Join-Path $InstallDir "$BinaryName.exe"
$TempPath = Join-Path $env:USERPROFILE ".ouroboros-download.tmp"

try {
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $TempPath -UseBasicParsing
    $Size = [math]::Round((Get-Item $TempPath).Length / 1MB, 1)
    Write-Host "      Downloaded successfully ($Size MB)"
} catch {
    Write-Host "Download failed: $_" -ForegroundColor Red
    Write-Host "Check releases: https://github.com/$Repo/releases/latest"
    if (Test-Path $TempPath) { Remove-Item $TempPath -Force }
    exit 1
}

# 5. Install binary
Write-Host "[4/5] Installing..." -ForegroundColor Green
if (Test-Path $DestPath) {
    Remove-Item $DestPath -Force
}
Move-Item -Force $TempPath $DestPath
Write-Host "      Installed to $DestPath"

# 6. Configure
Write-Host "[5/5] Configuring node..." -ForegroundColor Green
if (-not (Test-Path $ConfigDir)) {
    New-Item -ItemType Directory -Force -Path $ConfigDir | Out-Null
}

$ConfigFile = Join-Path $ConfigDir "config.json"
if (-not (Test-Path $ConfigFile)) {
    & $DestPath register-node 2>$null | Out-Null
    Write-Host "      Created default configuration."
} else {
    Write-Host "      Using existing configuration."
}

# 7. Verify and print summary
$InstalledVersion = "unknown"
try {
    $VersionOutput = & $DestPath --version 2>&1
    if ($VersionOutput -match '(\d+\.\d+\.\d+)') {
        $InstalledVersion = $Matches[1]
    }
} catch {}

Write-Host ""
Write-Host "==========================================" -ForegroundColor Green
Write-Host "  Installation Complete!"
Write-Host "==========================================" -ForegroundColor Green
Write-Host ""
Write-Host "  Version:  v$InstalledVersion" -ForegroundColor Green
Write-Host "  Binary:   $DestPath"
Write-Host "  Config:   $ConfigDir\"
Write-Host ""
Write-Host "  Start your node:" -ForegroundColor Cyan
Write-Host "    ouro start                    # Heavy node (default)" -ForegroundColor White
Write-Host "    ouro start --role medium      # Subchain aggregator" -ForegroundColor DarkGray
Write-Host "    ouro start --role light       # App node / watchdog" -ForegroundColor DarkGray
Write-Host ""
Write-Host "  Useful commands:" -ForegroundColor Cyan
Write-Host "    ouro status                   # Live dashboard" -ForegroundColor White
Write-Host "    ouro roles                    # Tier details" -ForegroundColor DarkGray
Write-Host "    ouro benchmark                # Hardware benchmark" -ForegroundColor DarkGray
Write-Host "    ouro --help                   # All commands" -ForegroundColor DarkGray
Write-Host ""
