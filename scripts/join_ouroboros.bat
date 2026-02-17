@echo off
setlocal enabledelayedexpansion

echo ==========================================
echo   Ouroboros Network - Quick Join
echo ==========================================
echo.

set "INSTALL_DIR=%USERPROFILE%\.ouroboros"
set "BINARY=%INSTALL_DIR%\ouro-bin.exe"
set "DOWNLOAD_URL=https://github.com/ouroboros-network/ouroboros/releases/latest/download/ouro-windows-x64.exe"

:: Create install directory
if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"
if not exist "%INSTALL_DIR%\data" mkdir "%INSTALL_DIR%\data"

echo [1/4] Downloading Ouroboros node...

:: Try bitsadmin (available on all Windows versions)
echo       Using Windows BITS...
bitsadmin /transfer "OuroborosDownload" /download /priority high "%DOWNLOAD_URL%" "%BINARY%" >nul 2>&1
if exist "%BINARY%" (
    for %%A in ("%BINARY%") do if %%~zA GEQ 1000000 (
        echo       Download successful!
        goto :download_done
    )
)

:: Fallback: certutil (also built into Windows)
echo       Trying certutil...
certutil -urlcache -split -f "%DOWNLOAD_URL%" "%BINARY%" >nul 2>&1
if exist "%BINARY%" (
    for %%A in ("%BINARY%") do if %%~zA GEQ 1000000 (
        echo       Download successful!
        goto :download_done
    )
)

:: Fallback: PowerShell (if available)
echo       Trying PowerShell...
powershell -ExecutionPolicy Bypass -Command "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; (New-Object Net.WebClient).DownloadFile('%DOWNLOAD_URL%', '%BINARY%')" >nul 2>&1
if exist "%BINARY%" (
    for %%A in ("%BINARY%") do if %%~zA GEQ 1000000 (
        echo       Download successful!
        goto :download_done
    )
)

echo.
echo ERROR: Download failed. Please download manually from:
echo   %DOWNLOAD_URL%
echo.
echo Save it to: %BINARY%
echo.
pause
exit /b 1

:download_done
echo.

:: Stop existing node
echo [2/4] Checking for existing node...
taskkill /F /IM ouro-bin.exe >nul 2>&1
if exist "%INSTALL_DIR%\data\LOCK" del /F "%INSTALL_DIR%\data\LOCK" >nul 2>&1
echo       OK

:: Create config if needed
echo [3/4] Configuring node...
set "ENV_FILE=%INSTALL_DIR%\.env"

if not exist "%ENV_FILE%" (
    :: Generate random values using PowerShell
    for /f %%i in ('powershell -Command "[guid]::NewGuid().ToString('N').Substring(0,16)"') do set "NODE_ID=node-%%i"
    for /f %%i in ('powershell -Command "[guid]::NewGuid().ToString('N')"') do set "API_KEY=%%i"
    for /f %%i in ('powershell -Command "1..64 | ForEach-Object { '{0:x}' -f (Get-Random -Maximum 16) } | Join-String"') do set "BFT_SECRET=%%i"

    (
        echo # Ouroboros Node Configuration
        echo DATABASE_PATH=%INSTALL_DIR%\data
        echo API_ADDR=0.0.0.0:8000
        echo LISTEN_ADDR=0.0.0.0:9000
        echo PEER_ADDRS=136.112.101.176:9000
        echo NODE_ID=!NODE_ID!
        echo BFT_SECRET_SEED=!BFT_SECRET!
        echo API_KEYS=!API_KEY!
        echo RUST_LOG=info
    ) > "%ENV_FILE%"
    echo       Created new configuration
) else (
    echo       Using existing configuration
)

:: Create helper scripts
echo [4/4] Creating helper scripts...

(
    echo @echo off
    echo cd /d "%INSTALL_DIR%"
    echo for /f "usebackq eol=# tokens=1,* delims==" %%%%a in ^(".env"^) do set "%%%%a=%%%%b"
    echo "%BINARY%" join
) > "%INSTALL_DIR%\start-node.bat"

(
    echo @echo off
    echo cd /d "%INSTALL_DIR%"
    echo for /f "usebackq eol=# tokens=1,* delims==" %%%%a in ^(".env"^) do set "%%%%a=%%%%b"
    echo "%BINARY%" status
) > "%INSTALL_DIR%\status.bat"

(
    echo @echo off
    echo taskkill /F /IM ouro-bin.exe 2^>nul
    echo if %%ERRORLEVEL%% EQU 0 ^(echo Node stopped.^) else ^(echo No node running.^)
) > "%INSTALL_DIR%\stop-node.bat"

echo       OK
echo.

:: Add to PATH
set "PATH=%INSTALL_DIR%;%PATH%"

:: Load environment and start
echo Starting Ouroboros node...
cd /d "%INSTALL_DIR%"

:: Create a temporary startup script that loads env and runs the node
(
    echo @echo off
    echo cd /d "%INSTALL_DIR%"
    echo for /f "usebackq eol=# tokens=1,* delims==" %%%%a in ^(".env"^) do set "%%%%a=%%%%b"
    echo "%BINARY%" join
) > "%INSTALL_DIR%\run-node.bat"

:: Start node in background with environment loaded
start "Ouroboros Node" cmd /c "%INSTALL_DIR%\run-node.bat"

timeout /t 5 /nobreak >nul

:: Check if running
tasklist /FI "IMAGENAME eq ouro-bin.exe" 2>nul | find /I "ouro-bin.exe" >nul
if %ERRORLEVEL% EQU 0 (
    echo.
    echo ==========================================
    echo   Node started successfully!
    echo ==========================================
    echo.
    echo   API: http://localhost:8000
    echo   Data: %INSTALL_DIR%\data
    echo.
    echo Commands:
    echo   %INSTALL_DIR%\status.bat     - View status
    echo   %INSTALL_DIR%\stop-node.bat  - Stop node
    echo   %INSTALL_DIR%\start-node.bat - Start node
    echo.
    echo You're now part of the Ouroboros network!
    echo ==========================================
) else (
    echo.
    echo ERROR: Node failed to start.
    echo Try running manually: "%BINARY%" join
)

echo.
pause
