@echo off
setlocal
pushd "%~dp0"

rem Install dependencies (uses npm.cmd to avoid PS script policy)
cmd /c npm install
if errorlevel 1 (
  echo npm install failed. Check Node/npm installation.
  pause
  goto :eof
)

rem Start desktop app (Tauri dev)
cmd /c npm run tauri:dev
pause

popd
endlocal