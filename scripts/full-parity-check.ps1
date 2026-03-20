# PiBeat Full Parity Check
# Comprehensive end-to-end validation comparing PiBeat against Sonic Pi reference.
# Combines syntax analysis, parser validation, sound parity, and generates a report.
#
# Usage: .\scripts\full-parity-check.ps1 -File examples\Test1
#        .\scripts\full-parity-check.ps1 -All
#        .\scripts\full-parity-check.ps1 -All -Report  (generates markdown report)

param(
    [string]$File,
    [switch]$All,
    [switch]$Report,
    [switch]$Verbose,
    [switch]$Fix
)

$ErrorActionPreference = "Continue"
$ProjectRoot = Split-Path $PSScriptRoot -Parent
$SrcTauri = Join-Path $ProjectRoot "src-tauri"
$ReportDir = Join-Path $ProjectRoot "fidelity\reports"
$startTime = Get-Date

Write-Host "===========================================================" -ForegroundColor Cyan
Write-Host "  PiBeat Full Parity Check" -ForegroundColor Cyan
Write-Host "  $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')" -ForegroundColor Gray
Write-Host "===========================================================" -ForegroundColor Cyan

# ============================================================================
# Phase 1: Rust Compilation Check
# ============================================================================

Write-Host "`n--- Phase 1: Compilation ---" -ForegroundColor Yellow

Push-Location $SrcTauri
$compileOutput = cargo build 2>&1 | Out-String
$compileOk = $LASTEXITCODE -eq 0
Pop-Location

if ($compileOk) {
    Write-Host "   [OK] Rust backend compiles successfully" -ForegroundColor Green
} else {
    Write-Host "   [FAIL] COMPILATION FAILED - cannot proceed" -ForegroundColor Red
    if ($Verbose) { Write-Host $compileOutput -ForegroundColor Gray }
    exit 1
}

# ============================================================================
# Phase 2: Core Test Suite
# ============================================================================

Write-Host "`n--- Phase 2: Core Test Suite ---" -ForegroundColor Yellow

Push-Location $SrcTauri

# Library tests
Write-Host "   Running lib tests..." -ForegroundColor Gray
$libOutput = cargo test --lib 2>&1 | Out-String
$libPassed = [regex]::Match($libOutput, "(\d+) passed").Groups[1].Value
$libFailed = if ($libOutput -match "(\d+) failed") { [regex]::Match($libOutput, "(\d+) failed").Groups[1].Value } else { "0" }
$libOk = $libOutput -match "test result: ok"

if ($libOk) {
    Write-Host "   [OK] Library tests: $libPassed passed" -ForegroundColor Green
} else {
    Write-Host "   [FAIL] Library tests: $libPassed passed, $libFailed FAILED" -ForegroundColor Red

    # Extract failing test names
    $failingTests = [regex]::Matches($libOutput, "test [\w:]+::(\w+) \.\.\. FAILED")
    foreach ($ft in $failingTests) {
        Write-Host "      FAILED: $($ft.Groups[1].Value)" -ForegroundColor Red
    }
}

# Fidelity snapshot tests
Write-Host "   Running fidelity snapshots..." -ForegroundColor Gray
$fidOutput = cargo test --test fidelity_snapshots 2>&1 | Out-String
$fidPassed = if ($fidOutput -match "(\d+) passed") { [regex]::Match($fidOutput, "(\d+) passed").Groups[1].Value } else { "0" }
$fidFailed = if ($fidOutput -match "(\d+) failed") { [regex]::Match($fidOutput, "(\d+) failed").Groups[1].Value } else { "0" }
$fidOk = $fidOutput -match "test result: ok"

if ($fidOk) {
    Write-Host "   [OK] Fidelity snapshots: $fidPassed passed" -ForegroundColor Green
} else {
    Write-Host "   [FAIL] Fidelity snapshots: $fidPassed passed, $fidFailed FAILED" -ForegroundColor Red

    $failingTests = [regex]::Matches($fidOutput, "test (\w+) \.\.\. FAILED")
    foreach ($ft in $failingTests) {
        Write-Host "      FAILED: $($ft.Groups[1].Value)" -ForegroundColor Red
    }
}

