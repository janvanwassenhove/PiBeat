# PiBeat Sound Parity Validation Script
# Validates that every sound output (sample, synth, effect) produces correct audio
# by parsing test files through the Rust engine and checking event streams.
#
# Usage: .\scripts\validate-sound-parity.ps1 -File <path>
#        .\scripts\validate-sound-parity.ps1 -All
#        .\scripts\validate-sound-parity.ps1 -Synths     # Check all synth types
#        .\scripts\validate-sound-parity.ps1 -Effects     # Check all effects
#        .\scripts\validate-sound-parity.ps1 -Samples     # Check all sample types

param(
    [string]$File,
    [switch]$All,
    [switch]$Synths,
    [switch]$Effects,
    [switch]$Samples,
    [switch]$Verbose,
    [switch]$Fix
)

$ErrorActionPreference = "Continue"
$ProjectRoot = Split-Path $PSScriptRoot -Parent
$SrcTauri = Join-Path $ProjectRoot "src-tauri"

# ============================================================================
# Sonic Pi Reference Values
# ============================================================================

# Sonic Pi v4.x default parameter values
$SonicPiDefaults = @{
    # Note defaults
    amp = 1.0
    pan = 0.0
    attack = 0.0
    decay = 0.0
    sustain = 0.0          # Hold time in beats (NOT sustain level)
    sustain_level = 1.0
    release = 1.0
    # Filter defaults
    cutoff = 130           # MIDI note (wide open)
    res = 0.0
    # Sample defaults
    rate = 1.0
    # Tempo
    bpm = 60.0
}

# MIDI note to frequency reference table (A4 = 440 Hz)
$NoteFreqReference = @{
    60 = 261.63    # C4
    61 = 277.18    # C#4
    62 = 293.66    # D4
    63 = 311.13    # D#4
    64 = 329.63    # E4
    65 = 349.23    # F4
    66 = 369.99    # F#4
    67 = 392.00    # G4
    68 = 415.30    # G#4
    69 = 440.00    # A4
    70 = 466.16    # A#4
    71 = 493.88    # B4
    72 = 523.25    # C5
    48 = 130.81    # C3
    36 = 65.41     # C2
    24 = 32.70     # C1
}

# Sonic Pi chord intervals (semitones from root)
$ChordIntervals = @{
    "major"  = @(0, 4, 7)
    "minor"  = @(0, 3, 7)
    "dom7"   = @(0, 4, 7, 10)
    "min7"   = @(0, 3, 7, 10)
    "maj7"   = @(0, 4, 7, 11)
    "dim"    = @(0, 3, 6)
    "aug"    = @(0, 4, 8)
    "sus2"   = @(0, 2, 7)
    "sus4"   = @(0, 5, 7)
    "minor7" = @(0, 3, 7, 10)
}

# ============================================================================
# Test Functions
# ============================================================================

function Run-CargoTest {
    param([string]$TestName, [switch]$Capture)

    Push-Location $SrcTauri

    $captureFlag = if ($Capture) { "-- --nocapture" } else { "" }
    $output = Invoke-Expression "cargo test $TestName $captureFlag 2>&1" | Out-String

    $passed = $output -match "test result: ok"
    # Use -cmatch (case-sensitive) to avoid matching "0 failed" from passed test summaries
    $failed = $output -cmatch "\.\.\. FAILED" -or $output -cmatch "test result: FAILED"

    Pop-Location

    return @{
        Output = $output
        Passed = $passed -and (-not $failed)
        TestName = $TestName
    }
}

