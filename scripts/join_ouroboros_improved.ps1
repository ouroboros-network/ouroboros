# Improved Ouroboros Quick Join Script
param([switch]$SkipAutoStart)

$nodeDir = "$env:USERPROFILE\.ouroboros"
$dataDir = "$nodeDir\data"

Write-Host "`n=========================================="
Write-Host "  🌟 WELCOME TO OUROBOROS NETWORK 🌟"
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "`nSetting up your validator node..."

# Create directories
New-Item -ItemType Directory -Force -Path $nodeDir, $dataDir | Out-Null

# Get binary
Write-Host "`n[1/5] 📥 Getting node binary..." -NoNewline
if (Test-Path "$nodeDir\ouro-node.exe") {
    Write-Host " ✅ (existing)" -ForegroundColor Green
} else {
    $localBuild = "C:\Users\LENOVO\Desktop\ouroboros\ouro_dag\target\release\ouro-node.exe"
    if (Test-Path $localBuild) {
        Copy-Item $localBuild "$nodeDir\ouro-node.exe"
        Write-Host " ✅ (local)" -ForegroundColor Green
    } else {
        Write-Host " ❌ Not found" -ForegroundColor Red
        Write-Host "`nPlease build first: cd ouroboros\ouro_dag && cargo build --release"
        exit 1
    }
}

# Create wallet
Write-Host "[2/5] 🔐 Creating wallet..." -NoNewline
$walletAddr = -join ((48..57) + (97..102) | Get-Random -Count 40 | % {[char]$_})
$walletAddr = "0x$walletAddr"
$nodeId = "ouro_" + (-join ((97..122) + (48..57) | Get-Random -Count 12 | % {[char]$_}))
Set-Content "$nodeDir\wallet.txt" $walletAddr
Set-Content "$nodeDir\node_id.txt" $nodeId
Write-Host " ✅" -ForegroundColor Green

# Create config
Write-Host "[3/5] ⚙️  Configuring node..." -NoNewline
@"
ROCKSDB_PATH=$dataDir
STORAGE_MODE=full
RUST_LOG=info
API_ADDR=0.0.0.0:8000
LISTEN_ADDR=0.0.0.0:9000
NODE_ID=$nodeId
SEED_NODES=136.112.101.176:9000
"@ | Set-Content "$nodeDir\.env"
Write-Host " ✅" -ForegroundColor Green

# Create start script
@"
@echo off
cd /d %USERPROFILE%\.ouroboros
start /min ouro-node.exe start
"@ | Set-Content "$nodeDir\start-node.bat"

# Create ouro CLI (PowerShell)
@'
$nodeDir = "$env:USERPROFILE\.ouroboros"
cd $nodeDir

