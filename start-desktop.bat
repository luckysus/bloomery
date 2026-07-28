@echo off
setlocal

cd /d "%~dp0"

where npm >nul 2>nul
if errorlevel 1 (
  echo npm was not found. Please install Node.js first.
  pause
  exit /b 1
)

where cargo >nul 2>nul
if errorlevel 1 (
  echo cargo was not found. Please install Rust first.
  pause
  exit /b 1
)

cargo tauri --version >nul 2>nul
if errorlevel 1 (
  echo Installing Tauri CLI...
  cargo install tauri-cli --locked --version "^2"
  if errorlevel 1 (
    echo Tauri CLI install failed.
    pause
    exit /b 1
  )
)

if not exist "frontend\node_modules" (
  echo Installing frontend dependencies...
  pushd frontend
  call npm install
  if errorlevel 1 (
    popd
    echo npm install failed.
    pause
    exit /b 1
  )
  popd
)

echo Starting Bloomery...
pushd src-tauri
cargo tauri dev
set EXIT_CODE=%ERRORLEVEL%
popd

if not "%EXIT_CODE%"=="0" (
  echo Bloomery exited with code %EXIT_CODE%.
  pause
)

exit /b %EXIT_CODE%
