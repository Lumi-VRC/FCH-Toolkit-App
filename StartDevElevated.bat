@echo off
setlocal EnableExtensions

:: Elevate if not running as Administrator
whoami /groups | find "S-1-5-32-544" >nul 2>&1
if errorlevel 1 (
	echo Requesting administrative privileges...
	powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
	exit /b
)

pushd "%~dp0"

set "SCRIPT_DIR=%CD%"
set "PACKAGE_JSON=%SCRIPT_DIR%\package.json"

if not exist "%PACKAGE_JSON%" (
    echo ERROR: package.json not found at "%PACKAGE_JSON%".
    echo This script expects to run from the project root: FCH-Toolkit-App\
    echo Please double-check the workspace path or update StartDevElevated
    pause
    popd
    exit /b 1
)

if exist "%SCRIPT_DIR%\node_modules" (
    echo INFO: Detected node_modules directory.
) else (
    echo INFO: node_modules not found. Installing dependencies...
    call npm install
    if errorlevel 1 (
        echo ERROR: npm install failed.
        pause
        popd
        exit /b 1
    )
)

:: Ensure npm is available
where npm >nul 2>&1
if errorlevel 1 (
    echo npm not found in PATH. Please install Node.js and ensure npm is in PATH.
    pause
    popd
    exit /b 1
)

:: Launch elevated PowerShell, set working directory, and start the dev server
start "FCH Client Dev" powershell -NoProfile -ExecutionPolicy Bypass -NoExit -Command "Set-Location -LiteralPath '%SCRIPT_DIR%'; npm run tauri dev"

popd
endlocal
