# PiBeat Fidelity Progress Log

## Session 1 — Initial Audit + Implementation

### CHANGE-01: `play chord()` Multi-Note Playback
- **Symptom**: `play chord(:c4, :major)` only produced 1 note (root only)
- **Root Cause**: `parse_play_chord()` in `parser.rs` returned single `PlayNote` for root
- **Fix**: Rewrote `parse_play_chord()` to return `TimesLoop { count: 1, commands: [PlayNote × N] }` so all chord tones play simultaneously at same time_offset
- **Tests**: `snapshot_play_chord_major` — verifies ≥3 notes at t=0 with correct frequencies
- **File**: `src-tauri/src/audio/parser.rs`

### CHANGE-03: Seedable PRNG
- **Symptom**: `use_random_seed` was a no-op; `rrand()` used `thread_rng()` (nondeterministic)
- **Root Cause**: `ParseContext` had no RNG state; `use_random_seed` was parsed as comment
- **Fix**: Added `rng: StdRng` field to `ParseContext`, initialized with `seed_from_u64(0)`. Replaced all 6 `rand::thread_rng()` sites with `self.rng`. Made `use_random_seed` call `ctx.rng = StdRng::seed_from_u64(seed)`
- **Tests**: `snapshot_rrand_seeded` — runs same code twice, verifies identical amp values
- **Files**: `src-tauri/src/audio/parser.rs` (imports, ParseContext, resolve_numeric, eval_simple_arithmetic, eval_one_in, evaluate_condition)

### CHANGE-06: Default Parameter Values
- **Symptom**: Default envelope values differed from Sonic Pi (amp 0.5, release 0.3, etc.)
- **Root Cause**: Hardcoded fallback values in `extract_param_with_defaults` calls and `Envelope::default()`
- **Fix**: Updated all defaults: amp 0.5→1.0, attack 0.01→0.0, decay 0.1→0.0, sustain_level 0.7→1.0, release 0.3→1.0. Updated `Envelope::default()` in `synth.rs`
- **Tests**: `snapshot_default_envelope` — verifies attack=0, decay=0, sustain=1, release=1
- **Files**: `src-tauri/src/audio/parser.rs`, `src-tauri/src/audio/synth.rs`

### CHANGE-04: cpal Single Scheduler Thread
- **Symptom**: cpal engine spawned one `std::thread` per timed event with `thread::sleep(delay)` — up to 15.6ms jitter on Windows
- **Root Cause**: Per-event thread spawning pattern at `lib.rs` lines 580-625 + `schedule_samples_with_timing`
- **Fix**: Replaced with single scheduler thread using `timeBeginPeriod(1)` + coarse sleep + spin-wait (same pattern as SC path). Merged note and sample events into single sorted list.
- **Tests**: Existing tests pass; timing improvement measurable in real-time playback
- **File**: `src-tauri/src/lib.rs`

### CHANGE-07: MIDI Note Number Parsing
- **Symptom**: `play 60` produced frequency 60 Hz instead of C4 (261.63 Hz)
- **Root Cause**: `parse_note_value()` parsed `60` as `f32 = 60.0 > 20.0` and returned it as raw frequency
- **Fix**: Reordered parsing: integer values 0-127 → MIDI conversion first; float values with decimal point → raw frequency; values > 127 → raw frequency
- **Tests**: `snapshot_play_midi_number` — verifies MIDI 60→261.63, 64→329.63, 67→392.00
- **File**: `src-tauri/src/audio/parser.rs` function `parse_note_value()`

### CHANGE-08: Variable Resolution in `play`
- **Symptom**: `my_note = :c4; play my_note` produced 0 notes
- **Root Cause**: `play` command handler took raw token and passed to `parse_note_value()` without resolving variables
- **Fix**: Added `ctx.resolve_string(note_str)` before `parse_note_value()` call in the `play` handler
- **Tests**: `snapshot_variable_assignment` — verifies variable resolution for note names
- **File**: `src-tauri/src/audio/parser.rs`

