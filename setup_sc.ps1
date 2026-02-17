<#
.SYNOPSIS
    Downloads and extracts SuperCollider files needed to embed scsynth in PiBeat.
    
.DESCRIPTION
    This script downloads the SuperCollider Windows release, extracts only the
    files needed for the embedded audio engine (scsynth, UGen plugins, DLLs),
    and places them in src-tauri/sc-bundle/ for Tauri resource bundling.
    
    It also pre-compiles SynthDefs using sclang, so sclang is NOT needed at runtime.
    
.NOTES
    Run this once before building the app. Requires internet access.
    SuperCollider is licensed under GPL v3.
#>

param(
    [string]$ScVersion = "3.14.1",
    [switch]$Force
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$BundleDir = Join-Path (Join-Path $ProjectRoot "src-tauri") "sc-bundle"
$TempDir = Join-Path $ProjectRoot ".sc-download"

$ScZipUrl = "https://github.com/supercollider/supercollider/releases/download/Version-$ScVersion/SuperCollider-$ScVersion-win64.zip"
$ScZipFile = Join-Path $TempDir "SuperCollider-$ScVersion.zip"
$ScExtractDir = Join-Path $TempDir "SuperCollider-$ScVersion"

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  PiBeat - SuperCollider Setup" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Check if already set up
if ((Test-Path (Join-Path $BundleDir "scsynth.exe")) -and -not $Force) {
    Write-Host "[OK] SC bundle already exists at $BundleDir" -ForegroundColor Green
    Write-Host "     Use -Force to re-download and re-extract." -ForegroundColor DarkGray
    
    # Still compile synthdefs if they're missing
    $SynthDefsDir = Join-Path $BundleDir "synthdefs"
    if (-not (Test-Path (Join-Path $SynthDefsDir "sonic_beep.scsyndef"))) {
        Write-Host "[..] SynthDefs not compiled yet, compiling..." -ForegroundColor Yellow
        & (Join-Path $ProjectRoot "compile_synthdefs.ps1")
    }
    exit 0
}

# ----------------------------------------
# Step 1: Download SuperCollider
# ----------------------------------------
Write-Host "[1/4] Downloading SuperCollider $ScVersion..." -ForegroundColor Yellow

if (-not (Test-Path $TempDir)) {
    New-Item -ItemType Directory -Path $TempDir | Out-Null
}

if (-not (Test-Path $ScZipFile)) {
    Write-Host "       URL: $ScZipUrl" -ForegroundColor DarkGray
    try {
        # Use TLS 1.2
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        
        $ProgressPreference = 'SilentlyContinue'  # Speed up download
        Invoke-WebRequest -Uri $ScZipUrl -OutFile $ScZipFile -UseBasicParsing
        $ProgressPreference = 'Continue'
        
        Write-Host "       Downloaded: $([math]::Round((Get-Item $ScZipFile).Length / 1MB, 1)) MB" -ForegroundColor DarkGray
    }
    catch {
        Write-Host "[ERROR] Failed to download SuperCollider." -ForegroundColor Red
        Write-Host "        $($_.Exception.Message)" -ForegroundColor Red
        Write-Host ""
        Write-Host "You can manually download from:" -ForegroundColor Yellow
        Write-Host "  $ScZipUrl" -ForegroundColor Cyan
        Write-Host "Extract and place files in: $BundleDir" -ForegroundColor Yellow
        exit 1
    }
} else {
    Write-Host "       Using cached download" -ForegroundColor DarkGray
}

# ----------------------------------------
# Step 2: Extract SuperCollider
# ----------------------------------------
Write-Host "[2/4] Extracting SuperCollider..." -ForegroundColor Yellow

if (Test-Path $ScExtractDir) {
    Remove-Item -Recurse -Force $ScExtractDir
}

Expand-Archive -Path $ScZipFile -DestinationPath $TempDir -Force

# Find the actual extracted folder (may have a slightly different name)
$ExtractedFolder = Get-ChildItem -Path $TempDir -Directory | Where-Object { $_.Name -like "SuperCollider*" } | Select-Object -First 1
if (-not $ExtractedFolder) {
    Write-Host "[ERROR] Could not find extracted SuperCollider folder" -ForegroundColor Red
    exit 1
}
$ScSourceDir = $ExtractedFolder.FullName
Write-Host "       Extracted to: $ScSourceDir" -ForegroundColor DarkGray

# ----------------------------------------
# Step 3: Copy needed files to sc-bundle
# ----------------------------------------
Write-Host "[3/4] Copying required files to sc-bundle..." -ForegroundColor Yellow

# Create bundle directory
if (Test-Path $BundleDir) {
    Remove-Item -Recurse -Force $BundleDir
}
New-Item -ItemType Directory -Path $BundleDir | Out-Null
New-Item -ItemType Directory -Path (Join-Path $BundleDir "plugins") | Out-Null
New-Item -ItemType Directory -Path (Join-Path $BundleDir "synthdefs") | Out-Null

# Copy scsynth executable
$ScSynthSrc = Join-Path $ScSourceDir "scsynth.exe"
if (Test-Path $ScSynthSrc) {
    Copy-Item $ScSynthSrc -Destination $BundleDir
    Write-Host "       scsynth.exe" -ForegroundColor DarkGray
} else {
    Write-Host "[ERROR] scsynth.exe not found in extracted folder" -ForegroundColor Red
    exit 1
}

# Copy required DLLs (these are needed by scsynth at runtime)
$RequiredDlls = @(
    "libsndfile-1.dll",
    "scsynth.dll",
    "libfftw3f-3.dll",
    "libfftw3-3.dll"
)

# Also copy any DLLs that start with common SC dependencies
$OptionalDllPatterns = @(
    "libgcc*.dll",
    "libstdc*.dll",
    "libwinpthread*.dll",
    "portaudio*.dll",
    "Qt*.dll",
    "libsamplerate*.dll"
)

$CopiedDlls = 0
foreach ($dll in $RequiredDlls) {
    $dllPath = Join-Path $ScSourceDir $dll
    if (Test-Path $dllPath) {
        Copy-Item $dllPath -Destination $BundleDir
        Write-Host "       $dll" -ForegroundColor DarkGray
        $CopiedDlls++
    }
}

foreach ($pattern in $OptionalDllPatterns) {
    $matches = Get-ChildItem -Path $ScSourceDir -Filter $pattern -ErrorAction SilentlyContinue
    foreach ($match in $matches) {
        Copy-Item $match.FullName -Destination $BundleDir
        Write-Host "       $($match.Name) (optional)" -ForegroundColor DarkGray
        $CopiedDlls++
    }
}

# Copy ALL DLLs from the SC directory (safest approach - ensures nothing is missing)
$AllDlls = Get-ChildItem -Path $ScSourceDir -Filter "*.dll" -ErrorAction SilentlyContinue
foreach ($dll in $AllDlls) {
    $dest = Join-Path $BundleDir $dll.Name
    if (-not (Test-Path $dest)) {
        Copy-Item $dll.FullName -Destination $BundleDir
        Write-Host "       $($dll.Name) (extra)" -ForegroundColor DarkGray
        $CopiedDlls++
    }
}

Write-Host "       Copied $CopiedDlls DLLs" -ForegroundColor DarkGray

# Copy UGen plugins (.scx files)
$PluginsSource = Join-Path $ScSourceDir "plugins"
if (Test-Path $PluginsSource) {
    $PluginFiles = Get-ChildItem -Path $PluginsSource -Filter "*.scx" -ErrorAction SilentlyContinue
    $PluginCount = 0
    foreach ($plugin in $PluginFiles) {
        Copy-Item $plugin.FullName -Destination (Join-Path $BundleDir "plugins")
        $PluginCount++
    }
    Write-Host "       Copied $PluginCount UGen plugins (.scx)" -ForegroundColor DarkGray
} else {
    Write-Host "[WARN] plugins/ directory not found in SC installation" -ForegroundColor Yellow
    Write-Host "       scsynth will have limited functionality" -ForegroundColor Yellow
}

# ----------------------------------------
# Step 4: Compile SynthDefs
# ----------------------------------------
Write-Host "[4/4] Compiling SynthDefs..." -ForegroundColor Yellow

# Check if sclang exists in the extracted folder
$SclangPath = Join-Path $ScSourceDir "sclang.exe"
if (Test-Path $SclangPath) {
    # Use the compile_synthdefs.ps1 script, passing the sclang path
    & (Join-Path $ProjectRoot "compile_synthdefs.ps1") -SclangPath $SclangPath
} else {
    Write-Host "[WARN] sclang.exe not found - SynthDefs will be compiled on first run" -ForegroundColor Yellow
    Write-Host "       This requires SuperCollider to be installed on the target machine" -ForegroundColor Yellow
}

# ----------------------------------------
# Summary
# ----------------------------------------
Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "  Setup Complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
Write-Host ""

$BundleSize = (Get-ChildItem -Recurse -Path $BundleDir | Measure-Object -Sum Length).Sum / 1MB
Write-Host "Bundle location: $BundleDir" -ForegroundColor Cyan
Write-Host "Bundle size:     $([math]::Round($BundleSize, 1)) MB" -ForegroundColor Cyan
Write-Host ""
Write-Host "Files included:" -ForegroundColor Cyan
Write-Host "  - scsynth.exe (audio server)" -ForegroundColor DarkGray
Write-Host "  - DLLs (runtime dependencies)" -ForegroundColor DarkGray
Write-Host "  - plugins/*.scx (UGen plugins)" -ForegroundColor DarkGray
Write-Host "  - synthdefs/*.scsyndef (pre-compiled SynthDefs)" -ForegroundColor DarkGray
Write-Host ""
Write-Host "You can now build the app with: npm run tauri build" -ForegroundColor Yellow
Write-Host ""

# Clean up temp directory (optional)
# Remove-Item -Recurse -Force $TempDir
Write-Host "Temp files kept at: $TempDir" -ForegroundColor DarkGray
Write-Host "Run 'Remove-Item -Recurse -Force `"$TempDir`"' to clean up." -ForegroundColor DarkGray
