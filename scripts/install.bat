@echo off
setlocal enabledelayedexpansion
title Ouroboros Node Installer
chcp 65001 >nul 2>&1

echo.
echo  ==========================================
echo    Ouroboros Network - Quick Join
echo  ==========================================
echo.
echo  This installer does NOT require admin rights
echo  or PowerShell execution policy changes.
echo.

set "REPO=ouroboros-network/ouroboros"
set "INSTALL_DIR=%USERPROFILE%\.ouroboros"
set "BIN_PATH=%INSTALL_DIR%\ouro-bin.exe"
set "ENV_FILE=%INSTALL_DIR%\.env"
set "LOG_FILE=%INSTALL_DIR%\node.log"
set "DEFAULT_SEEDS=136.112.101.176:9000,34.57.121.217:9000"
set "DOWNLOAD_URL=https://github.com/%REPO%/releases/latest/download/ouro-windows-x64.exe"

:: ── Step 1: Create directories ─────────────────────────────────────
echo [1/4] Setting up install directory...
if not exist "%INSTALL_DIR%"       mkdir "%INSTALL_DIR%"
if not exist "%INSTALL_DIR%\data"  mkdir "%INSTALL_DIR%\data"

:: Stop any running node
tasklist /FI "IMAGENAME eq ouro-bin.exe" 2>nul | find /I "ouro-bin.exe" >nul 2>&1
if %ERRORLEVEL% EQU 0 (
    echo       Stopping running node...
    taskkill /IM ouro-bin.exe /F >nul 2>&1
    timeout /t 3 /nobreak >nul
)

:: Remove stale lock
if exist "%INSTALL_DIR%\data\LOCK" (
    del /F /Q "%INSTALL_DIR%\data\LOCK" >nul 2>&1
)

:: Remove old binary so we can replace it
if exist "%BIN_PATH%" (
    del /F /Q "%BIN_PATH%" >nul 2>&1
    if exist "%BIN_PATH%" (
        echo.
        echo  ERROR: Cannot replace existing binary ^(file is locked^).
        echo  Stop the running node first: taskkill /IM ouro-bin.exe /F
        echo.
        pause & exit /b 1
    )
)

:: ── Step 2: Download binary ────────────────────────────────────────
echo [2/4] Downloading Ouroboros node...
echo       From: %DOWNLOAD_URL%
echo.

set "DOWNLOAD_OK=0"

:: Method 1: PowerShell Invoke-WebRequest (handles GitHub redirects best)
powershell -ExecutionPolicy Bypass -NoProfile -Command ^
  "try { $p='SilentlyContinue'; $ProgressPreference=$p; Invoke-WebRequest -Uri '%DOWNLOAD_URL%' -OutFile '%BIN_PATH%' -UseBasicParsing -MaximumRedirection 10 -ErrorAction Stop; exit 0 } catch { exit 1 }" >nul 2>&1
if exist "%BIN_PATH%" (
    for %%F in ("%BIN_PATH%") do if %%~zF GTR 1000000 set "DOWNLOAD_OK=1"
)

:: Method 2: certutil (no PowerShell needed — built into every Windows)
if "%DOWNLOAD_OK%"=="0" (
    echo       Trying certutil...
    certutil -urlcache -split -f "%DOWNLOAD_URL%" "%BIN_PATH%" >nul 2>&1
    if exist "%BIN_PATH%" (
        for %%F in ("%BIN_PATH%") do if %%~zF GTR 1000000 set "DOWNLOAD_OK=1"
    )
)

:: Method 3: bitsadmin (legacy fallback)
if "%DOWNLOAD_OK%"=="0" (
    echo       Trying bitsadmin...
    bitsadmin /transfer "OuroDownload" /download /priority FOREGROUND "%DOWNLOAD_URL%" "%BIN_PATH%" >nul 2>&1
    if exist "%BIN_PATH%" (
        for %%F in ("%BIN_PATH%") do if %%~zF GTR 1000000 set "DOWNLOAD_OK=1"
    )
)

if "%DOWNLOAD_OK%"=="0" (
    echo.
    echo  ERROR: All download methods failed.
    echo.
    echo  Please download manually:
    echo    1. Open: https://github.com/%REPO%/releases/latest
    echo    2. Download: ouro-windows-x64.exe
    echo    3. Save as: %BIN_PATH%
    echo    4. Run this installer again.
    echo.
    pause & exit /b 1
)
echo       Downloaded successfully!

:: ── Step 3: Configure ─────────────────────────────────────────────
echo [3/4] Configuring node...

:: Reuse existing config if valid
set "NEEDS_CONFIG=1"
if exist "%ENV_FILE%" (
    findstr /C:"API_KEYS=" "%ENV_FILE%" >nul 2>&1
    if !ERRORLEVEL! EQU 0 (
        findstr /C:"BFT_SECRET_SEED=" "%ENV_FILE%" >nul 2>&1
        if !ERRORLEVEL! EQU 0 (
            echo       Using existing configuration.
            set "NEEDS_CONFIG=0"
        )
    )
)