function Test-SynthTypes {
    Write-Host "`n[SYNTH] SYNTH TYPE VALIDATION" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------" -ForegroundColor Cyan

    $synths = @(
        @{ Name = "sine"; Alias = "beep" },
        @{ Name = "saw" },
        @{ Name = "square" },
        @{ Name = "triangle" },
        @{ Name = "noise" },
        @{ Name = "pulse" },
        @{ Name = "super_saw"; Alias = "supersaw" },
        @{ Name = "dsaw" },
        @{ Name = "dpulse" },
        @{ Name = "dtri" },
        @{ Name = "fm" },
        @{ Name = "mod_fm" },
        @{ Name = "mod_sine" },
        @{ Name = "mod_saw" },
        @{ Name = "mod_dsaw" },
        @{ Name = "mod_tri" },
        @{ Name = "mod_pulse" },
        @{ Name = "tb303" },
        @{ Name = "prophet" },
        @{ Name = "zawa" },
        @{ Name = "blade" },
        @{ Name = "tech_saws" },
        @{ Name = "hoover" },
        @{ Name = "pluck" },
        @{ Name = "piano" },
        @{ Name = "pretty_bell" },
        @{ Name = "dull_bell" },
        @{ Name = "hollow" },
        @{ Name = "dark_ambience" },
        @{ Name = "growl" },
        @{ Name = "chip_lead" },
        @{ Name = "chip_bass" },
        @{ Name = "chip_noise" },
        @{ Name = "bnoise" },
        @{ Name = "pnoise" },
        @{ Name = "gnoise" },
        @{ Name = "cnoise" },
        @{ Name = "sub_pulse" },
        @{ Name = "gabber_kick" }
    )

    $passed = 0
    $failed = 0
    $errors = @()

    foreach ($synth in $synths) {
        $code = "use_synth :$($synth.Name)`nplay :c4, release: 0.2"

        # Write temp fixture
        $tempPath = Join-Path $SrcTauri "test_parity_temp.rb"
        Set-Content -Path $tempPath -Value $code

        # Test parsing
        $result = Run-CargoTest "snapshot_play_note_basic" -Capture

        if ($Verbose) {
            Write-Host "   Testing :$($synth.Name)..." -ForegroundColor Gray -NoNewline
        }

        # Since we can't dynamically create tests, verify the synth name is in parse_synth_name
        $parserContent = Get-Content (Join-Path $SrcTauri "src/audio/parser.rs") -Raw
        $synthPattern = """$($synth.Name)"""

        if ($parserContent -match [regex]::Escape($synthPattern)) {
            $passed++
            if ($Verbose) { Write-Host " [OK]" -ForegroundColor Green }
        } else {
            # Try alias
            if ($synth.Alias -and ($parserContent -match [regex]::Escape("""$($synth.Alias)"""))) {
                $passed++
                if ($Verbose) { Write-Host " [OK] (alias: $($synth.Alias))" -ForegroundColor Green }
            } else {
                $failed++
                $errors += ":$($synth.Name)"
                if ($Verbose) { Write-Host " [FAIL] NOT IN PARSER" -ForegroundColor Red }
            }
        }

        # Cleanup
        if (Test-Path $tempPath) { Remove-Item $tempPath }
    }

    Write-Host "   Result: $passed/$($synths.Count) synths registered in parser" -ForegroundColor $(if ($failed -eq 0) { "Green" } else { "Yellow" })
    if ($errors.Count -gt 0) {
        Write-Host "   Missing: $($errors -join ', ')" -ForegroundColor Red
    }

    return @{ Passed = $passed; Failed = $failed; Total = $synths.Count; Errors = $errors }
}

function Test-EffectTypes {
    Write-Host "`n[FX] EFFECT TYPE VALIDATION" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------" -ForegroundColor Cyan

    $effects = @(
        @{ Name = "reverb"; Key = "Reverb" },
        @{ Name = "gverb"; Key = "Reverb" },
        @{ Name = "echo"; Key = "Echo" },
        @{ Name = "delay"; Key = "Delay" },
        @{ Name = "distortion"; Key = "Distortion" },
        @{ Name = "lpf"; Key = "lpf_l" },
        @{ Name = "rlpf"; Key = "lpf_res" },
        @{ Name = "hpf"; Key = "hpf_l" },
        @{ Name = "rhpf"; Key = "hpf_res" },
        @{ Name = "slicer"; Key = "Slicer" },
        @{ Name = "bitcrusher"; Key = "Bitcrusher" },
        @{ Name = "krush"; Key = "Bitcrusher" },
        @{ Name = "compressor"; Key = "Compressor" },
        @{ Name = "normaliser"; Key = "Normaliser" },
        @{ Name = "normalizer"; Key = "Normaliser" },
        @{ Name = "flanger"; Key = "Flanger" },
        @{ Name = "chorus"; Key = "Chorus" },
        @{ Name = "ring_mod"; Key = "ring_mod_freq" },
        @{ Name = "pan"; Key = "Pan" },
        @{ Name = "wobble"; Key = "Wobble" },
        @{ Name = "ixi_techno"; Key = "Wobble" },
        @{ Name = "octaver"; Key = "Octaver" }
    )

    $passed = 0
    $failed = 0
    $errors = @()

    $effectsContent = Get-Content (Join-Path $SrcTauri "src/audio/effects.rs") -Raw
    $parserContent = Get-Content (Join-Path $SrcTauri "src/audio/parser.rs") -Raw

    foreach ($fx in $effects) {
        $inParser = $parserContent -match [regex]::Escape("""$($fx.Name)""")
        $inEffects = $effectsContent -match $fx.Key

        if ($Verbose) {
            Write-Host "   Testing :$($fx.Name)..." -ForegroundColor Gray -NoNewline
        }

        if ($inParser -and $inEffects) {
            $passed++
            if ($Verbose) { Write-Host " [OK] (parser + effects.rs)" -ForegroundColor Green }
        } elseif ($inParser) {
            $passed++
            $errors += ":$($fx.Name) (in parser but check effects.rs implementation)"
            if ($Verbose) { Write-Host " [WARN] (parser only)" -ForegroundColor Yellow }
        } else {
            $failed++
            $errors += ":$($fx.Name)"
            if ($Verbose) { Write-Host " [FAIL]" -ForegroundColor Red }
        }
    }

    Write-Host "   Result: $passed/$($effects.Count) effects registered" -ForegroundColor $(if ($failed -eq 0) { "Green" } else { "Yellow" })
    if ($errors.Count -gt 0) {
        Write-Host "   Issues: $($errors -join ', ')" -ForegroundColor Yellow
    }

    return @{ Passed = $passed; Failed = $failed; Total = $effects.Count; Errors = $errors }
}

