# PiBeat Syntax Validation Script
# Analyzes a Sonic Pi test file and reports which constructs are used,
# which are supported by PiBeat, and which need implementation.
#
# Usage: .\scripts\validate-syntax.ps1 -File <path-to-sonic-pi-file>
#        .\scripts\validate-syntax.ps1 -File examples\Test1 -Fix
#        .\scripts\validate-syntax.ps1 -All  (validates all examples/)

param(
    [string]$File,
    [switch]$All,
    [switch]$Fix,
    [switch]$Verbose,
    [switch]$Json
)

$ErrorActionPreference = "Continue"

# ============================================================================
# Sonic Pi Construct Definitions
# ============================================================================

$SupportedConstructs = @{
    # --- Fully Supported ---
    "play :note"               = @{ Status = "full"; Pattern = "(?m)^\s*play\s+[:\d]" }
    "play chord()"             = @{ Status = "full"; Pattern = "play\s+chord\s*\(" }
    "play_pattern_timed"       = @{ Status = "full"; Pattern = "play_pattern_timed" }
    "play_pattern"             = @{ Status = "full"; Pattern = "(?<!\w)play_pattern(?!_timed)" }
    "sample :name"             = @{ Status = "full"; Pattern = '(?m)^\s*sample\s+[:"\/]' }
    "sleep"                    = @{ Status = "full"; Pattern = "(?m)^\s*sleep\s+" }
    "wait"                     = @{ Status = "full"; Pattern = "(?m)^\s*wait\s+" }
    "use_bpm"                  = @{ Status = "full"; Pattern = "use_bpm\s+" }
    "use_synth"                = @{ Status = "full"; Pattern = "use_synth\s+" }
    "synth :name"              = @{ Status = "full"; Pattern = "(?m)^\s*synth\s+:" }
    "use_synth_defaults"       = @{ Status = "full"; Pattern = "use_synth_defaults" }
    "use_sample_defaults"      = @{ Status = "full"; Pattern = "use_sample_defaults" }
    "with_fx"                  = @{ Status = "full"; Pattern = "with_fx\s+:" }
    "live_loop"                = @{ Status = "full"; Pattern = "live_loop\s+:" }
    "loop do"                  = @{ Status = "full"; Pattern = "(?m)^\s*loop\s+do" }
    "N.times do"               = @{ Status = "full"; Pattern = "\d+\.times\s+do" }
    "in_thread"                = @{ Status = "full"; Pattern = "in_thread" }
    "define :name"             = @{ Status = "full"; Pattern = "define\s+:" }
    "if/elsif/else"            = @{ Status = "full"; Pattern = "(?m)^\s*(if|elsif|else|unless)\s" }
    "while"                    = @{ Status = "full"; Pattern = "(?m)^\s*while\s+" }
    ".each do"                 = @{ Status = "full"; Pattern = "\.each\s+do" }
    ".each_with_index"         = @{ Status = "full"; Pattern = "\.each_with_index" }
    "variable assignment"      = @{ Status = "full"; Pattern = "(?m)^\s*\w+\s*=\s*[^=]" }
    "ring()"                   = @{ Status = "full"; Pattern = "(?<!\w)ring\s*\(" }
    "spread()"                 = @{ Status = "full"; Pattern = "spread\s*\(" }
    "knit()"                   = @{ Status = "full"; Pattern = "knit\s*\(" }
    "range()"                  = @{ Status = "full"; Pattern = "(?<!\w)range\s*\(" }
    "choose()"                 = @{ Status = "full"; Pattern = "(?<!\.)choose\s*\(" }
    ".choose"                  = @{ Status = "full"; Pattern = "\.choose(?!\()" }
    "scale()"                  = @{ Status = "full"; Pattern = "scale\s*\(" }
    "rrand()"                  = @{ Status = "full"; Pattern = "rrand\s*\(" }
    "rrand_i()"                = @{ Status = "full"; Pattern = "rrand_i\s*\(" }
    "rand()"                   = @{ Status = "full"; Pattern = "(?<!\w)rand\s*\(" }
    "rand_i()"                 = @{ Status = "full"; Pattern = "rand_i\s*\(" }
    "dice()"                   = @{ Status = "full"; Pattern = "dice\s*\(" }
    "one_in()"                 = @{ Status = "full"; Pattern = "one_in\s*\(" }
    "use_random_seed"          = @{ Status = "full"; Pattern = "use_random_seed" }
    "at block"                 = @{ Status = "full"; Pattern = "(?m)^\s*at\s+\[" }
    "time_warp"                = @{ Status = "full"; Pattern = "time_warp" }
    "with_synth"               = @{ Status = "full"; Pattern = "with_synth\s+:" }
    "with_bpm"                 = @{ Status = "full"; Pattern = "with_bpm\s+" }
    "with_bpm_mul"             = @{ Status = "full"; Pattern = "with_bpm_mul" }
    "set/get"                  = @{ Status = "full"; Pattern = "(?m)^\s*set\s+:|get\s*\(" }
    "puts/print"               = @{ Status = "full"; Pattern = "(?m)^\s*(puts|print)\s+" }
    "stop"                     = @{ Status = "full"; Pattern = "(?m)^\s*stop\s*$" }
    "next"                     = @{ Status = "full"; Pattern = "(?m)^\s*next\s*$" }
    # Sample params
    "sample amp:"              = @{ Status = "full"; Pattern = "sample\s+.*\bamp:" }
    "sample rate:"             = @{ Status = "full"; Pattern = "sample\s+.*\brate:" }
    "sample pan:"              = @{ Status = "full"; Pattern = "sample\s+.*\bpan:" }
    "sample pitch:"            = @{ Status = "full"; Pattern = "sample\s+.*\bpitch:" }
    "sample rpitch:"           = @{ Status = "full"; Pattern = "sample\s+.*\brpitch:" }
    "sample sustain:"          = @{ Status = "full"; Pattern = "sample\s+.*\bsustain:" }
    "sample beat_stretch:"     = @{ Status = "full"; Pattern = "sample\s+.*\bbeat_stretch:" }
    "sample start:"            = @{ Status = "full"; Pattern = "sample\s+.*\bstart:" }
    "sample finish:"           = @{ Status = "full"; Pattern = "sample\s+.*\bfinish:" }
    # Play params
    "play amp:"                = @{ Status = "full"; Pattern = "play\s+.*\bamp:" }
    "play attack:"             = @{ Status = "full"; Pattern = "play\s+.*\battack:" }
    "play decay:"              = @{ Status = "full"; Pattern = "play\s+.*\bdecay:" }
    "play sustain:"            = @{ Status = "full"; Pattern = "play\s+.*\bsustain:" }
    "play release:"            = @{ Status = "full"; Pattern = "play\s+.*\brelease:" }
    "play pan:"                = @{ Status = "full"; Pattern = "play\s+.*\bpan:" }
    "play cutoff:"             = @{ Status = "full"; Pattern = "play\s+.*\bcutoff:" }
    "play res:"                = @{ Status = "full"; Pattern = "play\s+.*\bres:" }

    # --- Partially Supported ---
    ".tick"                    = @{ Status = "partial"; Pattern = "\.tick" ; Note = "Ring cycling approximated probabilistically" }
    ".look"                    = @{ Status = "partial"; Pattern = "\.look" ; Note = "Ring cycling approximated probabilistically" }
    "cue"                      = @{ Status = "partial"; Pattern = "(?m)^\s*cue\s+" ; Note = "Parsed but no-op (no sync)" }
    "sync"                     = @{ Status = "partial"; Pattern = "(?m)^\s*sync\s+" ; Note = "Parsed but no-op (no sync)" }
    "sync: on live_loop"       = @{ Status = "partial"; Pattern = "live_loop\s+:.*sync:" ; Note = "Parsed but ignored" }
    "control"                  = @{ Status = "partial"; Pattern = "(?m)^\s*control\s+" ; Note = "Parsed but no-op" }
    "sample lpf:"              = @{ Status = "partial"; Pattern = "sample\s+.*\blpf:" ; Note = "Parsed but not applied (use with_fx :lpf)" }

    # --- Not Supported ---
    "lambda/proc"              = @{ Status = "none"; Pattern = "lambda\s*\{|proc\s*\{|\.call" ; Note = "Ruby lambda/proc not supported" }
    "Time.now"                 = @{ Status = "none"; Pattern = "Time\.now" ; Note = "Returns 0.0 (Ruby runtime)" }
    "with_swing"               = @{ Status = "none"; Pattern = "with_swing" ; Note = "Not implemented" }
    "def method"               = @{ Status = "none"; Pattern = "(?m)^\s*def\s+\w+\s*\(" ; Note = "Use define :name instead" }
    "should_stop?"             = @{ Status = "none"; Pattern = "should_stop\?" ; Note = "Ruby method defs not supported" }
    "midi/midi_note_on"        = @{ Status = "none"; Pattern = "(?m)^\s*midi\s+" ; Note = "MIDI output not supported" }
    ".to_i/.to_f"              = @{ Status = "partial"; Pattern = "\.to_[if]" ; Note = "Basic numeric conversion" }
    ".floor/.ceil"             = @{ Status = "full"; Pattern = "\.(floor|ceil|round|abs)" }
}

