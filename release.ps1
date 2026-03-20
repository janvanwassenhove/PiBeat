# PiBeat Release Builder (Tauri v2)
param(
    [Parameter(Mandatory=$false)]
    [switch]$SkipBuild,

    [Parameter(Mandatory=$false)]
    [switch]$SkipPush,

    [Parameter(Mandatory=$false)]
    [switch]$Help
)

if ($Help) {
    Write-Host @"
PiBeat Release Builder

Usage:
    .\release.ps1 [-SkipBuild] [-SkipPush] [-Help]

Parameters:
    -SkipBuild            : Skip local Tauri build (just tag & push)
    -SkipPush             : Don't push to remote repository
    -Help                 : Show this help message

Examples:
    .\release.ps1
    .\release.ps1 -SkipPush
    .\release.ps1 -SkipBuild

What happens:
    1. You'll be prompted for the version number
    2. Updates version in package.json, tauri.conf.json, and Cargo.toml
    3. Optionally builds locally via 'npm run tauri build' (verifies the build works)
    4. Creates a git tag (v<version>)
    5. Pushes tag to GitHub
    6. GitHub Actions automatically builds Windows + macOS + Linux installers
    7. A GitHub Release is created with all installers attached
"@
    exit 0
}

Write-Host ""
Write-Host "=================================================" -ForegroundColor Cyan
Write-Host "         PiBeat Release Builder (Tauri v2)" -ForegroundColor Cyan
Write-Host "=================================================" -ForegroundColor Cyan
Write-Host ""

# Check if we're in a git repository
try {
    git rev-parse --git-dir 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw }
} catch {
    Write-Host "ERROR: Not in a git repository!" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

# Check if working directory is clean
$status = git status --porcelain 2>$null
if ($status) {
    Write-Host "ERROR: Working directory is not clean! Please commit or stash changes first." -ForegroundColor Red
    Write-Host ""
    git status --short
    Read-Host "Press Enter to exit"
    exit 1
}

# Check required tools
$requiredTools = @("node", "npm", "cargo", "rustc")
foreach ($tool in $requiredTools) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        Write-Host "ERROR: '$tool' is not installed or not in PATH!" -ForegroundColor Red
        Read-Host "Press Enter to exit"
        exit 1
    }
}

# Version files
$packageJsonPath = "package.json"
$tauriConfPath = "src-tauri\tauri.conf.json"
$cargoTomlPath = "src-tauri\Cargo.toml"

foreach ($file in @($packageJsonPath, $tauriConfPath, $cargoTomlPath)) {
    if (-not (Test-Path $file)) {
        Write-Host "ERROR: $file not found!" -ForegroundColor Red
        Read-Host "Press Enter to exit"
        exit 1
    }
}

# Get current version from package.json
$packageJson = Get-Content $packageJsonPath -Raw | ConvertFrom-Json
$currentVersion = $packageJson.version

Write-Host "Current version: $currentVersion" -ForegroundColor Yellow
Write-Host ""

# Get new version
do {
    $Version = Read-Host "Enter new version (e.g., 0.2.0, 1.0.0)"
} while (-not $Version)

# Validate version format
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') {
    Write-Host "ERROR: Invalid version format! Use semantic versioning (e.g., 1.0.0)" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