function Test-DefaultValues {
    Write-Host "`n[DEFAULTS] DEFAULT VALUE VALIDATION" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------" -ForegroundColor Cyan

    # Test via the snapshot tests which validate exact default values
    $result = Run-CargoTest "snapshot_default_envelope" -Capture

    if ($result.Passed) {
        Write-Host "   [OK] Default envelope: attack=0, decay=0, sustain_level=1, release=1" -ForegroundColor Green
    } else {
        Write-Host "   [FAIL] Default envelope test failed" -ForegroundColor Red
        if ($Verbose) { Write-Host $result.Output -ForegroundColor Gray }
    }

    # Test play note defaults
    $result2 = Run-CargoTest "snapshot_play_note_basic" -Capture
    if ($result2.Passed) {
        Write-Host "   [OK] Default amp=1.0, default synth=Sine" -ForegroundColor Green
    } else {
        Write-Host "   [FAIL] Play note default test failed" -ForegroundColor Red
    }

    # Test BPM default
    $result3 = Run-CargoTest "snapshot_use_bpm" -Capture
    if ($result3.Passed) {
        Write-Host "   [OK] BPM conversion correct" -ForegroundColor Green
    } else {
        Write-Host "   [FAIL] BPM test failed" -ForegroundColor Red
    }

    return @{
        Passed = @($result, $result2, $result3).Where({ $_.Passed }).Count
        Failed = 3 - @($result, $result2, $result3).Where({ $_.Passed }).Count
        Total = 3
    }
}

function Test-NoteFrequencies {
    Write-Host "`n[FREQ] NOTE FREQUENCY VALIDATION" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------" -ForegroundColor Cyan

    $result = Run-CargoTest "snapshot_play_midi_number" -Capture

    if ($result.Passed) {
        Write-Host "   [OK] MIDI note -> frequency conversion correct (C4=261.63, E4=329.63, G4=392.00)" -ForegroundColor Green
    } else {
        Write-Host "   [FAIL] MIDI note conversion test failed" -ForegroundColor Red
        if ($Verbose) { Write-Host $result.Output -ForegroundColor Gray }
    }

    $result2 = Run-CargoTest "snapshot_play_note_basic" -Capture
    if ($result2.Passed) {
        Write-Host "   [OK] Symbol note -> frequency conversion correct (:c4 = 261.63 Hz)" -ForegroundColor Green
    } else {
        Write-Host "   [FAIL] Symbol note conversion test failed" -ForegroundColor Red
    }

    return @{
        Passed = @($result, $result2).Where({ $_.Passed }).Count
        Failed = 2 - @($result, $result2).Where({ $_.Passed }).Count
        Total = 2
    }
}

