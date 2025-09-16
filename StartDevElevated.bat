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

:: Ensure npm is available
where npm >nul 2>&1
if errorlevel 1 (
	echo npm not found in PATH. Please install Node.js and ensure npm is in PATH.
	pause
	popd
	exit /b 1
)

:: Launch elevated PowerShell, set working directory, and start the dev server
start "FCH Client Dev" powershell -NoProfile -ExecutionPolicy Bypass -NoExit -Command "Set-Location -LiteralPath '%CD%'; npm run tauri dev"

popd
endlocal
