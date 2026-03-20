---
name: sound-parity
description: "Validates PiBeat's sound output matches the original Sonic Pi IDE exactly. Use when asked to check audio fidelity, verify synth sounds, compare effect processing, validate sample playback, check envelope shapes, verify filter behavior, compare WAV renders, run audio comparison tests, or ensure DSP accuracy. Covers oscillators, ADSR envelopes, reverb, delay, distortion, filters, panning, and timing."
---

# Sound Parity Validation Skill

Ensures that PiBeat's Rust audio engine produces sound output that is identical or indistinguishable from the original Sonic Pi IDE. Validates oscillators, envelopes, effects, samples, filters, panning, and temporal accuracy.

## When to Use This Skill

- Verifying a synth type sounds correct compared to Sonic Pi
- Checking that an effect (reverb, delay, etc.) produces the right output
- Comparing WAV renders between PiBeat and Sonic Pi reference recordings
- Validating envelope shapes (ADSR) match Sonic Pi defaults
- Debugging why a sound is "off" (wrong pitch, volume, timbre, timing)
- Checking that sample playback parameters (rate, pitch, beat_stretch) work correctly
- Running the full sound parity validation suite
- Writing new audio comparison tests

## Prerequisites

- Rust toolchain installed (`cargo` available)
- PowerShell available for running validation scripts
- Reference WAV files in `fidelity/renders/` (if doing audio comparison)
- SuperCollider installed (optional, for SC engine validation)

## Step-by-Step Workflows

### Workflow 1: Full Sound Parity Check

1. Run the comprehensive sound parity script:
   ```powershell
   .\scripts\validate-sound-parity.ps1 -All -Verbose
   ```
2. This checks:
   - All 44 synth types parse and map to correct `OscillatorType`
   - All 22 effect types parse and map to correct `FxType`
   - Default parameter values match Sonic Pi v4.x
   - Note frequencies match A4=440Hz tuning standard
   - Fidelity snapshot tests pass
   - Audio comparison tests pass

### Workflow 2: Validate Specific Sound Domain

```powershell
# Synths only
.\scripts\validate-sound-parity.ps1 -Synths -Verbose

# Effects only
.\scripts\validate-sound-parity.ps1 -Effects -Verbose

# Samples only
.\scripts\validate-sound-parity.ps1 -Samples -Verbose
```

### Workflow 3: WAV Audio Comparison

1. Run audio comparison tests:
   ```bash
   cd src-tauri && cargo test --test audio_compare -- --nocapture
   ```
2. Tests compare PiBeat WAV output against reference renders using:
   - **RMS difference** — overall loudness match
   - **Spectral similarity** — frequency content match
   - **Onset detection** — timing of note/sample attacks
   - **Silence detection** — gaps and timing accuracy

### Workflow 4: 5-Phase Full Parity Check

1. Run the comprehensive report:
   ```powershell
   .\scripts\full-parity-check.ps1 -All -Report -Verbose
   ```
2. Phases:
   - **Phase 1**: Rust compilation check
   - **Phase 2**: Core test suite (lib + fidelity + parity)
   - **Phase 3**: Syntax coverage analysis
   - **Phase 4**: Sound coverage (synths, effects, defaults)
   - **Phase 5**: Gap analysis and summary
3. Report is generated in `fidelity/reports/`

### Workflow 5: Debug a Specific Sound Issue

1. **Identify the sound domain**: Is it a synth, effect, sample, or envelope issue?
2. **Find the relevant module**:
   | Domain | Rust File |
   |--------|-----------|
   | Oscillator/Synth | `src-tauri/src/audio/synth.rs` |
   | Effects | `src-tauri/src/audio/effects.rs` |
   | Samples | `src-tauri/src/audio/sample.rs` |
   | Envelope | `src-tauri/src/audio/synth.rs` (ADSR section) |
   | Panning | `src-tauri/src/audio/engine.rs` |
   | Timing | `src-tauri/src/audio/engine.rs` + `parser.rs` (commands_to_audio) |
