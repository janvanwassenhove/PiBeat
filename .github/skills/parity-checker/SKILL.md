---
name: parity-checker
description: "Deep parity analysis and auto-fix for PiBeat's Sonic Pi implementation. Use when asked to check, diagnose, or fix sound parity between PiBeat and the original Sonic Pi IDE. Covers synths, effects, samples, language constructs, audio engine parameters, envelope shapes, filter behavior, timing accuracy, and automated code corrections."
---

# Parity Checker Skill

Analyzes PiBeat code for full Sonic Pi compatibility and automatically suggests or applies fixes to achieve sound parity. This is the comprehensive skill for ensuring PiBeat produces identical output to Sonic Pi.

## When to Use This Skill

- User asks "check parity", "sound parity", "compatibility check"
- User reports their code sounds different from Sonic Pi
- User wants to ensure their Sonic Pi code works identically in PiBeat
- User asks to "fix parity" or "make it sound like Sonic Pi"
- User wants a detailed compatibility report
- Validating new parser/engine features match Sonic Pi behavior
- When generating code that must have 100% Sonic Pi fidelity

## Architecture

### Backend (Rust)
The `validate_parity` Tauri command in `src-tauri/src/lib.rs` performs deep analysis:

1. **Parse** — `validate_and_parse()` converts code to `ParsedCommand` tree
2. **Collect Usage** — `collect_usage()` recursively extracts all features used
3. **Convert** — `commands_to_audio()` generates timed audio events
4. **Classify** — Each feature is classified as supported/partial/unsupported
5. **Suggest** — Specific fix suggestions with replacement code
6. **Score** — Overall parity score 0-100%

Returns `ParityReport` with:
- `score` (0.0-1.0)
- `categories[]` (Synths, Effects, Sample Features, Language Constructs, Audio Output)
- `suggestions[]` with severity, message, and optional fix code
- `warnings[]` from parser

### Frontend (TypeScript)
The agent system (`src/agent.ts`) handles parity intents:

- **`parity_check`** — Full parity analysis (all categories)
- **`parity_fix`** — Auto-apply workarounds for unsupported features
- **`parity_synths`** — Synth-focused compatibility check
- **`parity_effects`** — Effects-focused compatibility check
- **`parity_samples`** — Sample parameter compatibility check

Falls back to client-side static analysis when the Rust backend is unavailable.

### LLM Integration (`src/llm.ts`)
The system context includes comprehensive parity knowledge:
- All 42 supported synths with oscillator types
- All 22 effects with default parameter values
- Sample parameter support matrix
- Language construct support levels
- Effect defaults reference table
- Audio engine specifications

## Step-by-Step Workflows

### Workflow 1: Full Parity Check (User Request)

1. User clicks "Parity check" quick action or asks "check parity of my code"
2. Agent detects `parity_check` intent
3. Invokes `validate_parity` Tauri command with current buffer code
4. Formats `ParityReport` into readable markdown with:
   - Score percentage with color indicator
   - Category breakdowns (synths, effects, samples, constructs)
   - Specific suggestions with code fixes
   - Parse warnings
5. Returns formatted response to user

### Workflow 2: Auto-Fix Parity Issues

1. User clicks "Fix parity issues" or asks "fix parity"
2. Agent detects `parity_fix` intent
3. Applies automatic fixes:
   - Replace unsupported FX with alternatives
   - Convert Ruby `def` to `define`
   - Flag `control` usage with workaround code
   - Note sync/cue limitations
4. Returns modified code with change list

### Workflow 3: LLM-Assisted Deep Parity Analysis

1. User asks parity question with LLM provider active (OpenAI/Anthropic)
2. System context includes full parity knowledge
3. `buildUserPrompt()` detects parity request and injects analysis instructions
4. LLM analyzes code against comprehensive feature matrix
5. Generates detailed report with corrected code

### Workflow 4: Synth Parity Validation

For each synth used in code:
1. Check against 42 supported `OscillatorType` variants
2. Verify oscillator algorithm matches Sonic Pi's SuperCollider SynthDefs:
   - Basic waveforms: PolyBLEP anti-aliased (saw, square, triangle)
   - FM synths: Carrier/modulator with correct depth/divisor
   - Classic synths: TB303 (saw→SVF), Prophet (dual detuned)
   - Physical models: Pluck (Karplus-Strong)
3. Check synth-specific parameters (cutoff, res, detune, depth, etc.)

### Workflow 5: Effect Parity Validation

For each effect used:
1. Check against 22 supported effect types
2. Verify parameter defaults match Sonic Pi v4.x:
   - reverb: mix=0.4, room=0.6, damp=0.5
   - echo/delay: phase=0.25 beats, decay=2, mix=1
   - distortion: distort=0.5, mix=1
   - lpf/rlpf: cutoff=100 MIDI, res=0
   - hpf/rhpf: cutoff=60 MIDI, res=0
   - bitcrusher: bits=10, sr=10000