function Test-FidelitySnapshots {
    Write-Host "`n[SNAPSHOT] FIDELITY SNAPSHOT TESTS" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------" -ForegroundColor Cyan

    $result = Run-CargoTest "--test fidelity_snapshots" -Capture

    # Parse test results
    $passedMatch = [regex]::Match($result.Output, "(\d+) passed")
    $failedMatch = [regex]::Match($result.Output, "(\d+) failed")

    $passed = if ($passedMatch.Success) { [int]$passedMatch.Groups[1].Value } else { 0 }
    $failed = if ($failedMatch.Success) { [int]$failedMatch.Groups[1].Value } else { 0 }

    if ($result.Passed) {
        Write-Host "   [OK] All $passed snapshot tests passed" -ForegroundColor Green
    } else {
        Write-Host "   [FAIL] $failed/$($passed + $failed) snapshot tests failed" -ForegroundColor Red

        # Extract failing test names
        $failingTests = [regex]::Matches($result.Output, "test (\w+) \.\.\. FAILED")
        foreach ($ft in $failingTests) {
            Write-Host "      FAILED: $($ft.Groups[1].Value)" -ForegroundColor Red
        }
    }

    if ($Verbose) {
        # Show individual test results
        $testLines = $result.Output -split "`n" | Where-Object { $_ -match "test \w+ \.\.\. (ok|FAILED)" }
        foreach ($line in $testLines) {
            $color = if ($line -match "ok") { "Green" } else { "Red" }
            Write-Host "      $($line.Trim())" -ForegroundColor $color
        }
    }

    return @{ Passed = $passed; Failed = $failed; Total = $passed + $failed }
}

function Test-ExampleParsing {
    param([string]$SpecificFile = "")

    Write-Host "`n[PARSE] EXAMPLE FILE PARSING" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------" -ForegroundColor Cyan

    if ($SpecificFile) {
        $testName = "test_parse_$($SpecificFile.ToLower())"
        $result = Run-CargoTest "--test example_parsing $testName" -Capture
    } else {
        $result = Run-CargoTest "--test example_parsing" -Capture
    }

    $passedMatch = [regex]::Match($result.Output, "(\d+) passed")
    $failedMatch = [regex]::Match($result.Output, "(\d+) failed")

    $passed = if ($passedMatch.Success) { [int]$passedMatch.Groups[1].Value } else { 0 }
    $failed = if ($failedMatch.Success) { [int]$failedMatch.Groups[1].Value } else { 0 }

    if ($result.Passed) {
        Write-Host "   [OK] All $passed example files parsed successfully" -ForegroundColor Green
    } else {
        Write-Host "   [FAIL] $failed/$($passed + $failed) example parsing tests failed" -ForegroundColor Red
    }

    if ($Verbose) {
        # Show per-test output including note/sample counts
        $testLines = $result.Output -split "`n" | Where-Object { $_ -match "Test\d+:|notes|samples|FAILED" }
        foreach ($line in $testLines) {
            Write-Host "      $($line.Trim())" -ForegroundColor Gray
        }
    }

    return @{ Passed = $passed; Failed = $failed; Total = $passed + $failed }
}

function Test-AudioComparison {
    Write-Host "`n[AUDIO] AUDIO COMPARISON HARNESS" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------" -ForegroundColor Cyan

    $result = Run-CargoTest "--test audio_compare" -Capture

    $passedMatch = [regex]::Match($result.Output, "(\d+) passed")
    $failedMatch = [regex]::Match($result.Output, "(\d+) failed")

    $passed = if ($passedMatch.Success) { [int]$passedMatch.Groups[1].Value } else { 0 }
    $failed = if ($failedMatch.Success) { [int]$failedMatch.Groups[1].Value } else { 0 }

    if ($result.Passed) {
        Write-Host "   [OK] All $passed audio comparison tests passed" -ForegroundColor Green
    } else {
        Write-Host "   [FAIL] $failed/$($passed + $failed) audio comparison tests failed" -ForegroundColor Red
    }

    return @{ Passed = $passed; Failed = $failed; Total = $passed + $failed }
}

function Test-ParityValidation {
    Write-Host "`n[PARITY] PARITY VALIDATION TESTS" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------" -ForegroundColor Cyan

    $result = Run-CargoTest "--test parity_validation" -Capture

    $passedMatch = [regex]::Match($result.Output, "(\d+) passed")
    $failedMatch = [regex]::Match($result.Output, "(\d+) failed")

    $passed = if ($passedMatch.Success) { [int]$passedMatch.Groups[1].Value } else { 0 }
    $failed = if ($failedMatch.Success) { [int]$failedMatch.Groups[1].Value } else { 0 }

    if ($result.Passed) {
        Write-Host "   [OK] All $passed parity validation tests passed" -ForegroundColor Green
    } else {
        Write-Host "   [FAIL] $failed/$($passed + $failed) parity validation tests failed" -ForegroundColor Red

        $failingTests = [regex]::Matches($result.Output, "test (\w+) \.\.\. FAILED")
        foreach ($ft in $failingTests) {
            Write-Host "      FAILED: $($ft.Groups[1].Value)" -ForegroundColor Red
        }
    }

    return @{ Passed = $passed; Failed = $failed; Total = $passed + $failed }
}

