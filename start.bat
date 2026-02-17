@echo off
cd /d "%~dp0"

REM Check if SC bundle exists, run setup if not
if not exist "src-tauri\sc-bundle\scsynth.exe" (
    echo [SC] SuperCollider bundle not found, running setup...
    powershell -ExecutionPolicy Bypass -File "%~dp0setup_sc.ps1"
    if errorlevel 1 (
        echo [SC] Setup failed, continuing without SuperCollider
    )
)

REM Check if SynthDefs are compiled
if exist "src-tauri\sc-bundle\scsynth.exe" (
    if not exist "src-tauri\sc-bundle\synthdefs\sonic_beep.scsyndef" (
        echo [SC] SynthDefs not compiled, running compilation...
        powershell -ExecutionPolicy Bypass -File "%~dp0compile_synthdefs.ps1"
    )
)

echo Starting PiBeat in development mode...
npm run tauri dev