# Known synth types
$SupportedSynths = @(
    "sine", "beep", "saw", "square", "triangle", "noise", "pulse", "super_saw",
    "dsaw", "dpulse", "dtri", "fm", "mod_fm", "mod_sine", "mod_saw", "mod_dsaw",
    "mod_tri", "mod_pulse", "tb303", "prophet", "zawa", "blade", "tech_saws",
    "hoover", "pluck", "piano", "pretty_bell", "dull_bell", "hollow",
    "dark_ambience", "growl", "chip_lead", "chip_bass", "chip_noise",
    "bnoise", "pnoise", "gnoise", "cnoise", "sub_pulse", "gabber_kick",
    "gabberkick", "supersaw"
)

# Known effect types
$SupportedEffects = @(
    "reverb", "gverb", "echo", "delay", "distortion", "lpf", "rlpf",
    "hpf", "rhpf", "slicer", "bitcrusher", "krush", "compressor",
    "normaliser", "normalizer", "flanger", "chorus", "ring_mod", "pan",
    "wobble", "ixi_techno", "octaver"
)

# Known built-in samples (categories)
$SampleCategories = @(
    "bd_", "sn_", "hat_", "drum_", "elec_", "ambi_", "bass_", "loop_",
    "perc_", "tabla_", "vinyl_", "glitch_", "misc_", "mehackit_"
)

