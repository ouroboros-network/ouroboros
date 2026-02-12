# Ouroboros Node Installer (Windows)
# PowerShell script to install 'ouro' binary

$ErrorActionPreference = "Stop"

Write-Host "=== Ouroboros Node Installer ===" -ForegroundColor Cyan

# Configuration
$BinaryName = "ouro"
$InstallDir = "$env:USERPROFILE\.cargo\bin"
$ConfigDir = "$env:USERPROFILE\.ouroboros"

# 1. Check Prerequisites (Rust/Cargo)
if (-not (Get-Command "cargo" -ErrorAction SilentlyContinue)) {
    Write-Host "Error: Rust/Cargo is not installed." -ForegroundColor Red
    Write-Host "Please install Rust from https://rustup.rs/ and try again."
    exit 1
}

# 2. Build from Source
Write-Host "Building from source using Cargo..." -ForegroundColor Green
try {
    cargo build --release --bin $BinaryName
} catch {
    Write-Host "Build failed. Please check your Rust installation and dependencies." -ForegroundColor Red
    exit 1
}

# 3. Install Binary
if (-not (Test-Path -Path $InstallDir)) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

$SourcePath = "target\release\$BinaryName.exe"
$DestPath = "$InstallDir\$BinaryName.exe"

Write-Host "Installing to $DestPath..."
Copy-Item -Force -Path $SourcePath -Destination $DestPath

# 4. Initialize Configuration
Write-Host "Initializing configuration in $ConfigDir..."
if (-not (Test-Path -Path $ConfigDir)) {
    New-Item -ItemType Directory -Force -Path $ConfigDir | Out-Null
}

$ConfigFile = "$ConfigDir\config.json"
if (-not (Test-Path -Path $ConfigFile)) {
    # Run register-node to generate default config
    & $DestPath register-node | Out-Null
    Write-Host "Default configuration created." -ForegroundColor Yellow
}

Write-Host "=== Installation Complete! ===" -ForegroundColor Cyan
Write-Host "You can now start your node by running:"
Write-Host "  ouro start" -ForegroundColor Green
Write-Host ""
Write-Host "To see available commands:"
Write-Host "  ouro --help" -ForegroundColor Green
