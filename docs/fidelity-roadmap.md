# PiBeat Fidelity Roadmap

**Target**: Sonic Pi v4.x (latest stable)
**Engine**: PiBeat Rust runtime (cpal + SuperCollider backends)

## Phase 1: Core Semantic Parity ✅
Ensure the parser and event-stream output match Sonic Pi behavior for the supported subset.

| Feature | Status | Tests | Notes |
|---------|--------|-------|-------|
| `play :note` (symbol) | ✅ Done | `snapshot_play_note_basic` | |
| `play <midi_number>` | ✅ Done | `snapshot_play_midi_number` | Fixed: was treating MIDI as frequency |
| `play chord()` (all tones) | ✅ Done | `snapshot_play_chord_major` | Was only emitting root note |
| `sleep` timing | ✅ Done | `snapshot_sleep_basic` | |
| `use_bpm` | ✅ Done | `snapshot_use_bpm` | |
| `use_synth` switching | ✅ Done | `snapshot_use_synth_multiple` | |
| `sample :name` | ✅ Done | `snapshot_sample_basic` | |
| `sample` with params | ✅ Done | `snapshot_sample_with_params` | |
| `N.times do` loop | ✅ Done | `snapshot_times_loop` | |
| `live_loop` iteration | ✅ Done | `snapshot_live_loop_basic` | |
| Multiple `live_loop`s | ✅ Done | `snapshot_drum_pattern_basic` | |
| `with_fx` blocks | ✅ Done | `snapshot_with_fx_reverb` | |
| Nested `with_fx` | ✅ Done | `snapshot_with_fx_nested` | |
| `in_thread` (single run) | ✅ Done | `snapshot_in_thread_basic` | Fixed: was looping 500x |
| `define :fn` functions | ✅ Done | `snapshot_define_function` | |
| Variable assignment/usage | ✅ Done | `snapshot_variable_assignment` | Fixed: play now resolves vars |
| `play_pattern_timed` | ✅ Done | `snapshot_play_pattern_timed` | |
| Seedable PRNG (`rrand` etc) | ✅ Done | `snapshot_rrand_seeded` | `use_random_seed` now functional |
| Default envelope values | ✅ Done | `snapshot_default_envelope` | a=0, d=0, s=1, r=1 |

## Phase 2: Scheduler Parity ✅
| Feature | Status | Notes |
|---------|--------|-------|
| cpal single-thread scheduler | ✅ Done | Replaced per-event thread::spawn |
| `timeBeginPeriod(1)` on Windows | ✅ Done | 1ms timer resolution for cpal path |
| Coarse sleep + spin-wait | ✅ Done | Matches SC path pattern |
| SC scheduler (was already good) | ✅ N/A | Single thread + spin-wait already |

## Phase 3: Audio Engine Parity ✅
| Feature | Status | Notes |
|---------|--------|-------|
| OSC bundle timestamps | ⬜ Todo | SC events use /s_new, no bundle timestamps |
| SynthDef envelope curves | ⬜ Todo | All `\lin` → should be exponential for release |
| `with_fx` on cpal engine | ✅ Done | Per-voice VoiceFx chain (14 FX types) |
| `with_synth` block scoping | ✅ Done | Saves/restores current_synth |
| `use_synth_defaults` | ✅ Done | `parse_defaults_line()` + `ctx.synth_defaults` |
| Sample `beat_stretch:` | ✅ Done | Rate adjusted by sample duration/BPM |
| Sample `start:/finish:` | ✅ Done | Audio trimming applied |
| Sample `lpf:/hpf:` | ✅ Done | Wrapped with FxStart/FxEnd per-voice |
| Reverb send bus | ✅ Done | Shared Schroeder reverb for scoped reverb |
| Delay send bus | ✅ Done | Shared delay line for scoped echo/delay |

## Phase 4: Advanced Sonic Pi Features
| Feature | Status | Notes |
|---------|--------|-------|
| `cue`/`sync` semantics | ⬜ Todo | Recognized but no-ops (warning logged) |
| `at` blocks | ✅ Done | Implemented |
| `choose()` for arrays | ✅ Done | Random selection works |
| Ring `.tick`/`.look` cycling | ✅ Done | Deterministic counter-based, LCM for multi-ring |
| `use_bpm_mul` | ⬜ Todo | |
| Multiple octave notation | ⬜ Todo | e.g., `:c` without octave |
| `control` command | ⬜ Todo | Recognized but no-op (warning logged) |
| `Time.now` access | ❌ N/A | Ruby runtime feature, not supportable |
| `def method(args)` | ✅ Done | Ruby-style methods work |
| `.each do \|x\|` | ✅ Done | Array iteration works |
| `sync:` on live_loop | ⬜ Todo | Recognized but ignored (warning logged) |
| Per-sample ADSR | ⬜ Todo | `attack:/decay:/release:` on samples |

## Phase 5: Audio Comparison Pipeline
| Component | Status | Notes |
|-----------|--------|-------|
| Fidelity fixtures (40+ .rb files) | ✅ Done | `fidelity/fixtures/` |
| Event-stream snapshot tests | ✅ Done | 50 Rust integration tests |
| JSON event-stream exports | ✅ Done | `fidelity/event_stream/*.json` |
| Audio comparison harness | ✅ Done | RMS, spectral, onset, silence metrics |
| Reference WAV import mode | ✅ Ready | Place in `fidelity/renders/reference/` |
| Candidate WAV generation | ⬜ Todo | Automate cpal offline render |
| CI integration | ⬜ Todo | Run fidelity suite in CI |

## Definition of Done
- [x] `cargo test` all green (113+ tests)
- [x] Fidelity test fixtures exist for all supported features
- [x] Event-stream snapshot tests verify parser output
- [x] Audio comparison harness exists and self-tests pass
- [ ] Reference WAVs generated for fixture set
- [ ] Candidate WAVs auto-generated from PiBeat
- [ ] All supported features pass audio comparison within tolerances
- [ ] Unsupported features documented with explicit gaps
