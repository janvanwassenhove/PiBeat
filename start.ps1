# PiBeat Development Startup Script
Set-Location $PSScriptRoot

# Check if SC bundle exists, run setup if not
$scsynth = Join-Path (Join-Path (Join-Path $PSScriptRoot "src-tauri") "sc-bundle") "scsynth.exe"
$synthdef = Join-Path (Join-Path (Join-Path (Join-Path $PSScriptRoot "src-tauri") "sc-bundle") "synthdefs") "sonic_beep.scsyndef"

if (-not (Test-Path $scsynth)) {
    Write-Host "[SC] SuperCollider bundle not found, running setup..." -ForegroundColor Yellow
    try {
        & (Join-Path $PSScriptRoot "setup_sc.ps1")
    } catch {
        Write-Host "[SC] Setup failed: $($_.Exception.Message) - continuing without SuperCollider" -ForegroundColor Red
    }
}

if ((Test-Path $scsynth) -and -not (Test-Path $synthdef)) {
    Write-Host "[SC] SynthDefs not compiled, running compilation..." -ForegroundColor Yellow
    try {
        & (Join-Path $PSScriptRoot "compile_synthdefs.ps1")
    } catch {
        Write-Host "[SC] SynthDef compilation failed: $($_.Exception.Message)" -ForegroundColor Red
    }
}

Write-Host "Starting PiBeat in development mode..." -ForegroundColor Green
npm run tauri dev