3. **Write a minimal fixture** in `fidelity/fixtures/` that isolates the issue
4. **Run the focused test** and compare output

## Sonic Pi v4.x Sound Reference

### Default Envelope (ADSR)
| Parameter | Default | Unit |
|-----------|---------|------|
| attack | 0.0 | beats |
| decay | 0.0 | beats |
| sustain | 0.0 | beats (hold time) |
| sustain_level | 1.0 | amplitude multiplier |
| release | 1.0 | beats |

### Synth Implementation Details

PiBeat implements 44 synth types in `src-tauri/src/audio/synth.rs`. Each must produce waveforms matching Sonic Pi's SuperCollider SynthDefs.

#### Basic Waveforms
| Synth | Waveform | PiBeat DSP | Sonic Pi Equivalent |
|-------|----------|-----------|-------------------|
| `:sine` | Sine | Direct `sin()` | `SinOsc` |
| `:saw` | Sawtooth | PolyBLEP anti-aliased | `Saw` |
| `:square` | Square | PolyBLEP anti-aliased | `Pulse(width: 0.5)` |
| `:triangle` | Triangle | PolyBLEP integrated square | `LFTri` |
| `:noise` | White noise | Random samples | `WhiteNoise` |
| `:pulse` | Variable pulse | PolyBLEP with `pulse_width` param | `Pulse` |
| `:beep` | Sine (alias) | Same as Sine | `SinOsc` |

#### Detuned / Supersaw
| Synth | Voice Count | Detune | Notes |
|-------|-------------|--------|-------|
| `:super_saw` | 7 | ±0.1 semitone spread | Main + 6 detuned voices |
| `:dsaw` | 2 | Slight detune | Dual detuned saw |
| `:dpulse` | 2 | Slight detune | Dual detuned pulse |
| `:dtri` | 2 | Slight detune | Dual detuned triangle |

#### FM Synthesis
| Synth | Carrier | Modulator | Key Param |
|-------|---------|-----------|-----------|
| `:fm` | Sine | Sine | `divisor`, `depth` |
| `:mod_fm` | Sine | Sine | `mod_phase` (default=1.0, multiplied by 6.0) |
| `:mod_sine` | Sine | Sine AM | `mod_phase` |
| `:mod_saw` | Saw | Sine AM | `mod_phase` |
| `:mod_dsaw` | DSaw | Sine AM | `mod_phase` |
| `:mod_tri` | Triangle | Sine AM | `mod_phase` |
| `:mod_pulse` | Pulse | Sine AM | `mod_phase` |

#### Classic Synths
| Synth | Algorithm | Key Difference from Sonic Pi |
|-------|-----------|----------------------------|
| `:tb303` | Saw → SVF resonant filter | Simplified — no accent/slide |
| `:prophet` | Dual detuned saw + PWM | Close match |
| `:zawa` | Phase-distortion | Approximated |

#### Other Synths
| Synth | Category | Notes |
|-------|----------|-------|
| `:blade` | Filtered pad | Dual slow-detuned saw |
| `:tech_saws` | Filtered pad | Multiple saws + filter |
| `:hoover` | Filtered pad | Detuned + portamento-like |
| `:pluck` | Physical model | Karplus-Strong (noise → filtered delay) |
| `:piano` | Additive | Multi-partial with decay (⚠️ not physical model) |
| `:pretty_bell` | FM bell | Harmonic FM |
| `:dull_bell` | FM bell | Inharmonic FM |
| `:hollow` | Filtered pad | Band-pass filtered noise |
| `:dark_ambience` | Pad | Filtered low-frequency oscillators |
| `:growl` | Aggressive | Modulated saw |
| `:chip_lead` | Chiptune | Hard-clipped square |
| `:chip_bass` | Chiptune | Hard-clipped low square |
| `:chip_noise` | Chiptune | 1-bit noise |
| `:bnoise` / `:pnoise` / `:gnoise` / `:cnoise` | Colored noise | Brown/pink/grey/clip noise |
| `:sub_pulse` | Sub bass | Low pulse wave |
| `:gabber_kick` | Percussive | Frequency-swept sine |

