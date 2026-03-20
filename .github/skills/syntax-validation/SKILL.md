---
name: syntax-validation
description: "Validates PiBeat's Sonic Pi syntax parsing and compilation. Use when asked to check if Sonic Pi code parses correctly, find unsupported constructs, test parser coverage, analyze .rb fixture files, verify ParsedCommand output, or debug parsing failures. Covers all Sonic Pi DSL constructs: play, sample, sleep, synths, effects, loops, threads, functions, randomization, rings, spreads, conditionals, and control flow."
---

# Sonic Pi Syntax Validation Skill

Validates that PiBeat's Rust parser correctly handles all Sonic Pi DSL constructs — from basic `play :c4` to complex nested `live_loop` with `with_fx`, randomization, rings, and control flow.

## When to Use This Skill

- Checking if a specific Sonic Pi construct is parsed correctly by PiBeat
- Finding which Sonic Pi features are NOT yet supported
- Analyzing a `.rb` fixture file for construct coverage
- Debugging why a Sonic Pi code snippet produces no events or wrong events
- Verifying that `parse_code()` → `Vec<ParsedCommand>` output is correct
- Adding support for a new Sonic Pi syntax construct
- Running the syntax validation scripts

## Prerequisites

- Rust toolchain installed (`cargo` available)
- PowerShell available for running validation scripts
- Workspace rooted at the PiBeat project directory

## Step-by-Step Workflows

### Workflow 1: Validate a Single Sonic Pi File

1. Run syntax analysis:
   ```powershell
   .\scripts\validate-syntax.ps1 -File <path> -Verbose
   ```
2. Review output for:
   - **Supported constructs** (green) — these parse correctly
   - **Partial constructs** (yellow) — parsed but limited runtime behavior
   - **Unsupported constructs** (red) — will be ignored or cause errors
   - **Issues** — loops without sleep, duplicate loop names, etc.
3. If issues found, check whether `parse_line()` in `src-tauri/src/audio/parser.rs` has a match arm

### Workflow 2: Full Syntax Coverage Scan

1. Run against all example files:
   ```powershell
   .\scripts\validate-syntax.ps1 -All -Verbose
   ```
2. For JSON-parseable output:
   ```powershell
   .\scripts\validate-syntax.ps1 -All -Json
   ```
3. Review aggregate statistics: total constructs used vs. supported

### Workflow 3: Test Parser with Rust Tests

1. Run the parity validation test suite:
   ```bash
   cd src-tauri && cargo test --test parity_validation -- --nocapture
   ```
2. Run fidelity snapshot tests (compares parsed event streams against golden JSON):
   ```bash
   cargo test --test fidelity_snapshots -- --nocapture
   ```
3. Run example parsing tests:
   ```bash
   cargo test --test example_parsing -- --nocapture
   ```

### Workflow 4: Add Support for a New Construct

1. **Create a fixture**: Write a `.rb` file in `fidelity/fixtures/` with the construct
2. **Write a snapshot test**: Add a test in `src-tauri/tests/fidelity_snapshots.rs` that calls `parse_code()` and asserts on the `ParsedCommand` output
3. **Implement parsing**: Add a match arm in `parse_line()` in `parser.rs`
4. **Add parity test**: Add a test in `src-tauri/tests/parity_validation.rs`
5. **Verify**: Run `cargo test` and ensure all tests pass
6. **Update matrix**: Update `parity/PARITY_MATRIX.md` with new status

## Sonic Pi Construct Reference

### Fully Supported Constructs
| Construct | Example | Parser Function |
|-----------|---------|----------------|
| Play note | `play :c4` | `parse_line()` → `PlayNote` |
| Play MIDI | `play 60` | `parse_line()` → `PlayNote` |
| Play with params | `play :c4, amp: 0.5` | `parse_line()` → `PlayNote` |
| Play chord | `play chord(:c4, :major)` | `parse_play_chord()` → `PlayChord` |
| Play pattern | `play_pattern_timed [...], [...]` | `parse_play_pattern_timed()` → `PlayPatternTimed` |
| Sample | `sample :kick` | `parse_line()` → `PlaySample` |
| Sleep | `sleep 0.5` | `parse_line()` → `Sleep` |
| Use synth | `use_synth :saw` | `parse_line()` → `UseSynth` |
| Use BPM | `use_bpm 120` | `parse_line()` → `UseBpm` |
| With FX | `with_fx :reverb do` | `parse_line()` → `WithFx` |
| Live loop | `live_loop :beat do` | `parse_line()` → `LiveLoop` |
| Loop | `loop do` | `parse_line()` → `Loop` |
| Times | `4.times do` | `parse_line()` → `Times` |
| In thread | `in_thread do` | `parse_line()` → `InThread` |
| Define | `define :melody do` | `parse_line()` → `Define` |
| Variables | `x = 42` | `parse_line()` → `Variable` |
| Ring | `ring(:c4, :e4, :g4)` | `parse_line()` → `Ring` |
| Spread | `spread(3, 8)` | `parse_line()` → `Spread` |
| Choose | `choose([:c4, :e4])` | `parse_line()` → `Choose` |
| Scale | `scale(:c4, :major)` | `parse_line()` → `Scale` |
| Randomization | `rrand(0.5, 1.0)` | `parse_line()` → `Rrand` |
| Conditionals | `if one_in(3) do` | `parse_line()` → `If` |
| At block | `at [0, 1, 2] do` | `parse_line()` → `AtBlock` |
| Time warp | `time_warp 0.5 do` | `parse_line()` → `TimeWarp` |
| Set/Get | `set :x, 42` / `get(:x)` | `parse_line()` → `Set` / `Get` |

### Partially Supported
| Construct | Issue | Workaround |
|-----------|-------|------------|
| `.tick` / `.look` | Ring cycling approximated | Use explicit note sequencing |
| `cue` / `sync` | Parsed but no-op | Use separate `live_loop` blocks |
| `control` | Parsed but no-op | Use explicit notes with timing |

### Not Supported
| Construct | Reason |
|-----------|--------|
| `lambda` / `proc` / `.call` | Ruby runtime feature |
| `Time.now` | Ruby runtime feature |
| `with_swing` | Not implemented |
| `def method()` | Use `define :name` instead |
| `midi` / `midi_note_on` | MIDI output not planned |

## Troubleshooting

| Problem | Cause | Solution |
|---------|-------|----------|
| Code produces no events | `parse_line()` has no match arm | Add matching logic in `parser.rs` |
| Wrong ParsedCommand variant | Pattern match too broad/narrow | Refine regex or condition in `parse_line()` |
| Snapshot test fails | Parser output changed | Run with `--nocapture`, compare old vs. new JSON |
| "Unknown synth" warning | `parse_synth_name()` missing entry | Add synth mapping in `parser.rs` |
| Parameters ignored | Param extraction not implemented | Check `parse_params()` in `parser.rs` |
| Nested blocks broken | Block depth tracking wrong | Check `do`/`end` counter in parser state |

## Key Files

| File | Role |
|------|------|
| `src-tauri/src/audio/parser.rs` | Main parser — all syntax handling |
| `src-tauri/tests/parity_validation.rs` | Syntax + semantic parity tests |
| `src-tauri/tests/fidelity_snapshots.rs` | Golden JSON snapshot tests |
| `src-tauri/tests/example_parsing.rs` | Example file parsing tests |
| `fidelity/fixtures/*.rb` | Sonic Pi test fixtures |
| `fidelity/event_stream/*.json` | Golden event stream snapshots |
| `scripts/validate-syntax.ps1` | PowerShell syntax analysis |
| `parity/PARITY_MATRIX.md` | Feature support matrix |
