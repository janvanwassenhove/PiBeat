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
| `use_bpm_mul` | ✅ | Multiplies current BPM |
| `with_bpm_mul N do...end` | ✅ | Scoped BPM multiplication with save/restore |
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
| `sync/cue` | ✅ | `sync` waits for the next matching `cue`, resolved statically over the expanded timeline |
| `sync:` on live_loop | ✅ | First iteration starts at the first matching cue (Sonic Pi gates only the first) |
| `control` | ⚠️ | Parsed, no-op (use explicit notes instead) |
| `at` | ✅ | Schedule at specific beat times, non-clock-advancing |
| `time_warp` | ✅ | Schedule at relative offset, non-clock-advancing |
| `choose` | ✅ | Random element from array |
| `ring` / `spread` | ✅ | Ring buffers and Euclidean rhythms |
| `with_swing` | ✅ | One run in every `pulse` time-warped by `shift`, counted per `tick:` key |

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
| `sync`/`cue` coordination | ✅ | Resolved over the expanded timeline; unmatched `sync` continues rather than hanging |

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
| `:bpf` / `:rbpf` | ✅ | Band-pass filter with MIDI→Hz cutoff, resonance |
| `:nbpf` / `:nrbpf` | ✅ | Normalised band-pass filter variants |
| `:nrlpf` / `:nrhpf` | ✅ | Normalised resonant low/high-pass filter variants |
| `:tremolo` | ✅ | LFO amplitude modulation (4 wave types: sine/saw/square/triangle) |
| `:ping_pong` | ✅ | Stereo ping-pong delay, BPM-synced phase |
| `:level` | ✅ | Simple gain/amplitude adjustment |
| `:mono` | ✅ | Forces mono output (pan override to center) |
| `:band_eq` | ✅ | Parametric EQ with freq/db/res |
| `:pitch_shift` | ✅ | Pitch shifting via rate adjustment (approximation) |
| `:whammy` | ✅ | Pitch transposition via rate adjustment (approximation) |
| `:tanh` | ✅ | Tanh distortion (krunch parameter) |

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
| Master mixer chain | ✅ | `sonic_mixer` ports Sonic Pi's master stage: DC block → Limiter(0.99, 0.01) → hard clip → 10 Hz / 20.5 kHz safety filters (SC path) |
| Sample-accurate note placement | ✅ | Timestamped OSC bundles sent 0.5s ahead (Sonic Pi's `sched_ahead_time`); scsynth starts each synth on the exact sample (SC path) |
| `env_curve` / `attack_level` / `decay_level` | ✅ | Envelopes use Sonic Pi's `shapedAdsr` array form on both engines |
| Equal-power panning | ✅ | Cosine-law constant-power pan (matches Sonic Pi Pan2) |
| Envelope click protection | ✅ | Min 1ms release to prevent clicks on zero-release notes |
| Reverse playback | ✅ | Negative rate support with bounds checking |
| MIDI→Hz filter cutoff | ✅ | Automatic conversion when cutoff ≤ 130 |
| Resonant filters | ✅ | Q parameter for BiquadFilter LPF/HPF |
| Per-voice FX (cpal) | ✅ | `with_fx` blocks apply per-voice via VoiceFx chain |
| Reverb send bus | ✅ | Shared Schroeder reverb for scoped `with_fx :reverb` |
| Delay send bus | ✅ | Shared delay line for scoped `with_fx :echo`/`:delay` |

## Test Coverage

All of these run from a clean clone (`cargo test` in `src-tauri/`).

| Metric | Count |
|--------|-------|
| Library unit tests | 51 |
| Audio comparison tests | 8 |
| Disco groove fixture test | 1 |
| Example parsing tests | 13 |
| Fidelity snapshot tests | 72 |
| Parity validation tests | 204 |
| **Total** | **349** |

## Known Gaps

| Gap | Priority | Notes |
|-----|----------|-------|
| `control` no-op | P0 | Cannot modify a running synth. Needs the parser to model synth handles (`s = play 60` … `control s, note: 65`), which it has no concept of today. |
| cpal engine has no master limiter | P1 | The Sonic Pi master chain is implemented on the SuperCollider path only; the built-in engine still sums straight to the device. |
| `sync` resolved statically | P1 | Cues are matched over the pre-expanded timeline rather than at playback time, so a cue whose time depends on a runtime random value can land on the wrong iteration. Converges for ordinary metronome/follower arrangements. |
| Live reload | P3 | Code changes require stop/start |
| Piano synth accuracy | P3 | Additive, not physical model |

## Verification status

Everything above is verified by the automated suite, which checks the
*event stream* — what is scheduled, when, with which parameters. The SC-path
changes (master mixer, OSC bundle timing) cannot be exercised without a running
`scsynth`, so they are verified by construction against Sonic Pi's published
SynthDef sources rather than by test. Rendering reference WAVs and comparing
audio remains open — see `docs/fidelity-roadmap.md`, Phase 5.
| TB303 accent/slide | P3 | Simplified implementation |

## Legend

- ✅ Fully supported
- ⚠️ Partially supported / limitations
- ❌ Not implemented