### Effect Implementation Details — Full Parameter Reference

PiBeat implements 22 effect types in `src-tauri/src/audio/effects.rs`. Effects are applied in the `EffectChain::process()` method in this fixed order:

**Processing Order**: distortion → LPF → HPF → slicer → bitcrusher → compressor → flanger → chorus → ring_mod → pan → wobble → octaver → delay → reverb → normaliser

| Effect | Algorithm | Key Parameters | Sonic Pi Defaults |
|--------|-----------|----------------|-------------------|
| `:reverb` | Schroeder (8 comb + 3 allpass) | `mix`, `room`, `damp` | mix=0.4, room=0.6, damp=0.5 |
| `:gverb` | Same as reverb | `mix`, `room`, `damp` | mix=0.4, room=0.6 |
| `:echo` / `:delay` | Delay line + feedback | `phase` (beats→sec), `decay`/`feedback`, `mix` | phase=0.25, decay=2, mix=1 |
| `:distortion` | Soft clip (`(x × gain).tanh()`) | `distort`, `mix` | distort=0.5, mix=1 |
| `:lpf` / `:rlpf` | Biquad lowpass (Q from res) | `cutoff` (MIDI→Hz), `res` | cutoff=100 MIDI, res=0 |
| `:hpf` / `:rhpf` | Biquad highpass (Q from res) | `cutoff` (MIDI→Hz), `res` | cutoff=60 MIDI, res=0 |
| `:slicer` | Amplitude gating LFO | `phase`, `mix`, `wave` | phase=0.25, mix=1, wave=0(square) |
| `:bitcrusher` / `:krush` | Bit reduction + sample-and-hold | `bits`, `sample_rate`, `mix` | bits=10, sr=10000, mix=1 |
| `:compressor` | Feed-forward RMS | `threshold`, `clamp_time`, `relax_time`, `mix` | 0.2, 0.01, 0.01, 1 |
| `:normaliser` / `:normalizer` | Peak limiter | `level` | level=1 |
| `:flanger` | Modulated delay (5ms±4ms range) | `rate`, `depth`, `feedback`, `mix` | 0.5, 0.5, 0.5, 1 |
| `:chorus` | 3-voice detuned delay (15-25ms) | `rate`, `depth`, `mix` | 0.3, 0.5, 1 |
| `:ring_mod` | Sine carrier × input | `freq`, `mix` | freq=30, mix=1 |
| `:pan` | Equal-power cosine panning | `pan` | pan=0 |
| `:wobble` / `:ixi_techno` | LFO-modulated LPF (200-8000Hz) | `rate`, `depth`, `mix` | rate=1, depth=0.5, mix=1 |
| `:octaver` | Sub: zero-crossing flip-flop; Super: squaring | `mix`, `sub_amp`, `super_amp` | mix=1, sub=0.5, super=0 |

#### Critical Effect Parity Notes
- **Echo/delay phase is in beats**: `phase_seconds = phase_beats × 60 / bpm`
- **Filter cutoff uses MIDI notes**: When `cutoff ≤ 130`, convert: `hz = 440 × 2^((midi-69)/12)`
- **Resonance mapping**: `Q = 0.7071 + res × 19.3` (res range 0→1)
- **Slicer wave shapes**: 0=square, 1=saw-down, 2=saw-up, 3=triangle
- **Bitcrusher/krush routing**: `:krush` maps to the bitcrusher implementation
- **Per-voice FX**: `with_fx` blocks create `VoiceFxSlot` instances attached to individual voices (supports: LPF, HPF, distortion, slicer, bitcrusher, ring_mod, delay, reverb, flanger, chorus, wobble, octaver, pan)

### Sample Playback Details

PiBeat procedurally generates built-in samples in `src-tauri/src/audio/sample.rs` and supports file-based WAV/MP3 loading.