3. Verify processing order: distortion → LPF → HPF → slicer → bitcrusher → compressor → flanger → chorus → ring_mod → pan → wobble → octaver → delay → reverb → normaliser
4. Check parameter conversions:
   - Echo phase: beats → seconds (phase_secs = phase_beats × 60 / bpm)
   - Filter cutoff: MIDI → Hz (hz = 440 × 2^((midi-69)/12))
   - Resonance: Q = 0.7071 + res × 19.3

### Workflow 6: Sample Feature Validation

Check sample parameters against supported features:
| Parameter | Status | Implementation |
|-----------|--------|----------------|
| amp | ✅ Full | Direct multiplier |
| rate | ✅ Full | Cubic Hermite interpolation |
| pan | ✅ Full | Equal-power cosine pan |
| pitch/rpitch | ✅ Full | Via rate = rate × 2^(pitch/12) |
| sustain | ✅ Full | Truncates to N beats |
| beat_stretch | ✅ Full | rate = sample_dur / (beats × 60/bpm) |
| start/finish | ✅ Full | Normalized 0.0-1.0 trimming |
| lpf/hpf | ✅ Full | Per-voice FX chain |
| attack/decay/sustain_level/release | ✅ Full | ADSR envelope |

## Parity Classification Reference

### Fully Supported Synths (42)
sine, saw, square, triangle, noise, pulse, super_saw, tb303, prophet, blade, pluck, fm, beep, dark_ambience, hollow, growl, pretty_bell, dull_bell, chip_lead, chip_bass, chip_noise, tech_saws, hoover, zawa, mod_fm, mod_sine, mod_saw, mod_tri, mod_pulse, dsaw, dpulse, dtri, sub_pulse, gabber_kick, piano, bnoise, pnoise, gnoise, cnoise

### Fully Supported Effects (22)
reverb, gverb, echo, delay, distortion, lpf, rlpf, hpf, rhpf, flanger, chorus, ring_mod, wobble, ixi_techno, octaver, pan, slicer, bitcrusher, krush, compressor, normaliser, normalizer

### Unsupported Effects (with workarounds)
| Effect | Workaround |
|--------|-----------|
| pitch_shift | Use `rate:` param on samples |
| whammy | Use `with_fx :wobble` |
| band_eq | Combine `with_fx :lpf` + `with_fx :hpf` |
| tanh | Use `with_fx :distortion, distort: 0.3` |
| vowel | Use `with_fx :lpf` + `with_fx :hpf` |

### Partial Constructs  
| Construct | Limitation | Workaround |
|-----------|-----------|-----------|
| sync/cue | No blocking | Use separate live_loops |
| control | No-op | Use play + sleep sequences |
| .tick/.look | Counter-based cycle | Works for most patterns |
| sync: on live_loop | Not enforced | Loops start immediately |

### Unsupported Ruby Features
should_stop?, Time.now, lambda, proc, def methods, .each_cons, multi-variable block params

## Key Files

| File | Role |
|------|------|
| `src-tauri/src/lib.rs` | `validate_parity` Tauri command, `collect_usage()`, `ParityReport` |
| `src/agent.ts` | Parity intent handlers, `runParityAnalysis()`, `runParityFix()`, `getParityContext()` |
| `src/llm.ts` | System context with parity knowledge, enhanced `buildUserPrompt()` |
| `src/components/AgentChat.tsx` | Parity quick action buttons |
| `src-tauri/src/audio/synth.rs` | 42 OscillatorType implementations |
| `src-tauri/src/audio/effects.rs` | 22 effect processors with defaults |
| `src-tauri/src/audio/parser.rs` | Sonic Pi DSL parser (3500+ lines) |
| `src-tauri/src/audio/engine.rs` | Audio mixing, scheduling, FX buses |
| `src-tauri/src/audio/sample.rs` | Sample loading, playback parameters |
| `parity/PARITY_MATRIX.md` | Feature parity tracking matrix |
| `parity/PARITY_REPORT.md` | Historical parity analysis report |
| `scripts/full-parity-check.ps1` | 3-phase automated validation script |

## Troubleshooting

| Problem | Cause | Solution |
|---------|-------|---------|
| Parity check returns empty | Buffer has no code | Write Sonic Pi code first |
| Low parity score | Using unsupported features | Run "fix parity" for auto-workarounds |
| Effect sounds different | Default params mismatch | Compare against effect defaults table above |
| Synth timbre differs | Oscillator algorithm difference | Check synth.rs implementation vs Sonic Pi SynthDef |
| Timing is off | BPM or sleep calculation | Verify commands_to_audio beat→seconds conversion |
| Filter not resonating | res param mapping | Check Q = 0.7071 + res × 19.3 |
| Parse error on parity check | Invalid Sonic Pi syntax | Fix syntax errors first, then re-check parity |