if "%NEEDS_CONFIG%"=="1" (
    :: Generate random keys using PowerShell crypto
    for /f "delims=" %%a in ('powershell -ExecutionPolicy Bypass -NoProfile -Command ^
      "[System.BitConverter]::ToString([System.Security.Cryptography.RandomNumberGenerator]::GetBytes(32)).Replace('-','').ToLower()"') do set "BFT_SEED=%%a"

    for /f "delims=" %%a in ('powershell -ExecutionPolicy Bypass -NoProfile -Command ^
      "'ouro_' + [System.BitConverter]::ToString([System.Security.Cryptography.RandomNumberGenerator]::GetBytes(8)).Replace('-','').ToLower()"') do set "NODE_ID=%%a"

    for /f "delims=" %%a in ('powershell -ExecutionPolicy Bypass -NoProfile -Command ^
      "'ouro_' + [System.BitConverter]::ToString([System.Security.Cryptography.RandomNumberGenerator]::GetBytes(16)).Replace('-','').ToLower()"') do set "API_KEY=%%a"

    set "SEEDS=%DEFAULT_SEEDS%"
    if defined OUROBOROS_SEED set "SEEDS=!OUROBOROS_SEED!"

    (
        echo # Ouroboros Node Configuration
        echo ROCKSDB_PATH=%INSTALL_DIR%\data
        echo API_ADDR=0.0.0.0:8000
        echo LISTEN_ADDR=0.0.0.0:9000
        echo PEER_ADDRS=!SEEDS!
        echo NODE_ID=!NODE_ID!
        echo BFT_SECRET_SEED=!BFT_SEED!
        echo API_KEYS=!API_KEY!
        echo RUST_LOG=info
        echo STORAGE_MODE=rocksdb
    ) > "%ENV_FILE%"

    echo       Node ID: !NODE_ID!
    echo       API Key: !API_KEY!
)

:: Create helper bat scripts
(
    echo @echo off
    echo cd /d "%INSTALL_DIR%"
    echo for /f "usebackq eol=# tokens=1,* delims==" %%%%a in ^(".env"^) do set "%%%%a=%%%%b"
    echo ouro-bin.exe start
) > "%INSTALL_DIR%\start-node.bat"

(
    echo @echo off
    echo taskkill /IM ouro-bin.exe /F 2^>nul
    echo if %%ERRORLEVEL%% EQU 0 ^(echo Node stopped.^) else ^(echo No node running.^)
) > "%INSTALL_DIR%\stop-node.bat"

(
    echo @echo off
    echo cd /d "%INSTALL_DIR%"
    echo for /f "usebackq eol=# tokens=1,* delims==" %%%%a in ^(".env"^) do set "%%%%a=%%%%b"
    echo ouro-bin.exe status
) > "%INSTALL_DIR%\status.bat"

:: Add install dir to user PATH
for /f "tokens=2*" %%a in ('reg query "HKCU\Environment" /v Path 2^>nul') do set "CUR_PATH=%%b"
echo !CUR_PATH! | findstr /I "%INSTALL_DIR%" >nul 2>&1
if !ERRORLEVEL! NEQ 0 (
    setx PATH "%INSTALL_DIR%;!CUR_PATH!" >nul 2>&1
    set "PATH=%INSTALL_DIR%;%PATH%"
    echo       Added to PATH.
)

:: Update cargo bin if it exists (takes PATH priority over install dir)
set "CARGO_BIN=%USERPROFILE%\.cargo\bin\ouro.exe"
if exist "%CARGO_BIN%" (
    copy /Y "%BIN_PATH%" "%CARGO_BIN%" >nul 2>&1
)

:: ── Step 4: Start ─────────────────────────────────────────────────
echo [4/4] Starting node...

start "Ouroboros Node" /min cmd /c ^
  "cd /d "%INSTALL_DIR%" && for /f "usebackq eol=# tokens=1,* delims==" %%a in (".env") do set "%%a=%%b" && ouro-bin.exe start >> "%LOG_FILE%" 2>&1"

timeout /t 6 /nobreak >nul

:: Verify it started
tasklist /FI "IMAGENAME eq ouro-bin.exe" 2>nul | find /I "ouro-bin.exe" >nul 2>&1
if %ERRORLEVEL% EQU 0 (
    echo.
    echo  ==========================================
    echo    You're now on the Ouroboros Network!
    echo  ==========================================
    echo.
    echo    API:  http://localhost:8000
    echo    Data: %INSTALL_DIR%\data
    echo    Log:  %LOG_FILE%
    echo.
    echo  Management scripts:
    echo    %INSTALL_DIR%\start-node.bat
    echo    %INSTALL_DIR%\stop-node.bat
    echo    %INSTALL_DIR%\status.bat
    echo.
    echo  SDK:
    echo    npm install ouro-sdk
    echo    pip install ouro-sdk
    echo.
) else (
    echo.
    echo  WARNING: Node may have failed to start.
    echo.
    if exist "%LOG_FILE%" (
        echo  Last 5 log lines:
        powershell -ExecutionPolicy Bypass -NoProfile -Command "Get-Content '%LOG_FILE%' -Tail 5 -ErrorAction SilentlyContinue"
    )
    echo.
    echo  Try starting manually: %INSTALL_DIR%\start-node.bat
    echo.
)

echo  Press any key to close this window.
pause >nul