#### Sample Parameter Implementation
| Parameter | Conversion | Implementation |
|-----------|-----------|----------------|
| `amp:` | Direct multiplier | Applied to all output samples |
| `rate:` | Direct playback speed | Cubic Hermite interpolation for non-integer rates |
| `pan:` | Stereo position | Equal-power cosine pan law |
| `pitch:` | → rate adjustment | `rate = rate × 2^(pitch/12)` |
| `rpitch:` | Same as pitch | Alias |
| `sustain:` | Truncates playback | `max_samples = sustain × 60/bpm × sample_rate` |
| `beat_stretch:` | → rate adjustment | `rate = sample_duration_secs / (beats × 60/bpm)` |
| `start:` | Normalized position | `start_sample = (start × total_samples) as usize` |
| `finish:` | Normalized position | `end_sample = (finish × total_samples) as usize` |
| `lpf:` | ⚠️ NOT applied | Parsed but ignored — use `with_fx :lpf` instead |

#### Shorthand → Full Sample Name Mapping
```
:kick     → drum_heavy_kick
:snare    → drum_snare_hard
:hihat    → hat_bdu
:hat      → hat_bdu
:clap     → perc_snap
:tom      → drum_tom_mid_hard
:bass     → bass_hit_c
:loop_amen → loop/loop_amen
```

### Audio Quality Metrics
When comparing audio output, use these thresholds:
| Metric | Acceptable | Good | Excellent |
|--------|-----------|------|-----------|
| RMS difference | < 0.1 | < 0.05 | < 0.01 |
| Spectral correlation | > 0.8 | > 0.9 | > 0.95 |
| Onset timing error | < 50ms | < 20ms | < 5ms |
| Frequency accuracy | < 2 cents | < 1 cent | < 0.5 cents |

## Troubleshooting

| Problem | Cause | Solution |
|---------|-------|----------|
| Wrong pitch | `parse_note_value()` returning wrong MIDI | Check note name mapping; A4=69=440Hz |
| Clicks/pops at note end | Release too short | Ensure minimum 1ms release (click protection) |
| Effect not audible | `FxStart`/`FxEnd` not processed | Check `effects.rs` has processing for this FX type |
| Sample too fast/slow | Wrong `rate` calculation | Check `beat_stretch` → rate = sample_duration / (beats × beat_duration) |
| Wrong envelope shape | ADSR defaults wrong | Default: a=0, d=0, s_level=1.0, r=1.0 |
| Filter not resonating | `res` not mapped to Q | Check SVF `Q = 1 / (2 * (1 - res))` mapping |
| Stereo imbalance | Pan law wrong | Should be equal-power cosine: `L = cos(θ), R = sin(θ)` |
| Reverb too muddy | Comb filter tuning | Check Schroeder comb delay lengths and damping coefficients |

## Key Files

| File | Role |
|------|------|
| `src-tauri/src/audio/synth.rs` | Oscillator generation, ADSR envelopes, SVF filter |
| `src-tauri/src/audio/effects.rs` | Effect processing (reverb, delay, distortion, etc.) |
| `src-tauri/src/audio/sample.rs` | Sample loading, playback, procedural generation |
| `src-tauri/src/audio/engine.rs` | Audio mixing, panning, scheduling |
| `src-tauri/tests/audio_compare.rs` | WAV comparison test suite |
| `src-tauri/tests/parity_validation.rs` | Sound parity test assertions |
| `scripts/validate-sound-parity.ps1` | Automated sound validation script |
| `fidelity/renders/` | Reference WAV renders |
| `fidelity/event_stream/` | Golden event stream JSON snapshots |

## Related Skills

| Skill | Use When |
|-------|----------|
| `parity-checker` | Agent-driven parity analysis via Tauri `validate_parity` command, LLM-assisted reports, and auto-fix suggestions |
| `syntax-validation` | Checking if Sonic Pi code parses correctly (syntax-level, not audio-level) |
| `performance-validation` | Benchmarking audio engine latency and CPU usage |