# ============================================================================
# Analysis Functions
# ============================================================================

function Analyze-File {
    param([string]$FilePath)

    if (-not (Test-Path $FilePath)) {
        Write-Host "ERROR: File not found: $FilePath" -ForegroundColor Red
        return $null
    }

    $content = Get-Content $FilePath -Raw
    $lines = Get-Content $FilePath
    $fileName = Split-Path $FilePath -Leaf

    $report = @{
        File = $fileName
        Path = $FilePath
        Lines = $lines.Count
        Characters = $content.Length
        Constructs = @{
            Supported = @()
            Partial = @()
            Unsupported = @()
        }
        Synths = @{
            Used = @()
            Unsupported = @()
        }
        Effects = @{
            Used = @()
            Unsupported = @()
        }
        Samples = @{
            BuiltIn = @()
            External = @()
        }
        Issues = @()
        Warnings = @()
    }

    # --- Detect used constructs ---
    foreach ($construct in $SupportedConstructs.GetEnumerator()) {
        $name = $construct.Key
        $info = $construct.Value

        $matches = [regex]::Matches($content, $info.Pattern)
        if ($matches.Count -gt 0) {
            $entry = @{
                Name = $name
                Count = $matches.Count
                Status = $info.Status
                Note = $info.Note
            }

            switch ($info.Status) {
                "full"    { $report.Constructs.Supported += $entry }
                "partial" { $report.Constructs.Partial += $entry; $report.Warnings += "[WARN] '$name' used $($matches.Count)x - $($info.Note)" }
                "none"    { $report.Constructs.Unsupported += $entry; $report.Issues += "[FAIL] '$name' used $($matches.Count)x - $($info.Note)" }
            }
        }
    }

    # --- Detect synth types ---
    $synthMatches = [regex]::Matches($content, "(?:use_synth|synth|with_synth)\s+:(\w+)")
    foreach ($m in $synthMatches) {
        $synthName = $m.Groups[1].Value.ToLower()
        if ($report.Synths.Used -notcontains $synthName) {
            $report.Synths.Used += $synthName
        }
        if ($SupportedSynths -notcontains $synthName) {
            if ($report.Synths.Unsupported -notcontains $synthName) {
                $report.Synths.Unsupported += $synthName
                $report.Issues += "[FAIL] Unsupported synth: :$synthName"
            }
        }
    }

    # --- Detect effect types ---
    $fxMatches = [regex]::Matches($content, "with_fx\s+:(\w+)")
    foreach ($m in $fxMatches) {
        $fxName = $m.Groups[1].Value.ToLower()
        if ($report.Effects.Used -notcontains $fxName) {
            $report.Effects.Used += $fxName
        }
        if ($SupportedEffects -notcontains $fxName) {
            if ($report.Effects.Unsupported -notcontains $fxName) {
                $report.Effects.Unsupported += $fxName
                $report.Issues += "[FAIL] Unsupported effect: :$fxName"
            }
        }
    }

    # --- Detect sample usage ---
    $sampleMatches = [regex]::Matches($content, 'sample\s+:(\w+)')
    foreach ($m in $sampleMatches) {
        $sampleName = $m.Groups[1].Value
        if ($report.Samples.BuiltIn -notcontains $sampleName) {
            $report.Samples.BuiltIn += $sampleName
        }
    }

    $extSampleMatches = [regex]::Matches($content, 'sample\s+"([^"]+)"')
    foreach ($m in $extSampleMatches) {
        $samplePath = $m.Groups[1].Value
        if ($report.Samples.External -notcontains $samplePath) {
            $report.Samples.External += $samplePath
        }
    }

    # Also check for concatenated paths
    $concatSamples = [regex]::Matches($content, 'sample\s+\w+\s*\+\s*"([^"]+)"')
    foreach ($m in $concatSamples) {
        $sampleFile = $m.Groups[1].Value
        if ($report.Samples.External -notcontains $sampleFile) {
            $report.Samples.External += $sampleFile
        }
    }

    # --- Check for duplicate live_loop names ---
    $loopNames = [regex]::Matches($content, "live_loop\s+:(\w+)")
    $loopNameList = @{}
    foreach ($m in $loopNames) {
        $name = $m.Groups[1].Value
        if ($loopNameList.ContainsKey($name)) {
            $loopNameList[$name]++
        } else {
            $loopNameList[$name] = 1
        }
    }
    foreach ($entry in $loopNameList.GetEnumerator()) {
        if ($entry.Value -gt 1) {
            $report.Warnings += "[WARN] live_loop :$($entry.Key) defined $($entry.Value) times (later defs override)"
        }
    }

    # --- Check for loops without sleep ---
    $loopBlocks = [regex]::Matches($content, "((?:live_loop|loop)\s+.*?do\s*\n)(.*?)(?=\nend)", [System.Text.RegularExpressions.RegexOptions]::Singleline)
    foreach ($m in $loopBlocks) {
        $loopBody = $m.Groups[2].Value
        if ($loopBody -notmatch "sleep|wait") {
            $report.Issues += "[WARN] Loop without sleep detected - may cause infinite loop"
        }
    }

    # --- Check for advanced Ruby constructs ---
    if ($content -match "class\s+\w+") {
        $report.Issues += "[FAIL] Ruby class definitions not supported"
    }
    if ($content -match "require\s+") {
        $report.Warnings += "[WARN] 'require' statements ignored"
    }
    if ($content -match "module\s+\w+") {
        $report.Issues += "[FAIL] Ruby module definitions not supported"
    }

    return $report
}

