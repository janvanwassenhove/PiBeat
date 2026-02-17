<#
.SYNOPSIS
    Pre-compiles PiBeat SynthDefs using sclang.
    
.DESCRIPTION
    Runs compile_all.scd through sclang to produce pre-compiled .scsyndef
    binary files. These are bundled with the app so sclang is NOT required
    at runtime.
    
.PARAMETER SclangPath
    Path to sclang.exe. If not specified, searches common installation paths.
#>

param(
    [string]$SclangPath = ""
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$OutputDir = Join-Path (Join-Path (Join-Path $ProjectRoot "src-tauri") "sc-bundle") "synthdefs"
$ScdScript = Join-Path $OutputDir "compile_all.scd"

# Ensure output directory and compile script exist
if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}
if (-not (Test-Path $ScdScript)) {
    Write-Host "[ERROR] compile_all.scd not found at: $ScdScript" -ForegroundColor Red
    exit 1
}

# Find sclang if not provided
if (-not $SclangPath -or -not (Test-Path $SclangPath)) {
    Write-Host "[..] Searching for sclang..." -ForegroundColor Yellow
    
    $SearchPaths = @()
    
    # Check sc-download temp directory first
    $TempDir = Join-Path $ProjectRoot ".sc-download"
    if (Test-Path $TempDir) {
        $ScFolders = Get-ChildItem -Path $TempDir -Directory -Filter "SuperCollider*" -ErrorAction SilentlyContinue
        foreach ($folder in $ScFolders) {
            $SearchPaths += $folder.FullName
        }
    }
    
    # Common Windows install locations
    if ($env:ProgramFiles) {
        $SearchPaths += Join-Path $env:ProgramFiles "SuperCollider"
        $ScDirs = Get-ChildItem -Path $env:ProgramFiles -Directory -Filter "SuperCollider*" -ErrorAction SilentlyContinue
        foreach ($d in $ScDirs) { $SearchPaths += $d.FullName }
    }
    if (${env:ProgramFiles(x86)}) {
        $SearchPaths += Join-Path ${env:ProgramFiles(x86)} "SuperCollider"
    }
    if ($env:LOCALAPPDATA) {
        $SearchPaths += Join-Path (Join-Path $env:LOCALAPPDATA "Programs") "SuperCollider"
    }
    
    foreach ($path in $SearchPaths) {
        $candidate = Join-Path $path "sclang.exe"
        if (Test-Path $candidate) {
            $SclangPath = $candidate
            break
        }
    }
    
    # Try 'where' command as fallback
    if (-not $SclangPath -or -not (Test-Path $SclangPath)) {
        try {
            $whereResult = & where.exe sclang 2>$null
            if ($whereResult) {
                $SclangPath = ($whereResult -split "`n")[0].Trim()
            }
        } catch { }
    }
}

if (-not $SclangPath -or -not (Test-Path $SclangPath)) {
    Write-Host "[ERROR] sclang.exe not found!" -ForegroundColor Red
    Write-Host "        Please install SuperCollider or run setup_sc.ps1 first." -ForegroundColor Red
    Write-Host "        You can also specify the path: -SclangPath 'C:\path\to\sclang.exe'" -ForegroundColor Yellow
    exit 1
}

Write-Host "[OK] Using sclang: $SclangPath" -ForegroundColor Green
Write-Host "[..] Output directory: $OutputDir" -ForegroundColor DarkGray

Write-Host "[..] Running sclang to compile SynthDefs..." -ForegroundColor Yellow
Write-Host "     Script: $ScdScript" -ForegroundColor DarkGray

# Run sclang with a timeout
$ProcessInfo = New-Object System.Diagnostics.ProcessStartInfo
$ProcessInfo.FileName = $SclangPath
$ProcessInfo.Arguments = "`"$ScdScript`""
$ProcessInfo.RedirectStandardOutput = $true
$ProcessInfo.RedirectStandardError = $true
$ProcessInfo.UseShellExecute = $false
$ProcessInfo.CreateNoWindow = $true

$Process = New-Object System.Diagnostics.Process
$Process.StartInfo = $ProcessInfo

try {
    $Process.Start() | Out-Null
    
    # Wait up to 120 seconds for sclang to finish
    $exited = $Process.WaitForExit(120000)
    
    $stdout = $Process.StandardOutput.ReadToEnd()
    $stderr = $Process.StandardError.ReadToEnd()
    
    if (-not $exited) {
        $Process.Kill()
        Write-Host "[WARN] sclang timed out after 120 seconds" -ForegroundColor Yellow
    }
    
    if ($stdout -match "SynthDefs compiled successfully") {
        Write-Host "[OK] SynthDefs compiled successfully!" -ForegroundColor Green
    } else {
        Write-Host "[WARN] sclang finished but may not have compiled all SynthDefs" -ForegroundColor Yellow
        if ($stdout) { Write-Host "       stdout: $stdout" -ForegroundColor DarkGray }
        if ($stderr) { Write-Host "       stderr: $stderr" -ForegroundColor DarkGray }
    }
}
catch {
    Write-Host "[ERROR] Failed to run sclang: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}

# Verify compiled files
$CompiledFiles = Get-ChildItem -Path $OutputDir -Filter "*.scsyndef" -ErrorAction SilentlyContinue
Write-Host "[OK] Compiled $($CompiledFiles.Count) SynthDef files" -ForegroundColor Green
foreach ($f in $CompiledFiles | Select-Object -First 10) {
    Write-Host "     $($f.Name)" -ForegroundColor DarkGray
}
if ($CompiledFiles.Count -gt 10) {
    Write-Host "     ... and $($CompiledFiles.Count - 10) more" -ForegroundColor DarkGray
}