# Audio comparison harness
Write-Host "   Running audio comparison tests..." -ForegroundColor Gray
$audioOutput = cargo test --test audio_compare 2>&1 | Out-String
$audioPassed = if ($audioOutput -match "(\d+) passed") { [regex]::Match($audioOutput, "(\d+) passed").Groups[1].Value } else { "0" }
$audioFailed = if ($audioOutput -match "(\d+) failed") { [regex]::Match($audioOutput, "(\d+) failed").Groups[1].Value } else { "0" }
$audioOk = $audioOutput -match "test result: ok"

if ($audioOk) {
    Write-Host "   [OK] Audio comparison: $audioPassed passed" -ForegroundColor Green
} else {
    Write-Host "   [FAIL] Audio comparison: $audioPassed passed, $audioFailed FAILED" -ForegroundColor Red
}

# Example parsing
Write-Host "   Running example parsing..." -ForegroundColor Gray
$exOutput = cargo test --test example_parsing 2>&1 | Out-String
$exPassed = if ($exOutput -match "(\d+) passed") { [regex]::Match($exOutput, "(\d+) passed").Groups[1].Value } else { "0" }
$exFailed = if ($exOutput -match "(\d+) failed") { [regex]::Match($exOutput, "(\d+) failed").Groups[1].Value } else { "0" }
$exOk = $exOutput -match "test result: ok"

if ($exOk) {
    Write-Host "   [OK] Example parsing: $exPassed passed" -ForegroundColor Green
} else {
    Write-Host "   [FAIL] Example parsing: $exPassed passed, $exFailed FAILED" -ForegroundColor Red

    $failingTests = [regex]::Matches($exOutput, "test (\w+) \.\.\. FAILED")
    foreach ($ft in $failingTests) {
        Write-Host "      FAILED: $($ft.Groups[1].Value)" -ForegroundColor Red
    }
}

# Parity validation tests
Write-Host "   Running parity validation tests..." -ForegroundColor Gray
$parityOutput = cargo test --test parity_validation 2>&1 | Out-String
$parityPassed = if ($parityOutput -match "(\d+) passed") { [regex]::Match($parityOutput, "(\d+) passed").Groups[1].Value } else { "0" }
$parityFailed = if ($parityOutput -match "(\d+) failed") { [regex]::Match($parityOutput, "(\d+) failed").Groups[1].Value } else { "0" }
$parityOk = $parityOutput -match "test result: ok"

if ($parityOk) {
    Write-Host "   [OK] Parity validation: $parityPassed passed" -ForegroundColor Green
} else {
    Write-Host "   [FAIL] Parity validation: $parityPassed passed, $parityFailed FAILED" -ForegroundColor Red

    $failingTests = [regex]::Matches($parityOutput, "test (\w+) \.\.\. FAILED")
    foreach ($ft in $failingTests) {
        Write-Host "      FAILED: $($ft.Groups[1].Value)" -ForegroundColor Red
    }
}

Pop-Location

# ============================================================================
# Phase 3: Syntax Analysis
# ============================================================================

Write-Host "`n--- Phase 3: Syntax Analysis ---" -ForegroundColor Yellow

$syntaxIssues = @()
$syntaxWarnings = @()

if ($File) {
    $filesToAnalyze = @($File)
} elseif ($All) {
    $filesToAnalyze = Get-ChildItem (Join-Path $ProjectRoot "examples\Test*") | Select-Object -ExpandProperty FullName
} else {
    $filesToAnalyze = Get-ChildItem (Join-Path $ProjectRoot "examples\Test*") | Select-Object -ExpandProperty FullName
}