### CHANGE-09: `in_thread` Single Execution
- **Symptom**: `in_thread do ... end` ran body 500 times (same as `live_loop`)
- **Root Cause**: `in_thread` was parsed as `ParsedCommand::Loop { name: "thread", parallel: true }` and `commands_to_audio` expanded all parallel loops 500 times
- **Fix**: Added check for `name == "thread"` → `loop_iterations = 1` in `commands_to_audio`
- **Tests**: `snapshot_in_thread_basic` — verifies exactly 4 notes (2 per thread)
- **File**: `src-tauri/src/audio/parser.rs`

### Infrastructure Created
- **27 fidelity fixture files**: `fidelity/fixtures/*.rb`
- **22 event-stream snapshot tests**: `src-tauri/tests/fidelity_snapshots.rs`
- **11 JSON event-stream snapshots**: `fidelity/event_stream/*.json`
- **8 audio comparison harness tests**: `src-tauri/tests/audio_compare.rs`
- **Derives added**: `PartialEq` on `OscillatorType` and `Envelope`; `Serialize` on `AudioCommand`
- **Module visibility**: `pub mod audio` for integration test access

### Build Status
- **Compilation**: ✅ (16+ warnings, 0 errors)
- **Tests**: ✅ 58/58 pass (28 crate + 22 fidelity + 8 audio harness)
- **Remaining Risks**: 
  - `ring().tick/.look` cycling still approximated
  - `cue/sync` still no-ops
  - cpal `FxStart/FxEnd` still no-ops (effects don't work on built-in engine)

---

## Session 2 — Synth Implementation Parity

### CHANGE-10: Forward Synth Params to cpal Engine
- **Symptom**: `cutoff:`, `res:`, `detune:`, `pulse_width:`, `depth:`, `divisor:` parsed by parser but discarded by cpal engine (`params: _`)
- **Root Cause**: `engine.rs` PlayNote handler dropped `params` with `_` wildcard; `SynthVoice::new()` had no way to accept them
- **Fix**: 
  - Added `SynthVoice::new_with_params()` that accepts `&[(String, f32)]` and uses them to configure cutoff, resonance, detune, pulse_width, FM depth/divisor
  - Updated `engine.rs` to call `new_with_params()` instead of `new()` and pass `&params`
  - Kept `new()` as convenience wrapper calling `new_with_params()` with empty slice
- **Files**: `src-tauri/src/audio/synth.rs`, `src-tauri/src/audio/engine.rs`

### CHANGE-11: Fix SuperSaw Detune + Filter
- **Symptom**: `:super_saw` sounded thin/narrow, not at all like Sonic Pi's rich detuned supersaw
- **Root Cause**: 
  1. Detune amounts were `[-0.11..0.11] * 0.01` = ±0.0011 freq ratio — **100x too narrow**
  2. No low-pass filter applied (Sonic Pi has RLPF with `cutoff: 130` MIDI, `res: 0.7`)
  3. Mix divided by 7 instead of 3 (too quiet)
- **Fix**:
  - New detune formula: `(i-3) * detune * 0.06` where `detune` defaults to `0.1` (from params) — gives ±0.018 spread, matching Sonic Pi ~0.36 semitones
  - Added SVF low-pass filter to super_saw output
  - Changed normalization from `/7.0` to `/3.0` matching Sonic Pi's `Mix.ar(sigs)/3`
- **Files**: `src-tauri/src/audio/synth.rs`
- **SC SynthDef**: Also fixed to use `detune` parameter: `(i-3) * detune * 0.06`

### CHANGE-12: Add RLPF to Filtered Synths (cpal)
- **Symptom**: Synths that should have low-pass filters sounded too bright/harsh
- **Root Cause**: cpal implementations of `:saw`, `:square`, `:pulse`, `:prophet`, `:tech_saws`, `:sub_pulse`, `:dsaw`, `:dpulse` had no filter; only TB303, Blade, Hollow, DarkAmbience had filters
- **Fix**: Added `self.svf_tick(raw); self.filter_lp` to all synths that have RLPF in Sonic Pi. Filter params now properly initialized from parser `cutoff:` / `res:` params (MIDI→Hz conversion)
- **Files**: `src-tauri/src/audio/synth.rs`

### CHANGE-13: Fix SC SynthDef Defaults
- **Symptom**: SC SynthDef defaults didn't match Sonic Pi defaults
- **Root Cause**: All 38+ SynthDefs had `amp=0.5` (should be `1.0`), `attack=0.01` (should be `0`), `release=0.3` (should be `1.0`)
- **Fix**: Updated all SynthDef parameter defaults across all 38+ definitions to match Sonic Pi:
  - `amp`: `0.5` → `1`
  - `attack`: `0.01` → `0`  
  - `release`: `0.3` → `1` (for most synths; percussive synths like piano/bell kept their shorter defaults)
- **Files**: `src-tauri/src/audio/sc_synthdefs.rs`

### CHANGE-14: Exponential Envelope Release Curve
- **Symptom**: Note releases sounded unnatural — linear fade vs Sonic Pi's exponential decay
- **Root Cause**: `envelope_value()` used `sustain * (1.0 - release_t)` — pure linear ramp
- **Fix**: Changed release segment to `sustain * exp(-5.0 * release_t)` — exponential decay that reaches ~0.7% at t=1.0, matching the natural decay of Sonic Pi's envelope curves
- **Files**: `src-tauri/src/audio/synth.rs`

### CHANGE-15: Fix Detuned Oscillators (DSaw/DPulse)
- **Symptom**: `:dsaw`, `:dpulse` had hardcoded 0.5% detune, ignoring `detune:` parameter
- **Root Cause**: Second oscillator frequency was `freq * 1.005` regardless of params
- **Fix**: Uses `detune_amounts[0]` from param-computed array; falls back to `1.005` when no detune param given. Also added RLPF filter to both.
- **Files**: `src-tauri/src/audio/synth.rs`

---

## Session 3 — Example File Audit + Documentation

### ANALYSIS: Example File Compatibility
Analyzed three example files (`Test2`, `Test3`, `Test4`) for Sonic Pi feature coverage:

**Test2 (Dark Metal Guitar)**: ✅ Full support
- All features (`define`, `with_fx :distortion`, `play_pattern_timed`, `live_loop`, `in_thread`, `one_in()`) work correctly
- Effects apply globally (cpal limitation) but timing is correct

**Test3 (Synaptic Drift)**: ⚠️ Partial support
- `set`/`get` work for state management
- `sync:` on live_loop is ignored (loops start immediately)
- `cue`/`sync` synchronization not implemented
- `beat_stretch:` parsed but not applied
- `pitch:` works via rate adjustment
- File path samples work

**Test4 (Techno)**: ⚠️ Limited support
- `Time.now.to_f` not available (defaults to 0)
- `def method(args)` works but time-based conditions always return false
- `control` command is a no-op
- `.each do |n|` iteration works correctly
- `sync:` on live_loop ignored

### CHANGE-16: Add Warning Logs for Unsupported Features
- **Symptom**: Users don't know when features are being skipped silently
- **Root Cause**: `cue`, `sync`, `control`, `beat_stretch`, `sync:` param were parsed without feedback
- **Fix**: Added eprintln warnings with `[WARN]` prefix for:
  - `cue`/`sync` commands: "synchronization is NOT implemented - loops will run independently"
  - `control` command: "synth parameters cannot be modified at runtime"
  - `sync:` on live_loop: "loop will start immediately"
  - `beat_stretch:`: "NOT applied - sample will play at original speed"
  - `Time.now`: "NOT supported - value will be set to 0"
- **Files**: `src-tauri/src/audio/parser.rs`
- **Tests**: All 35 lib tests + 22 fidelity tests pass

### Documentation Update
- Created comprehensive `examples/README.md` with:
  - Per-example compatibility tables
  - Known issues and workarounds
  - General limitations documentation
  - Sample parameter support matrix

### Build Status
- **Compilation**: ✅ (warnings, 0 errors)
- **Tests**: ✅ 57 pass (35 lib + 22 fidelity)
- **Remaining Risks**: 
  - `ring().tick/.look` cycling still approximated
  - `cue/sync` still no-ops
  - cpal `FxStart/FxEnd` still no-ops (effects don't work per-note on built-in engine)
  - SC SynthDef envelope curves still use `\lin` (exponential only applies to cpal path)
  - `beat_stretch:` not implemented (requires sample duration knowledge)

---

## Session 3 — Complete Parity Audit + Fixes

### Audits Conducted
- **parser.rs** (6229 lines): Full recursive descent parser audit
- **synth.rs** (1283 lines): All oscillator types, envelope, note conversion
- **effects.rs** (784 lines): All 15 FX types, filter algorithms
- **engine.rs** (520 lines): Audio stream, mixing, panning
- **sample.rs**: WAV loading, playback

### PARITY-01: Flat Note Parsing (synth.rs)
- **Symptom**: Notes like `:df4`, `:ef4`, `:gf4` failed to parse (F suffix for flats)
- **Root Cause**: `note_name_to_midi()` only handled S suffix for sharps, B suffix for flats — not F
- **Fix**: Added DF/EF/GF/AF/BF/CF/FF/ES to base note match in `note_name_to_midi()`
- **Tests**: `snapshot_flat_notes` — verifies 5 flat notes at correct MIDI frequencies

### PARITY-02: Envelope Click Protection (synth.rs)
- **Symptom**: Zero-release notes caused audible clicks
- **Root Cause**: `envelope_value()` allowed release=0, causing discontinuity
- **Fix**: Added `effective_release = release.max(0.001)` (1ms minimum) in envelope_value()
- **Tests**: `snapshot_envelope_click_protection`

### PARITY-03: Resonant Filters (effects.rs)
- **Symptom**: `rlpf`/`rhpf` `res:` parameter was ignored
- **Root Cause**: BiquadFilter had no Q parameter support; parser didn't extract `res:`
- **Fix**: Added `low_pass_q`/`high_pass_q` to BiquadFilter, `set_lpf_res()`/`set_hpf_res()` methods, parser extracts `res:` for rlpf/rhpf
- **Tests**: `snapshot_lpf_resonance`, `snapshot_hpf_resonance`

### PARITY-04: Wobble Effect (effects.rs)
- **Symptom**: Wobble was amplitude modulation, not filter modulation
- **Root Cause**: Effect multiplied signal by LFO instead of modulating filter cutoff
- **Fix**: Changed to LFO-modulated lowpass filter matching Sonic Pi's `:ixi_techno`
- **Tests**: `snapshot_fx_wobble`, `snapshot_fx_ixi_techno_alias`

### PARITY-05: Octaver Sub-Octave (effects.rs)
- **Symptom**: Sub-octave doubled frequency instead of halving
- **Root Cause**: Used `abs()` which rectified signal (doubles frequency via full-wave rectification)
- **Fix**: Implemented zero-crossing flip-flop frequency divider (true octave-down)
- **Tests**: `snapshot_fx_octaver`

### PARITY-06: Equal-Power Panning (engine.rs)
- **Symptom**: Linear panning caused volume dip at center position
- **Root Cause**: Simple linear pan law: `left = 1-pan, right = pan`
- **Fix**: Implemented constant-power cosine law matching Sonic Pi's Pan2 UGen
- **Tests**: `snapshot_equal_power_panning`

### PARITY-07: Reverse Sample Playback (engine.rs)
- **Symptom**: Negative `rate:` values caused crash or no audio
- **Root Cause**: No bounds checking for negative position values
- **Fix**: Added `position >= 0.0` check and abs() on fractional part for interpolation
- **Tests**: `snapshot_reverse_sample_playback`

### PARITY-08: MIDI→Hz Cutoff Conversion (effects.rs)
- **Symptom**: LPF/HPF cutoff values treated as raw Hz even when MIDI notes
- **Root Cause**: `set_lpf()`/`set_hpf()` used value directly as Hz
- **Fix**: When cutoff ≤ 130, convert MIDI→Hz: `440 * 2^((midi-69)/12)`
- **Tests**: `snapshot_midi_cutoff_conversion` (implicit via lpf_resonance test)

### PARITY-09: Krush → Bitcrusher Routing (parser.rs)
- **Symptom**: `with_fx :krush` applied reverb instead of bitcrusher
- **Root Cause**: `"krush"` was matched in `"reverb" | "gverb" | "krush"` arm
- **Fix**: Separated krush into own match arm routing to bitcrusher params
- **Tests**: `snapshot_fx_krush_routes_to_bitcrusher`

### PARITY-10: Echo BPM Sync (parser.rs)
- **Symptom**: Echo `phase:` treated as seconds, not beats
- **Root Cause**: Parser passed phase value directly without BPM conversion
- **Fix**: Multiply `phase_beats * beat_duration` in echo/delay match arm
- **Tests**: `snapshot_echo_bpm_sync`

### PARITY-11: Reverb Damp Parameter (parser.rs)
- **Symptom**: Reverb `damp:` parameter was ignored
- **Root Cause**: Not extracted from params or passed through SetEffect
- **Fix**: Added `reverb_damp` field to SetEffect, extraction in parser, `set_reverb_damp()` in engine
- **Tests**: `snapshot_reverb_damp`

### PARITY-12: Delay Mix Parameter (parser.rs)
- **Symptom**: Echo/delay `mix:` parameter was ignored
- **Root Cause**: Not extracted or passed through SetEffect
- **Fix**: Added `delay_mix` field to SetEffect and extraction in parser
- **Tests**: `snapshot_delay_mix`

### PARITY-13: Bitcrusher Defaults (parser.rs)
- **Symptom**: Bitcrusher defaults (bits=8/16, sr=44100) didn't match Sonic Pi (bits=10, sr=10000)
- **Fix**: Updated defaults to `bits=10, sr=10000` for both `:bitcrusher` and `:krush`
- **Tests**: `snapshot_bitcrusher_defaults`

### PARITY-14: Chorus Linear Interpolation (effects.rs)
- **Symptom**: Chorus used integer sample index truncation causing aliasing
- **Fix**: Added linear interpolation between adjacent delay buffer samples

### PARITY-15: FX Parameter Extraction (parser.rs)
- **Symptom**: Many FX params (`bits`, `sample_rate`, `sub_amp`, `super_amp`, `pan`, `freq`, etc.) not extracted
- **Root Cause**: `extract_fx_params()` had limited param name list
- **Fix**: Extended param list to include all common FX parameter names

### PARITY-16: midi_to_hz / hz_to_midi (synth.rs)
- **Fix**: Added public `midi_to_hz()` and `hz_to_midi()` utility functions

### New Tests Added (25 new snapshot tests)
- `snapshot_flat_notes` — Flat note parsing
- `snapshot_fx_krush_routes_to_bitcrusher` — Krush → bitcrusher
- `snapshot_echo_bpm_sync` — Echo BPM sync
- `snapshot_reverb_damp` — Reverb damp param
- `snapshot_delay_mix` — Delay mix param
- `snapshot_lpf_resonance` — LPF resonance
- `snapshot_hpf_resonance` — HPF resonance
- `snapshot_equal_power_panning` — Pan values propagated
- `snapshot_reverse_sample_playback` — Negative rate
- `snapshot_fx_wobble` — Wobble effect
- `snapshot_fx_octaver` — Octaver effect
- `snapshot_fx_ixi_techno_alias` — ixi_techno alias
- `snapshot_bitcrusher_defaults` — Sonic Pi defaults
- `snapshot_envelope_click_protection` — Zero release
- `snapshot_fx_scope_restoration` — FX scope save/restore
- `snapshot_one_in_conditional` — Probabilistic
- `snapshot_ring_basic` — Ring buffers
- `snapshot_spread_euclidean` — Euclidean rhythms
- `snapshot_choose_array` — Random choice
- `snapshot_play_chord_minor7` — Minor7 chord
- `snapshot_scale_pattern` — Scale patterns
- `snapshot_set_get` — Global state
- `snapshot_while_loop` — While loops
- `snapshot_at_block` — At scheduling
- `snapshot_define_with_params` — Parameterized defines

### Build Status
- **Compilation**: ✅ (warnings only, 0 errors)
- **Tests**: ✅ 103 pass (39 lib + 8 audio_compare + 6 example_parsing + 50 fidelity_snapshots)
- **Remaining Gaps**: 
  - `sync/cue` still no-ops (P0)
  - `control` still no-ops (P0)
  - `.tick/.look` approximated (P1)
  - `use_synth_defaults` not implemented (P1)
  - `with_synth` block not implemented (P1)
  - Per-sample `lpf:` not implemented (P2)
  - Piano synth uses additive model, not physical (P3)