function Format-Report {
    param($Report)

    Write-Host ""
    Write-Host "===================================================" -ForegroundColor Cyan
    Write-Host "  Syntax Analysis: $($Report.File)" -ForegroundColor Cyan
    Write-Host "  $($Report.Lines) lines, $($Report.Characters) characters" -ForegroundColor Gray
    Write-Host "===================================================" -ForegroundColor Cyan

    # Supported
    Write-Host "`n[OK] SUPPORTED CONSTRUCTS ($($Report.Constructs.Supported.Count)):" -ForegroundColor Green
    foreach ($c in ($Report.Constructs.Supported | Sort-Object { $_.Name })) {
        if ($Verbose) {
            Write-Host "   [OK] $($c.Name) ($($c.Count)x)" -ForegroundColor Green
        }
    }
    if (-not $Verbose -and $Report.Constructs.Supported.Count -gt 0) {
        $names = ($Report.Constructs.Supported | Sort-Object { $_.Name } | ForEach-Object { $_.Name }) -join ", "
        Write-Host "   $names" -ForegroundColor Gray
    }

    # Partial
    if ($Report.Constructs.Partial.Count -gt 0) {
        Write-Host "`n[WARN] PARTIAL SUPPORT ($($Report.Constructs.Partial.Count)):" -ForegroundColor Yellow
        foreach ($c in $Report.Constructs.Partial) {
            Write-Host "   [WARN] $($c.Name) ($($c.Count)x) - $($c.Note)" -ForegroundColor Yellow
        }
    }

    # Unsupported
    if ($Report.Constructs.Unsupported.Count -gt 0) {
        Write-Host "`n[FAIL] UNSUPPORTED ($($Report.Constructs.Unsupported.Count)):" -ForegroundColor Red
        foreach ($c in $Report.Constructs.Unsupported) {
            Write-Host "   [FAIL] $($c.Name) ($($c.Count)x) - $($c.Note)" -ForegroundColor Red
        }
    }

    # Synths
    Write-Host "`n[SYNTH] SYNTHS USED ($($Report.Synths.Used.Count)):" -ForegroundColor Magenta
    foreach ($s in $Report.Synths.Used) {
        $status = if ($SupportedSynths -contains $s) { "[OK]" } else { "[FAIL]" }
        Write-Host "   $status :$s" -ForegroundColor $(if ($status -eq "[OK]") { "Green" } else { "Red" })
    }

    # Effects
    Write-Host "`n[FX] EFFECTS USED ($($Report.Effects.Used.Count)):" -ForegroundColor Magenta
    foreach ($f in $Report.Effects.Used) {
        $status = if ($SupportedEffects -contains $f) { "[OK]" } else { "[FAIL]" }
        Write-Host "   $status :$f" -ForegroundColor $(if ($status -eq "[OK]") { "Green" } else { "Red" })
    }

    # Samples
    Write-Host "`n[SAMPLE] SAMPLES:" -ForegroundColor Magenta
    Write-Host "   Built-in: $($Report.Samples.BuiltIn.Count)" -ForegroundColor Gray
    Write-Host "   External: $($Report.Samples.External.Count)" -ForegroundColor Gray
    if ($Verbose) {
        foreach ($s in $Report.Samples.BuiltIn) { Write-Host "     :$s" -ForegroundColor Gray }
        foreach ($s in $Report.Samples.External) { Write-Host "     $s" -ForegroundColor Gray }
    }

    # Issues
    if ($Report.Issues.Count -gt 0) {
        Write-Host "`n[ERROR] ISSUES ($($Report.Issues.Count)):" -ForegroundColor Red
        foreach ($i in $Report.Issues) {
            Write-Host "   $i" -ForegroundColor Red
        }
    }

    # Warnings
    if ($Report.Warnings.Count -gt 0) {
        Write-Host "`n[WARN] WARNINGS ($($Report.Warnings.Count)):" -ForegroundColor Yellow
        foreach ($w in $Report.Warnings) {
            Write-Host "   $w" -ForegroundColor Yellow
        }
    }

    # Summary
    $totalConstructs = $Report.Constructs.Supported.Count + $Report.Constructs.Partial.Count + $Report.Constructs.Unsupported.Count
    $coverage = if ($totalConstructs -gt 0) { [math]::Round(($Report.Constructs.Supported.Count / $totalConstructs) * 100) } else { 100 }

    Write-Host "`n---------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  PARITY SCORE: ${coverage}% ($($Report.Constructs.Supported.Count)/$totalConstructs constructs fully supported)" -ForegroundColor $(if ($coverage -ge 90) { "Green" } elseif ($coverage -ge 70) { "Yellow" } else { "Red" })
    Write-Host "  Issues: $($Report.Issues.Count)  |  Warnings: $($Report.Warnings.Count)" -ForegroundColor Gray
    Write-Host "---------------------------------------------------" -ForegroundColor Cyan
}