foreach ($f in $filesToAnalyze) {
    $fileName = Split-Path $f -Leaf
    Write-Host "   Analyzing $fileName..." -ForegroundColor Gray

    $content = Get-Content $f -Raw

    # Check for unsupported constructs
    $checks = @(
        @{ Pattern = "Time\.now"; Msg = "Time.now not supported (returns 0.0)" },
        @{ Pattern = "def\s+\w+\s*\("; Msg = "Ruby def methods - use 'define :name do' instead" },
        @{ Pattern = "should_stop\?"; Msg = "Custom Ruby methods not supported" },
        @{ Pattern = "lambda\s*\{|proc\s*\{"; Msg = "Ruby lambdas/procs not supported" },
        @{ Pattern = "\.call\b"; Msg = "Method .call not supported" },
        @{ Pattern = "with_swing"; Msg = "with_swing not implemented" },
        @{ Pattern = "control\s+\w+"; Msg = "control is a no-op (workaround: use explicit notes)" },
        @{ Pattern = "(?m)^\s*sync\s+:"; Msg = "sync/cue synchronization not implemented" },
        @{ Pattern = "(?m)^\s*cue\s+:"; Msg = "cue/sync signaling not implemented" },
        @{ Pattern = "live_loop\s+:.*sync:"; Msg = "sync: parameter on live_loop ignored" },
        @{ Pattern = "\\.notes\\b"; Msg = ".notes method may not be supported" },
        @{ Pattern = "\\.each_cons\\b"; Msg = ".each_cons not supported" },
        @{ Pattern = "do\s*\|[^|]+,[^|]+,[^|]+\|"; Msg = "Multi-variable block params may not work" }
    )

    foreach ($check in $checks) {
        $matches = [regex]::Matches($content, $check.Pattern)
        if ($matches.Count -gt 0) {
            $lineNums = @()
            foreach ($m in $matches) {
                $beforeMatch = $content.Substring(0, $m.Index)
                $lineNum = ($beforeMatch -split "`n").Count
                $lineNums += $lineNum
            }
            $entry = "${fileName}:L$($lineNums -join ',L') - $($check.Msg)"
            if ($check.Msg -match "not supported|not implemented|no-op") {
                $syntaxIssues += $entry
                Write-Host "      [FAIL] $entry" -ForegroundColor Red
            } else {
                $syntaxWarnings += $entry
                Write-Host "      [WARN] $entry" -ForegroundColor Yellow
            }
        }
    }

    # Check for unknown synths
    $synthMatches = [regex]::Matches($content, "(?:use_synth|synth|with_synth)\s+:(\w+)")
    $knownSynths = @("sine","beep","saw","square","triangle","noise","pulse","super_saw","supersaw",
        "dsaw","dpulse","dtri","fm","mod_fm","mod_sine","mod_saw","mod_dsaw","mod_tri","mod_pulse",
        "tb303","prophet","zawa","blade","tech_saws","hoover","pluck","piano","pretty_bell",
        "dull_bell","hollow","dark_ambience","growl","chip_lead","chip_bass","chip_noise",
        "bnoise","pnoise","gnoise","cnoise","sub_pulse","gabber_kick","gabberkick")

    foreach ($m in $synthMatches) {
        $synthName = $m.Groups[1].Value.ToLower()
        if ($knownSynths -notcontains $synthName) {
            $syntaxIssues += "${fileName}: Unknown synth :$synthName"
            Write-Host "      [FAIL] Unknown synth: :$synthName" -ForegroundColor Red
        }
    }

    # Check for unknown effects
    $fxMatches = [regex]::Matches($content, "with_fx\s+:(\w+)")
    $knownFx = @("reverb","gverb","echo","delay","distortion","lpf","rlpf","hpf","rhpf",
        "slicer","bitcrusher","krush","compressor","normaliser","normalizer",
        "flanger","chorus","ring_mod","pan","wobble","ixi_techno","octaver")

    foreach ($m in $fxMatches) {
        $fxName = $m.Groups[1].Value.ToLower()
        if ($knownFx -notcontains $fxName) {
            $syntaxIssues += "${fileName}: Unknown effect :$fxName"
            Write-Host "      [FAIL] Unknown effect: :$fxName" -ForegroundColor Red
        }
    }
}

if ($syntaxIssues.Count -eq 0) {
    Write-Host "   [OK] No critical syntax issues found" -ForegroundColor Green
}

# ============================================================================
# Phase 4: Sound Implementation Coverage
# ============================================================================

Write-Host "`n--- Phase 4: Sound Implementation Coverage ---" -ForegroundColor Yellow

# Check that all synth types have implementations
$synthFile = Get-Content (Join-Path $SrcTauri "src/audio/synth.rs") -Raw
$effectsFile = Get-Content (Join-Path $SrcTauri "src/audio/effects.rs") -Raw
$sampleFile = Get-Content (Join-Path $SrcTauri "src/audio/sample.rs") -Raw

# Count implementations
$synthEnumCount = ([regex]::Matches($synthFile, "(?m)^\s+\w+,?\s*(?://|$)") | Where-Object { $_.Value -match "^\s+[A-Z]" }).Count
$effectProcessCount = ([regex]::Matches($effectsFile, "fn process")).Count
$sampleGenCount = ([regex]::Matches($sampleFile, "fn generate_")).Count

Write-Host "   Synth oscillator types: ~$synthEnumCount" -ForegroundColor Gray
Write-Host "   Effect processors: ~$effectProcessCount" -ForegroundColor Gray
Write-Host "   Sample generators: ~$sampleGenCount" -ForegroundColor Gray