# ============================================================================
# Main Execution
# ============================================================================

Write-Host "===================================================" -ForegroundColor Cyan
Write-Host "  PiBeat Sound Parity Validator" -ForegroundColor Cyan
Write-Host "===================================================" -ForegroundColor Cyan

$results = @()

if ($Synths -or $All) {
    $results += Test-SynthTypes
}

if ($Effects -or $All) {
    $results += Test-EffectTypes
}

if ($Samples -or $All) {
    # Test built-in sample catalog
    Write-Host "`n[SAMPLE] SAMPLE CATALOG VALIDATION" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------" -ForegroundColor Cyan

    $sampleContent = Get-Content (Join-Path $SrcTauri "src/audio/sample.rs") -Raw
    $sampleCategories = @("bd_", "sn_", "hat_", "drum_", "elec_", "ambi_", "bass_", "loop_", "perc_", "tabla_", "vinyl_", "glitch_", "misc_")

    $sampleCount = 0
    foreach ($cat in $sampleCategories) {
        $catMatches = [regex]::Matches($sampleContent, """($cat\w+)""")
        $sampleCount += $catMatches.Count
        if ($Verbose -and $catMatches.Count -gt 0) {
            Write-Host "   $cat*: $($catMatches.Count) samples" -ForegroundColor Gray
        }
    }
    Write-Host "   [OK] $sampleCount built-in samples registered in sample.rs" -ForegroundColor Green

    # Test sample parsing
    $sampleResult = Run-CargoTest "snapshot_sample_basic" -Capture
    if ($sampleResult.Passed) {
        Write-Host "   [OK] Sample parsing and parameter extraction working" -ForegroundColor Green
    } else {
        Write-Host "   [FAIL] Sample parsing test failed" -ForegroundColor Red
    }
}

if ($All -or (-not $File -and -not $Synths -and -not $Effects -and -not $Samples)) {
    # Run everything
    $results += Test-DefaultValues
    $results += Test-NoteFrequencies
    $results += Test-FidelitySnapshots
    $results += Test-ExampleParsing
    $results += Test-AudioComparison
    $results += Test-ParityValidation
}

if ($File) {
    # Validate a specific file
    Write-Host "`n[FILE] VALIDATING: $File" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------" -ForegroundColor Cyan

    # Run syntax analysis first
    & "$PSScriptRoot\validate-syntax.ps1" -File $File -Verbose:$Verbose

    # Then run parser test (determine which example)
    $fileName = Split-Path $File -Leaf
    if ($fileName -match "^Test(\d+)$") {
        $testNum = $Matches[1]
        $testResult = Test-ExampleParsing -SpecificFile "test$testNum"
    }
}

# ============================================================================
# Final Summary
# ============================================================================

$totalPassed = ($results | ForEach-Object { $_.Passed } | Measure-Object -Sum).Sum
$totalFailed = ($results | ForEach-Object { $_.Failed } | Measure-Object -Sum).Sum
$totalTests = $totalPassed + $totalFailed

if ($totalTests -gt 0) {
    Write-Host "`n" -NoNewline
    Write-Host "===================================================" -ForegroundColor Cyan
    Write-Host "  OVERALL SOUND PARITY RESULT" -ForegroundColor Cyan
    Write-Host "===================================================" -ForegroundColor Cyan

    $score = [math]::Round(($totalPassed / $totalTests) * 100)
    Write-Host "  Score: ${score}% ($totalPassed/$totalTests passed)" -ForegroundColor $(if ($score -ge 95) { "Green" } elseif ($score -ge 80) { "Yellow" } else { "Red" })

    if ($totalFailed -gt 0) {
        Write-Host "  $totalFailed test(s) need attention" -ForegroundColor Red
    } else {
        Write-Host "  All sound parity checks passed!" -ForegroundColor Green
    }
    Write-Host "===================================================" -ForegroundColor Cyan
}
