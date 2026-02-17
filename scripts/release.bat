@echo off
setlocal enabledelayedexpansion

:: Build locally and upload Windows binary to a GitHub release.
:: Usage: scripts\release.bat v1.5.2

if "%~1"=="" (
    echo Usage: release.bat ^<tag^>  ^(e.g. v1.5.2^)
    exit /b 1
)

set "TAG=%~1"
set "REPO=ouroboros-network/ouroboros"
set "SCRIPT_DIR=%~dp0"
set "ROOT_DIR=%SCRIPT_DIR%.."

echo === Building release binary for %TAG% ===

cd /d "%ROOT_DIR%\ouro_dag"
cargo build --release --bin ouro
if %ERRORLEVEL% NEQ 0 (
    echo Build failed!
    exit /b 1
)

:: Copy binary
set "DIST_DIR=%ROOT_DIR%\dist"
if not exist "%DIST_DIR%" mkdir "%DIST_DIR%"
copy /Y target\release\ouro.exe "%DIST_DIR%\ouro-windows-x64.exe" >nul

echo === Built: %DIST_DIR%\ouro-windows-x64.exe ===

:: Create release if it doesn't exist
gh release view %TAG% --repo %REPO% >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo === Creating release %TAG% ===
    gh release create %TAG% --repo %REPO% --generate-notes
)

:: Upload binary
echo === Uploading ouro-windows-x64.exe to %TAG% ===
gh release upload %TAG% "%DIST_DIR%\ouro-windows-x64.exe" --repo %REPO% --clobber

echo.
echo === Done! ===
echo Release: https://github.com/%REPO%/releases/tag/%TAG%
echo.
echo To upload binaries from other platforms, run on each machine:
echo   gh release upload %TAG% ^<binary-file^> --repo %REPO% --clobber