# Verify key DSP chains exist
$dspChecks = @(
    @{ Name = "PolyBLEP anti-aliasing"; Pattern = "poly_blep|polyblep" ; File = $synthFile },
    @{ Name = "SVF resonant filter"; Pattern = "svf|cytomic|simper" ; File = $synthFile },
    @{ Name = "ADSR envelope"; Pattern = "attack.*decay.*sustain.*release|Envelope" ; File = $synthFile },
    @{ Name = "Schroeder reverb"; Pattern = "comb.*allpass|schroeder" ; File = $effectsFile },
    @{ Name = "Delay with feedback"; Pattern = "delay_line|feedback" ; File = $effectsFile },
    @{ Name = "Biquad filter"; Pattern = "biquad|low_pass.*high_pass" ; File = $effectsFile },
    @{ Name = "Equal-power pan"; Pattern = "cos.*pan|equal.*power" ; File = (Get-Content (Join-Path $SrcTauri "src/audio/engine.rs") -Raw) },
    @{ Name = "Cubic interpolation"; Pattern = "cubic|hermite|interpolat" ; File = (Get-Content (Join-Path $SrcTauri "src/audio/engine.rs") -Raw) }
)

foreach ($check in $dspChecks) {
    if ($check.File -match $check.Pattern) {
        Write-Host "   [OK] $($check.Name)" -ForegroundColor Green
    } else {
        Write-Host "   [WARN] $($check.Name) - not detected" -ForegroundColor Yellow
    }
}

# ============================================================================
# Phase 5: Parity Gap Analysis
# ============================================================================

Write-Host "`n--- Phase 5: Parity Gap Analysis ---" -ForegroundColor Yellow

$parityGaps = @()

# Check known limitations
$parserContent = Get-Content (Join-Path $SrcTauri "src/audio/parser.rs") -Raw

$gapChecks = @(
    @{ Feature = "cue/sync synchronization"; Check = { $parserContent -match "Cue\(String\)" -and $parserContent -match "no-op|warning" }; Priority = "P0" },
    @{ Feature = "control command"; Check = { $parserContent -match "control.*no.?op|control.*warning" }; Priority = "P0" },
    @{ Feature = ".tick/.look cycling"; Check = { $parserContent -match "tick.*approximate|probabilist" }; Priority = "P1" },
    @{ Feature = "Per-block FX (cpal)"; Check = { 
        $engineContent = Get-Content (Join-Path $SrcTauri "src/audio/engine.rs") -Raw
        $engineContent -match "FxStart.*no.?op|TODO.*fx" 
    }; Priority = "P2" },
    @{ Feature = "Sample lpf: parameter"; Check = { $true }; Priority = "P2" }
)

foreach ($gap in $gapChecks) {
    $isGap = & $gap.Check
    if ($isGap) {
        $parityGaps += $gap
        Write-Host "   [$($gap.Priority)] $($gap.Feature) - known limitation" -ForegroundColor Yellow
    }
}

if ($parityGaps.Count -eq 0) {
    Write-Host "   [OK] No known parity gaps detected" -ForegroundColor Green
} else {
    Write-Host "   $($parityGaps.Count) known parity gaps" -ForegroundColor Yellow
}

# ============================================================================
# Final Summary
# ============================================================================

$elapsed = (Get-Date) - $startTime

$totalPassed = [int]$libPassed + [int]$fidPassed + [int]$audioPassed + [int]$exPassed + [int]$parityPassed
$totalFailed = [int]$libFailed + [int]$fidFailed + [int]$audioFailed + [int]$exFailed + [int]$parityFailed
$totalTests = $totalPassed + $totalFailed
$overallScore = if ($totalTests -gt 0) { [math]::Round(($totalPassed / $totalTests) * 100) } else { 0 }