switch ($args[0]) {
    "status" {
        Write-Host "`n=========================================="
        Write-Host "   OUROBOROS NODE STATUS"
        Write-Host "=========================================="
        $nodeId = if (Test-Path "node_id.txt") { Get-Content "node_id.txt" } else { "Unknown" }
        $wallet = if (Test-Path "wallet.txt") { Get-Content "wallet.txt" } else { "Unknown" }
        $running = Get-Process ouro-node -ErrorAction SilentlyContinue
        if ($running) {
            Write-Host "Status: RUNNING (PID: $($running.Id))" -ForegroundColor Green
        } else {
            Write-Host "Status: STOPPED" -ForegroundColor Red
            Write-Host "Run 'ouro start' to start the node" -ForegroundColor Yellow
        }
        Write-Host "Node ID: $nodeId"
        Write-Host "Wallet: $wallet"
        Write-Host ""
        try {
            $health = Invoke-WebRequest -Uri "http://localhost:8000/health" -UseBasicParsing -TimeoutSec 2 -ErrorAction Stop
            Write-Host "API: http://localhost:8000" -ForegroundColor Green
            Write-Host ""
            Write-Host "Check rewards: ouro rewards"
            Write-Host "Wallet balance: ouro wallet balance"
        } catch {
            Write-Host "API: Offline" -ForegroundColor Red
            if ($running) {
                Write-Host "Node is running but API not responding. Check logs: ouro logs" -ForegroundColor Yellow
            }
        }
        Write-Host ""
        Write-Host "Commands: ouro start|stop|logs|wallet|rewards"
        Write-Host "=========================================="
    }
    "start" {
        Write-Host "Starting Ouroboros node..."
        if (-not (Test-Path "$nodeDir\ouro-node.exe")) {
            Write-Host "Error: ouro-node.exe not found in $nodeDir" -ForegroundColor Red
            return
        }
        if (-not (Test-Path "$nodeDir\.env")) {
            Write-Host "Error: .env config not found in $nodeDir" -ForegroundColor Red
            return
        }
        $proc = Start-Process -FilePath "$nodeDir\ouro-node.exe" -ArgumentList "start" -WorkingDirectory $nodeDir -PassThru
        Write-Host "Started process $($proc.Id), waiting 5s for API..."
        Start-Sleep 5
        & $PSCommandPath status
    }
    "stop" {
        Write-Host "Stopping Ouroboros node..."
        Get-Process ouro-node -ErrorAction SilentlyContinue | Stop-Process -Force
        Write-Host "Node stopped"
    }
    "wallet" {
        if ($args[1] -eq "balance") {
            $wallet = if (Test-Path "wallet.txt") { Get-Content "wallet.txt" } else { "Not created" }
            Write-Host "Checking balance for $wallet..."
            try {
                $balance = Invoke-WebRequest -Uri "http://localhost:8000/balance/$wallet" -UseBasicParsing -TimeoutSec 5
                Write-Host $balance.Content
            } catch {
                Write-Host "Error: API offline or unreachable" -ForegroundColor Red
            }
        } else {
            $wallet = if (Test-Path "wallet.txt") { Get-Content "wallet.txt" } else { "Not created" }
            Write-Host "Your Wallet: $wallet"
            Write-Host ""
            Write-Host "Commands:"
            Write-Host "  ouro wallet balance  - Check balance"
        }
    }
    "rewards" {
        $nodeId = if (Test-Path "node_id.txt") { Get-Content "node_id.txt" } else { "Unknown" }
        Write-Host "Fetching rewards for $nodeId..."
        try {
            $rewards = Invoke-WebRequest -Uri "http://localhost:8000/metrics/$nodeId" -UseBasicParsing -TimeoutSec 5
            Write-Host $rewards.Content
        } catch {
            Write-Host "Error: API offline or unreachable" -ForegroundColor Red
        }
    }
    "logs" {
        if (Test-Path "node.log") {
            Write-Host "Last 30 lines of node.log:" -ForegroundColor Cyan
            Get-Content "node.log" -Tail 30
        } else {
            $running = Get-Process ouro-node -ErrorAction SilentlyContinue
            if ($running) {
                Write-Host "Node is running (PID: $($running.Id)) but no log file found" -ForegroundColor Yellow
                Write-Host "Process started: $($running.StartTime)"
            } else {
                Write-Host "No log file found and node is not running" -ForegroundColor Red
            }
        }
    }
    default {
        Write-Host ""
        Write-Host "Ouroboros Node CLI"
        Write-Host ""
        Write-Host "Usage: ouro [command]"
        Write-Host ""
        Write-Host "Commands:"
        Write-Host "  status   - Show node status"
        Write-Host "  start    - Start node"
        Write-Host "  stop     - Stop node"
        Write-Host "  logs     - View recent logs"
        Write-Host "  wallet   - Show wallet address"
        Write-Host "  rewards  - Check earned rewards"
        Write-Host ""
    }
}
'@ | Set-Content "$nodeDir\ouro.ps1"

# Create batch wrapper
@"
@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%USERPROFILE%\.ouroboros\ouro.ps1" %*
"@ | Set-Content "$nodeDir\ouro.bat"

# Add to PATH if not already there
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$nodeDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$nodeDir", "User")
}

# Auto-start on boot
Write-Host "[4/5] 🚀 Configuring auto-start..." -NoNewline
try {
    $action = New-ScheduledTaskAction -Execute "$nodeDir\start-node.bat"
    $trigger = New-ScheduledTaskTrigger -AtStartup -RandomDelay (New-TimeSpan -Seconds 30)
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    Register-ScheduledTask -TaskName "OuroborosNode" -Action $action -Trigger $trigger -Settings $settings -Force -ErrorAction Stop | Out-Null
    Write-Host " ✅" -ForegroundColor Green
} catch {
    Write-Host " ⏭️  Skipped (needs admin)" -ForegroundColor Yellow
}

# Start node
Write-Host "[5/5] 🌐 Starting node..." -NoNewline
Start-Process -FilePath "$nodeDir\ouro-node.exe" -ArgumentList "start" -WindowStyle Hidden -WorkingDirectory $nodeDir
Start-Sleep 3
Write-Host " ✅`n" -ForegroundColor Green

# Success message
Write-Host "==========================================" -ForegroundColor Green
Write-Host "  🎉 SUCCESS! You're now validating!" -ForegroundColor Green
Write-Host "==========================================" -ForegroundColor Green
Write-Host "`n📊 Your Node:" -ForegroundColor Cyan
Write-Host "   Node ID: $nodeId"
Write-Host "   Wallet:  $walletAddr"
Write-Host "   Status:  http://localhost:8000/health"
Write-Host "`n💰 Earnings:" -ForegroundColor Yellow
Write-Host "   ~4.5 OURO/hour (based on uptime + validations)"
Write-Host "   Check rewards: ouro rewards"
Write-Host "`n📝 Wallet saved to: $nodeDir\wallet.txt"
Write-Host "   ⚠️  Backup this file - you'll need it to recover funds!"
Write-Host "`n🎯 Quick Commands:" -ForegroundColor Cyan
Write-Host "   $nodeDir\ouro status   - Live node status"
Write-Host "   $nodeDir\ouro wallet   - Your wallet address"
Write-Host "   $nodeDir\ouro rewards  - Check earnings"
Write-Host "`n⚠️  Restart terminal, then use: ouro status"
Write-Host "`n🌐 Your node will auto-start with Windows"
Write-Host "   Keep your PC online to maximize rewards!`n"