# ============================================================================
# Main Execution
# ============================================================================

if ($All) {
    $files = Get-ChildItem "examples/Test*" | Select-Object -ExpandProperty FullName
    if ($files.Count -eq 0) {
        Write-Host "No example files found in examples/" -ForegroundColor Red
        exit 1
    }

    $allReports = @()
    foreach ($f in $files) {
        $report = Analyze-File -FilePath $f
        if ($report) {
            $allReports += $report
            Format-Report -Report $report
        }
    }

    # Summary across all files
    Write-Host "`n" -NoNewline
    Write-Host "===================================================" -ForegroundColor Cyan
    Write-Host "  OVERALL SUMMARY ($($allReports.Count) files)" -ForegroundColor Cyan
    Write-Host "===================================================" -ForegroundColor Cyan
    $totalIssues = ($allReports | ForEach-Object { $_.Issues.Count } | Measure-Object -Sum).Sum
    $totalWarnings = ($allReports | ForEach-Object { $_.Warnings.Count } | Measure-Object -Sum).Sum
    Write-Host "  Total Issues: $totalIssues" -ForegroundColor $(if ($totalIssues -eq 0) { "Green" } else { "Red" })
    Write-Host "  Total Warnings: $totalWarnings" -ForegroundColor $(if ($totalWarnings -eq 0) { "Green" } else { "Yellow" })

} elseif ($File) {
    $report = Analyze-File -FilePath $File
    if ($report) {
        if ($Json) {
            $report | ConvertTo-Json -Depth 5
        } else {
            Format-Report -Report $report
        }
    }
} else {
    Write-Host "Usage:" -ForegroundColor Yellow
    Write-Host "  .\scripts\validate-syntax.ps1 -File <path>"
    Write-Host "  .\scripts\validate-syntax.ps1 -All"
    Write-Host "  .\scripts\validate-syntax.ps1 -File examples\Test1 -Verbose"
    Write-Host "  .\scripts\validate-syntax.ps1 -File examples\Test1 -Json"
}
