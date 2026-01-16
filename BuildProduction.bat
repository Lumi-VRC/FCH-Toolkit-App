@echo off
REM Build Production Executable for FCH App
REM This script builds a release version of the Tauri application

echo ========================================
echo FCH App - Production Build
echo ========================================
echo.

REM Check if we're in the correct directory
if not exist "package.json" (
    echo ERROR: package.json not found. Please run this script from the FCHApp directory.
    pause
    exit /b 1
)

if not exist "src-tauri\Cargo.toml" (
    echo ERROR: src-tauri\Cargo.toml not found. Please run this script from the FCHApp directory.
    pause
    exit /b 1
)

echo [1/2] Cleaning previous build artifacts...
if exist "build" rmdir /s /q "build"
if exist "src-tauri\target\release" (
    echo   Cleaning Rust release build...
    rmdir /s /q "src-tauri\target\release"
)

echo.
echo [2/2] Building production bundle (frontend + backend + installer)...
echo   This will build the Svelte frontend, Rust backend, and create the installer.
echo   This may take several minutes...
echo.
call npm run tauri build
if errorlevel 1 (
    echo ERROR: Production build failed!
    pause
    exit /b 1
)

echo.
echo ========================================
echo Build Complete!
echo ========================================
echo.
echo Output location: src-tauri\target\release\bundle\
echo.
echo The installer executables should be in the bundle directory.
echo.
pause