# Check tag doesn't already exist
$existingTag = git tag -l "v$Version" 2>$null
if ($existingTag) {
    Write-Host "ERROR: Tag v$Version already exists!" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

Write-Host ""
Write-Host "Preparing release v$Version..." -ForegroundColor Green
Write-Host ""

# Function to restore backups on failure
function Restore-BackupFiles {
    Write-Host ""
    Write-Host "=================================================" -ForegroundColor Red
    Write-Host "ERROR: Release failed! Restoring backup files..." -ForegroundColor Red
    Write-Host "=================================================" -ForegroundColor Red
    Write-Host ""

    foreach ($file in @($packageJsonPath, $tauriConfPath, $cargoTomlPath)) {
        if (Test-Path "$file.backup") {
            Copy-Item "$file.backup" $file -Force
            Remove-Item "$file.backup" -Force
        }
    }
    Write-Host "Backup files restored. Please fix the issues and try again." -ForegroundColor Yellow
    Read-Host "Press Enter to exit"
    exit 1
}

# Create backups
Write-Host "Creating backups of version files..." -ForegroundColor Yellow
foreach ($file in @($packageJsonPath, $tauriConfPath, $cargoTomlPath)) {
    Copy-Item $file "$file.backup" -Force
}

try {
    # Update version in package.json
    Write-Host "Updating package.json..." -ForegroundColor Yellow
    $packageJson.version = $Version
    $packageJson | ConvertTo-Json -Depth 10 | Set-Content $packageJsonPath

    # Update version in tauri.conf.json
    Write-Host "Updating tauri.conf.json..." -ForegroundColor Yellow
    $tauriConf = Get-Content $tauriConfPath -Raw | ConvertFrom-Json
    $tauriConf.version = $Version
    $tauriConf | ConvertTo-Json -Depth 10 | Set-Content $tauriConfPath

    # Update version in Cargo.toml
    Write-Host "Updating Cargo.toml..." -ForegroundColor Yellow
    $cargoContent = Get-Content $cargoTomlPath -Raw
    $cargoContent = $cargoContent -replace '(?m)^version\s*=\s*"[^"]*"', "version = `"$Version`""
    Set-Content $cargoTomlPath $cargoContent -NoNewline

    # Optional local build
    if (-not $SkipBuild) {
        Write-Host ""
        Write-Host "=================================================" -ForegroundColor Cyan
        Write-Host "Building Tauri application locally..." -ForegroundColor Cyan
        Write-Host "=================================================" -ForegroundColor Cyan
        Write-Host ""

        # Install frontend dependencies
        Write-Host "Installing frontend dependencies..." -ForegroundColor Yellow
        npm install
        if ($LASTEXITCODE -ne 0) {
            Write-Host "ERROR: npm install failed!" -ForegroundColor Red
            Restore-BackupFiles
        }

        # Build the Tauri app
        Write-Host "Building Tauri app (this may take a while on first run)..." -ForegroundColor Yellow
        npm run tauri build
        if ($LASTEXITCODE -ne 0) {
            Write-Host "ERROR: Tauri build failed!" -ForegroundColor Red
            Restore-BackupFiles
        }

        Write-Host ""
        Write-Host "Local build successful!" -ForegroundColor Green
    } else {
        Write-Host "Skipping local build (-SkipBuild)." -ForegroundColor Yellow
    }

    Write-Host ""
    Write-Host "=================================================" -ForegroundColor Green
    Write-Host "Creating git release..." -ForegroundColor Green
    Write-Host "=================================================" -ForegroundColor Green
    Write-Host ""

    # Commit version changes
    Write-Host "Committing version changes..." -ForegroundColor Yellow
    git add $packageJsonPath $tauriConfPath $cargoTomlPath
    git commit -m "Release v$Version"
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Git commit failed!" -ForegroundColor Red
        Restore-BackupFiles
    }

    # Create git tag
    Write-Host "Creating git tag v$Version..." -ForegroundColor Yellow
    git tag -a "v$Version" -m "Release v$Version"
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Git tag creation failed!" -ForegroundColor Red
        Restore-BackupFiles
    }

    # Push changes and tag
    if (-not $SkipPush) {
        Write-Host ""
        $pushChoice = Read-Host "Push changes and tag to remote? (y/n)"
        if ($pushChoice -eq "y" -or $pushChoice -eq "Y") {
            Write-Host "Pushing changes..." -ForegroundColor Yellow
            git push origin main
            if ($LASTEXITCODE -ne 0) {
                Write-Host "WARNING: Failed to push commits, but tag was created locally." -ForegroundColor Yellow
            }

            Write-Host "Pushing tag..." -ForegroundColor Yellow
            git push origin "v$Version"
            if ($LASTEXITCODE -ne 0) {
                Write-Host "WARNING: Failed to push tag." -ForegroundColor Yellow
            }
        }
    }

    # Clean up backup files
    Write-Host "Cleaning up backup files..." -ForegroundColor Yellow
    foreach ($file in @($packageJsonPath, $tauriConfPath, $cargoTomlPath)) {
        Remove-Item "$file.backup" -Force -ErrorAction SilentlyContinue
    }

    Write-Host ""
    Write-Host "=================================================" -ForegroundColor Green
    Write-Host "SUCCESS! Release v$Version created!" -ForegroundColor Green
    Write-Host "=================================================" -ForegroundColor Green
    Write-Host ""

    if (-not $SkipBuild) {
        Write-Host "Local build artifacts:" -ForegroundColor Yellow
        Write-Host "  src-tauri\target\release\bundle\" -ForegroundColor White
        Write-Host ""
    }

    Write-Host "Git tag created: v$Version" -ForegroundColor Yellow
    Write-Host "Versions updated in: package.json, tauri.conf.json, Cargo.toml" -ForegroundColor Yellow
    Write-Host ""

    if (-not $SkipPush -and ($pushChoice -eq "y" -or $pushChoice -eq "Y")) {
        Write-Host ">> GitHub Actions will now automatically:" -ForegroundColor Cyan
        Write-Host "   1. Build Windows installer (.msi / .exe)" -ForegroundColor White
        Write-Host "   2. Build macOS installer (.dmg - Universal)" -ForegroundColor White
        Write-Host "   3. Build Linux packages (.deb / .AppImage)" -ForegroundColor White
        Write-Host "   4. Create GitHub Release with all installers" -ForegroundColor White
        Write-Host ""
        Write-Host ">> Monitor progress at:" -ForegroundColor Yellow
        Write-Host "   https://github.com/janvanwassenhove/PiBeat/actions" -ForegroundColor Cyan
        Write-Host ""
        Write-Host ">> Release will appear at:" -ForegroundColor Yellow
        Write-Host "   https://github.com/janvanwassenhove/PiBeat/releases/tag/v$Version" -ForegroundColor Cyan
    } else {
        Write-Host ">> To trigger multi-platform builds, push the tag:" -ForegroundColor Yellow
        Write-Host "   git push origin main" -ForegroundColor White
        Write-Host "   git push origin v$Version" -ForegroundColor White
        Write-Host "   GitHub Actions will build Windows + macOS + Linux installers automatically." -ForegroundColor Gray
    }
    Write-Host ""

} catch {
    Write-Host "ERROR: An unexpected error occurred: $_" -ForegroundColor Red
    Restore-BackupFiles
}

Read-Host "Press Enter to exit"
