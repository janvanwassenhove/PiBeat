# PiBeat Sonic Pi Parity Matrix

## Parser Features

| Feature | Status | Notes |
|---------|--------|-------|
| `play :note` | ✅ | Note names, MIDI numbers, flat notes (`:df4`, `:ef4`) |
| `play chord(:root, :type)` | ✅ | Major, minor, dom7, minor7, etc. |
| `play_pattern_timed` | ✅ | Arrays, scale(), chord(), variables |
| `sample :name` | ✅ | Built-in and file paths |
| `sleep` | ✅ | Numeric and variable |
| `use_synth` | ✅ | All standard synths |
| `use_bpm` | ✅ | |
| `with_fx` | ✅ | Nested supported, scoped save/restore |
| `live_loop` | ✅ | Max 500 iterations |
| `in_thread` | ✅ | **Fixed: variable scoping** |
| `.times do` | ✅ | With block variable |
| `.times do \|i\|` | ✅ | Arithmetic on loop vars |
| `while ... do` | ✅ | Max 500 iterations |
| `define :name do ... end` | ✅ | User functions |
| `set/get` | ✅ | Global variables |
| `if/else/elsif` | ✅ | Block form |
| Single-line `if` | ✅ | Proper block expansion |
| `sync/cue` | ⚠️ | Parsed with logging, basic handling |
| `sync:` on live_loop | ⚠️ | Parsed with logging, not synchronized |
| `control` | ⚠️ | Parsed, no-op (use explicit notes instead) |
| `at` | ✅ | Schedule at specific beat times, non-clock-advancing |
| `time_warp` | ✅ | Schedule at relative offset, non-clock-advancing |
| `choose` | ✅ | Random element from array |
| `ring` / `spread` | ✅ | Ring buffers and Euclidean rhythms |

## Runtime Semantics

| Feature | Status | Notes |
|---------|--------|-------|
| Loop variable (`i`) | ✅ | |
| Variable arithmetic | ✅ | |
| `get()` in arithmetic | ✅ | |
| Thread variable scoping | ✅ | No leak to parent |
| Thread timing | ⚠️ | All threads start immediately |
| Random seed | ✅ | `use_random_seed` |
| `rrand`/`rand`/`dice` | ✅ | |
| `one_in(n)` | ✅ | |
| `ring`/`spread` | ✅ | |
| `.tick`/`.look` | ✅ | Deterministic counter-based cycling, LCM for multi-ring |
| `sync`/`cue` coordination | ⚠️ | Parsed, logged, starts immediately |

## Synth List Status

| Synth | Status |
|-------|--------|
| `:sine` | ✅ |
| `:saw` | ✅ |
| `:square` | ✅ |
| `:triangle` | ✅ |
| `:noise` | ✅ |
| `:pulse` | ✅ |
| `:super_saw` | ✅ |
| `:tb303` | ✅ |
| `:prophet` | ✅ |
| `:blade` | ✅ |
| `:pluck` | ✅ |
| `:fm` | ✅ |
| `:beep` | ✅ |
| `:dark_ambience` | ✅ |

## FX List Status

| Effect | Status | Notes |
|--------|--------|-------|
| `:reverb` | ✅ | `mix`, `room`, `damp` params |
| `:echo` / `:delay` | ✅ | BPM-synced `phase` (beats→seconds), `mix`, `feedback` |
| `:distortion` | ✅ | |
| `:lpf` / `:rlpf` | ✅ | MIDI→Hz cutoff conversion, `res` param for resonance |
| `:hpf` / `:rhpf` | ✅ | MIDI→Hz cutoff conversion, `res` param for resonance |
| `:flanger` | ✅ | |
| `:chorus` | ✅ | Linear interpolation for anti-aliasing |
| `:ring_mod` | ✅ | |
| `:wobble` / `:ixi_techno` | ✅ | LFO-modulated lowpass filter (matches Sonic Pi) |
| `:octaver` | ✅ | Flip-flop sub-octave (correct frequency division) |
| `:pan` | ✅ | Equal-power (constant-power) cosine-law panning |
| `:slicer` | ✅ | |
| `:bitcrusher` / `:krush` | ✅ | Sonic Pi defaults (bits=10, sr=10000), krush→bitcrusher routing |
| `:compressor` | ✅ | |
| `:normaliser` | ✅ | |

## Sample Features Status

| Feature | Status | Notes |
|---------|--------|-------|
| Symbol names (`:kick`) | ✅ | |
| File paths | ✅ | Quoted strings |
| `amp:` | ✅ | |
| `rate:` | ✅ | Including negative (reverse playback) |
| `pan:` | ✅ | Equal-power panning |
| `pitch:` | ✅ | Via rate |
| `rpitch:` | ✅ | |
| `sustain:` | ✅ | Truncates playback |
| `beat_stretch:` | ✅ | Rate adjusted by sample duration/BPM |
| `start:` / `finish:` | ✅ | Audio trimming applied |
| `lpf:` | ✅ | Applied per-sample via VoiceFx LPF (wraps with FxStart/FxEnd) |
| `hpf:` | ✅ | Applied per-sample via VoiceFx HPF (wraps with FxStart/FxEnd) |

## Audio Engine Parity

| Feature | Status | Notes |
|---------|--------|-------|
| Equal-power panning | ✅ | Cosine-law constant-power pan (matches Sonic Pi Pan2) |
| Envelope click protection | ✅ | Min 1ms release to prevent clicks on zero-release notes |
| Reverse playback | ✅ | Negative rate support with bounds checking |
| MIDI→Hz filter cutoff | ✅ | Automatic conversion when cutoff ≤ 130 |
| Resonant filters | ✅ | Q parameter for BiquadFilter LPF/HPF |
| Per-voice FX (cpal) | ✅ | `with_fx` blocks apply per-voice via VoiceFx chain |
| Reverb send bus | ✅ | Shared Schroeder reverb for scoped `with_fx :reverb` |
| Delay send bus | ✅ | Shared delay line for scoped `with_fx :echo`/`:delay` |

## Test Coverage

| Metric | Count |
|--------|-------|
| Library unit tests | 48 |
| Audio comparison tests | 8 |
| Example parsing tests | 13 |
| Fidelity snapshot tests | 57 |
| Parity validation tests | 169 |
| **Total** | **296** |

## Known Gaps

| Gap | Priority | Notes |
|-----|----------|-------|
| `sync/cue` no-op | P0 | Cannot reproduce sync-based patterns |
| `control` no-op | P0 | Cannot modify running synths |
| Per-sample ADSR | ✅ | attack/decay/sustain_level/release on samples |
| Live reload | P3 | Code changes require stop/start |
| Piano synth accuracy | P3 | Additive, not physical model |
| TB303 accent/slide | P3 | Simplified implementation |

## Legend

- ✅ Fully supported
- ⚠️ Partially supported / limitations
- ❌ Not implemented
