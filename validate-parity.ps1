# PiBeat Parity Validation Script
# Run from project root: .\validate-parity.ps1

param(
    [switch]$Full,        # Run full test suite
    [switch]$Parsing,     # Run parsing tests only
    [switch]$Audio,       # Run audio comparison only
    [switch]$Snapshots,   # Run snapshot tests only
    [switch]$Examples,    # Run example parsing tests only
    [switch]$Quick,       # Quick validation (lib tests only)
    [switch]$Verbose,     # Show detailed output
    [string]$Fixture      # Run tests for a specific fixture
)

$ErrorActionPreference = "Continue"
$startTime = Get-Date

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  PiBeat Parity Validation Suite" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

Push-Location "$PSScriptRoot/src-tauri"

function Write-Section($title) {
    Write-Host ""
    Write-Host "--- $title ---" -ForegroundColor Yellow
}

function Run-Test($name, $command, $pattern) {
    Write-Host "Running: $name" -ForegroundColor Gray
    $output = Invoke-Expression $command 2>&1
    
    if ($Verbose) {
        $output | ForEach-Object { Write-Host $_ }
    }
    
    $result = $output | Select-String $pattern
    if ($result) {
        Write-Host $result -ForegroundColor $(if ($result -match "FAILED") { "Red" } else { "Green" })
    }
    
    return $output
}

# Quick validation (default if no flags)
if (-not $Full -and -not $Parsing -and -not $Audio -and -not $Snapshots -and -not $Examples -and -not $Fixture) {
    $Quick = $true
}

# Check if we're testing a specific fixture
if ($Fixture) {
    Write-Section "Testing Fixture: $Fixture"
    
    if ($Verbose) {
        $env:RUST_LOG = "debug"
    }
    
    $testName = "snapshot_$($Fixture -replace '-', '_')"
    $output = cargo test $testName -- --nocapture 2>&1
    
    if ($Verbose) {
        $output | ForEach-Object { Write-Host $_ }
    }
    
    $result = $output | Select-String "test result|FAILED|passed|ok"
    $result | ForEach-Object { 
        Write-Host $_ -ForegroundColor $(if ($_ -match "FAILED") { "Red" } else { "Green" })
    }
    
    Pop-Location
    exit
}

# Full test suite
if ($Full -or $Quick -or $Parsing) {
    Write-Section "Library Tests (Parser + Core)"
    Run-Test "lib tests" "cargo test --lib 2>&1" "test result"
}

if ($Full -or $Snapshots) {
    Write-Section "Fidelity Snapshot Tests"
    Run-Test "fidelity snapshots" "cargo test --test fidelity_snapshots 2>&1" "test result|FAILED"
}

if ($Full -or $Audio) {
    Write-Section "Audio Comparison Tests"
    Run-Test "audio compare" "cargo test --test audio_compare 2>&1" "test result|FAILED"
}

if ($Full -or $Examples) {
    Write-Section "Example Parsing Tests"
    Run-Test "example parsing" "cargo test --test example_parsing 2>&1" "test result|FAILED"
}

Pop-Location

# TypeScript check
if ($Full) {
    Write-Section "TypeScript Compilation Check"
    $tsOutput = npx tsc --noEmit 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Host "TypeScript: OK" -ForegroundColor Green
    } else {
        Write-Host "TypeScript errors:" -ForegroundColor Red
        $tsOutput | Select-Object -First 20 | ForEach-Object { Write-Host $_ }
    }
}

# Summary
$elapsed = (Get-Date) - $startTime
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Validation Complete" -ForegroundColor Cyan
Write-Host "  Time: $($elapsed.TotalSeconds.ToString('F1'))s" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# Quick reference
Write-Host ""
Write-Host "Usage:" -ForegroundColor Gray
Write-Host "  .\validate-parity.ps1 -Quick       # Fast lib tests only" -ForegroundColor Gray
Write-Host "  .\validate-parity.ps1 -Full        # All tests" -ForegroundColor Gray
Write-Host "  .\validate-parity.ps1 -Snapshots   # Fidelity snapshots only" -ForegroundColor Gray
Write-Host "  .\validate-parity.ps1 -Fixture foo # Test specific fixture" -ForegroundColor Gray
Write-Host "  .\validate-parity.ps1 -Verbose     # Show detailed output" -ForegroundColor Gray