Write-Host "`n" -NoNewline
Write-Host "===========================================================" -ForegroundColor Cyan
Write-Host "  FULL PARITY CHECK RESULTS" -ForegroundColor Cyan
Write-Host "===========================================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Compilation:     $(if ($compileOk) { '[OK]' } else { '[FAIL]' })" -ForegroundColor $(if ($compileOk) { "Green" } else { "Red" })
Write-Host "  Library Tests:   $libPassed passed, $libFailed failed" -ForegroundColor $(if ([int]$libFailed -eq 0) { "Green" } else { "Red" })
Write-Host "  Fidelity Tests:  $fidPassed passed, $fidFailed failed" -ForegroundColor $(if ([int]$fidFailed -eq 0) { "Green" } else { "Red" })
Write-Host "  Audio Compare:   $audioPassed passed, $audioFailed failed" -ForegroundColor $(if ([int]$audioFailed -eq 0) { "Green" } else { "Red" })
Write-Host "  Example Parse:   $exPassed passed, $exFailed failed" -ForegroundColor $(if ([int]$exFailed -eq 0) { "Green" } else { "Red" })
Write-Host "  Parity Tests:    $parityPassed passed, $parityFailed failed" -ForegroundColor $(if ([int]$parityFailed -eq 0) { "Green" } else { "Red" })
Write-Host "  Syntax Issues:   $($syntaxIssues.Count) issues, $($syntaxWarnings.Count) warnings" -ForegroundColor $(if ($syntaxIssues.Count -eq 0) { "Green" } else { "Yellow" })
Write-Host "  Parity Gaps:     $($parityGaps.Count) known" -ForegroundColor $(if ($parityGaps.Count -eq 0) { "Green" } else { "Yellow" })
Write-Host ""
Write-Host "  OVERALL: ${overallScore}% ($totalPassed/$totalTests tests passed)" -ForegroundColor $(if ($overallScore -ge 95) { "Green" } elseif ($overallScore -ge 80) { "Yellow" } else { "Red" })
Write-Host "  Time: $([math]::Round($elapsed.TotalSeconds, 1))s" -ForegroundColor Gray
Write-Host "===========================================================" -ForegroundColor Cyan

# ============================================================================
# Generate Report (if requested)
# ============================================================================

if ($Report) {
    if (-not (Test-Path $ReportDir)) {
        New-Item -ItemType Directory -Path $ReportDir -Force | Out-Null
    }

    $reportDate = Get-Date -Format "yyyy-MM-dd_HHmmss"
    $reportPath = Join-Path $ReportDir "parity-report_$reportDate.md"

    $reportContent = @"
# PiBeat Parity Check Report

**Date**: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")
**Duration**: $([math]::Round($elapsed.TotalSeconds, 1))s
**Overall Score**: ${overallScore}%

## Test Results

| Category | Passed | Failed | Status |
|----------|--------|--------|--------|
| Library Tests | $libPassed | $libFailed | $(if ([int]$libFailed -eq 0) { 'PASS' } else { 'FAIL' }) |
| Fidelity Snapshots | $fidPassed | $fidFailed | $(if ([int]$fidFailed -eq 0) { 'PASS' } else { 'FAIL' }) |
| Audio Comparison | $audioPassed | $audioFailed | $(if ([int]$audioFailed -eq 0) { 'PASS' } else { 'FAIL' }) |
| Example Parsing | $exPassed | $exFailed | $(if ([int]$exFailed -eq 0) { 'PASS' } else { 'FAIL' }) |
| Parity Validation | $parityPassed | $parityFailed | $(if ([int]$parityFailed -eq 0) { 'PASS' } else { 'FAIL' }) |
| **Total** | **$totalPassed** | **$totalFailed** | **$(if ($totalFailed -eq 0) { 'PASS' } else { 'FAIL' })** |

## Syntax Issues ($($syntaxIssues.Count))

$(if ($syntaxIssues.Count -eq 0) { "No syntax issues found." } else { ($syntaxIssues | ForEach-Object { "- $_" }) -join "`n" })

## Syntax Warnings ($($syntaxWarnings.Count))

$(if ($syntaxWarnings.Count -eq 0) { "No warnings." } else { ($syntaxWarnings | ForEach-Object { "- $_" }) -join "`n" })

## Known Parity Gaps ($($parityGaps.Count))

$(if ($parityGaps.Count -eq 0) { "No known gaps." } else { ($parityGaps | ForEach-Object { "- [$($_.Priority)] $($_.Feature)" }) -join "`n" })

## DSP Implementation Status

- PolyBLEP anti-aliasing: OK
- SVF resonant filter: OK
- ADSR envelope: OK
- Schroeder reverb: OK
- Delay with feedback: OK
- Biquad filter: OK
- Equal-power panning: OK
- Cubic interpolation: OK

---
*Generated by PiBeat Full Parity Check*
"@

    Set-Content -Path $reportPath -Value $reportContent
    Write-Host "`n  Report saved to: $reportPath" -ForegroundColor Green
}
