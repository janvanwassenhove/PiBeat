---
name: sonic-pi-rust-expert
description: "Deep domain expertise in Ruby, Sonic Pi DSL, Rust audio programming, and SuperCollider. Use when asked to write Sonic Pi code, translate Ruby patterns to Rust, implement DSP algorithms, design oscillators or filters, work with cpal audio, parse Ruby-like syntax in Rust, understand SuperCollider OSC protocol, or architect real-time audio systems. Covers Sonic Pi v4.x API reference, Rust audio patterns, and DSP fundamentals."
---

# Sonic Pi + Rust Audio Expert Skill

Provides deep domain expertise at the intersection of three domains: **Ruby/Sonic Pi DSL semantics**, **Rust systems programming for audio**, and **SuperCollider integration**. Use this skill when reasoning about how Sonic Pi concepts should be implemented in Rust.

## When to Use This Skill

- Writing or reviewing Sonic Pi code (Ruby-like DSL)
- Translating Sonic Pi semantics into Rust implementation
- Implementing DSP algorithms (oscillators, filters, envelopes, effects)
- Working with the cpal audio library in Rust
- Designing real-time audio scheduling systems
- Understanding SuperCollider OSC protocol and SynthDef format
- Debugging audio artifacts (clicks, aliasing, timing drift)
- Architecting the parser pipeline from text → parsed commands → audio events

## Domain 1: Ruby / Sonic Pi DSL

### Language Fundamentals
Sonic Pi uses a Ruby-based DSL with these key characteristics:
- **Top-to-bottom execution**: Code runs sequentially, `sleep` advances the timeline
- **Beat-relative timing**: All durations are in beats (relative to BPM, default 60)
- **Symbol notation**: Notes use Ruby symbols (`:c4`, `:fs3`) or MIDI numbers
- **Block syntax**: `do ... end` for blocks (e.g., loops, FX chains, threads)
- **Live coding**: `live_loop` enables hot-reload of running loops

### Sonic Pi v4.x API Quick Reference

**Notes & Chords**:
```ruby
play :c4                              # Symbol note
play 60                               # MIDI number
play :c4, amp: 0.5, pan: -1           # With parameters
play chord(:c4, :major)               # Chord
play_pattern_timed [:c4,:e4,:g4], [0.25]  # Sequential pattern
```

**Synths** (44 types in PiBeat):
```
:sine, :saw, :square, :triangle, :noise, :pulse, :super_saw,
:tb303, :prophet, :blade, :pluck, :fm, :beep, :piano,
:dark_ambience, :hollow, :growl, :pretty_bell, :dull_bell,
:chip_lead, :chip_bass, :chip_noise, :tech_saws, :hoover,
:zawa, :mod_fm, :mod_sine, :mod_saw, :mod_tri, :mod_pulse,
:dsaw, :dpulse, :dtri, :sub_pulse, :gabber_kick,
:bnoise, :pnoise, :gnoise, :cnoise
```

**Effects** (22 types):
```
:reverb, :gverb, :echo, :delay, :distortion,
:lpf, :rlpf, :hpf, :rhpf, :slicer, :bitcrusher, :krush,
:compressor, :normaliser/:normalizer,
:flanger, :chorus, :ring_mod, :pan, :wobble, :ixi_techno, :octaver
```

**Default Parameters**:
| Param | Default | Context |
|-------|---------|---------|
| amp | 1.0 | All sounds |
| pan | 0.0 | Center |
| attack | 0.0 | Envelope |
| decay | 0.0 | Envelope |
| sustain | 0.0 | Envelope (hold time) |
| sustain_level | 1.0 | Envelope |
| release | 1.0 | Envelope |
| cutoff | 130 | Filter (MIDI note) |
| res | 0.0 | Filter resonance |
| rate | 1.0 | Sample playback |
| BPM | 60.0 | Tempo |

### Idiomatic Sonic Pi Patterns
```ruby
# Layered composition: one loop per instrument
live_loop :drums do
  sample :kick; sleep 0.5
  sample :snare; sleep 0.5
end

live_loop :bass do
  use_synth :tb303
  play :c2, cutoff: 80, release: 0.25
  sleep 0.5
end

# Effect wrapping
with_fx :reverb, mix: 0.5 do
  with_fx :distortion, distort: 0.3 do
    play :e3
  end
end

# Randomization for variation
live_loop :hats do
  sample :hihat, amp: rrand(0.3, 0.8) if one_in(2)
  sleep 0.25
end
```

## Domain 2: Rust Audio Programming

### cpal Audio Architecture
PiBeat uses `cpal` for cross-platform audio output:

```rust
// Audio callback runs on a dedicated real-time thread
// Must be lock-free, no allocation, deterministic timing
fn audio_callback(data: &mut [f32], info: &OutputCallbackInfo) {
    for frame in data.chunks_mut(2) {  // Stereo
        let (left, right) = mix_all_voices();
        frame[0] = left;
        frame[1] = right;
    }
}
```

**Critical constraints for the audio callback**:
- No heap allocation (`Vec::push`, `String::new`, etc.)
- No mutex locking (use lock-free queues or atomics)
- No I/O or system calls
- Must complete within buffer time (typically 11.6ms for 512 samples @ 44.1kHz)
- Use pre-allocated ring buffers for voice and effect state

### DSP Algorithm Reference

**PolyBLEP Anti-Aliasing** (used for saw, square, triangle):
```rust
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        2.0 * t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}
```

**SVF Filter (Cytomic/Simper)** — used for LPF/HPF with resonance:
```rust
// State variables: ic1eq, ic2eq (initialized to 0)
let g = (PI * cutoff_hz / sample_rate).tan();
let k = 2.0 - 2.0 * resonance;  // resonance 0..1
let a1 = 1.0 / (1.0 + g * (g + k));
let a2 = g * a1;
let a3 = g * a2;
let v3 = input - ic2eq;
let v1 = a1 * ic1eq + a2 * v3;
let v2 = ic2eq + a2 * ic1eq + a3 * v3;
ic1eq = 2.0 * v1 - ic1eq;
ic2eq = 2.0 * v2 - ic2eq;
// LPF output = v2, HPF output = input - k*v1 - v2, BPF = v1
```

**ADSR Envelope**:
```rust
enum EnvelopeStage { Attack, Decay, Sustain, Release, Done }
// In PiBeat: attack/decay/sustain times in beats → converted to samples
// sustain_level is amplitude (0.0–1.0), sustain is hold time
```

**Schroeder Reverb** (8 comb + 3 allpass):
- Comb filters: parallel, different delay lengths (e.g., 1557, 1617, 1491, 1422, 1277, 1356, 1188, 1116 samples)
- Allpass filters: serial, shorter delays (e.g., 225, 556, 441 samples)
- `room` scales delay lengths, `damp` controls HF absorption

**Equal-Power Panning**:
```rust
let angle = (pan + 1.0) * PI / 4.0;  // pan: -1..1 → 0..π/2
let left = angle.cos();
let right = angle.sin();
```

### Rust Audio Patterns

**Lock-free communication** (UI → audio thread):
```rust
// Use crossbeam or std::sync::mpsc for command passing
// Audio thread: try_recv() — non-blocking
// UI thread: send() — can block (OK, not real-time)
```

**Pre-allocated voice pool**:
```rust
struct VoicePool {
    voices: Vec<Voice>,  // Fixed-size, allocated once
    active: usize,       // Number of active voices
}
```

**Sample-accurate scheduling**:
```rust
// Convert beat time to sample offset
let sample_offset = (beat_time * 60.0 / bpm * sample_rate) as usize;
```

## Domain 3: SuperCollider Integration

### OSC Protocol
PiBeat communicates with SuperCollider via OSC messages:
```
/s_new <synth_name> <node_id> <add_action> <target> [param pairs...]
/n_set <node_id> <param> <value>
/n_free <node_id>
/g_new <group_id> <add_action> <target>
```

### SynthDef Structure
Sonic Pi's synths are defined as SuperCollider SynthDefs:
- Compiled `.scsyndef` files in `sc-bundle/`
- Each defines the DSP graph (UGens, routing, parameters)
- PiBeat: cpal engine reimplements these in Rust; SC engine sends OSC

## Architecture Decision Guide

When implementing a Sonic Pi feature in PiBeat, decide:

| Question | If Yes → | If No → |
|----------|----------|---------|
| Is it pure syntax? | Parser only (`parser.rs`) | Need runtime support |
| Does it produce sound? | Audio engine changes needed | Parser + command output only |
| Does it modify running sound? | Complex (like `control`) — may need voice tracking | Simpler one-shot approach |
| Is timing involved? | Beat-relative scheduling in `commands_to_audio()` | Direct event emission |
| Is it Ruby-specific? | May not be implementable (lambda, Time.now) | Can be translated to Rust |

## Common Translation Patterns

### Ruby → Rust Idioms
| Ruby (Sonic Pi) | Rust (PiBeat Parser) |
|-----------------|---------------------|
| `:c4` (symbol) | `parse_note_value("c4")` → MIDI 60 → 261.63 Hz |
| `do ... end` block | Block depth counter, collect inner commands |
| `live_loop :name do` | `ParsedCommand::LiveLoop { name, body, max_iter: 500 }` |
| `rrand(0.5, 1.0)` | `rand::thread_rng().gen_range(0.5..=1.0)` |
| `one_in(3)` | `rand::thread_rng().gen_ratio(1, 3)` |
| `ring(:c4, :e4, :g4).tick` | Store ring values, approximate cycling |
| `spread(3, 8)` | Bjorklund algorithm → `[true, false, false, true, false, false, true, false]` |

### Frequency Calculation
```rust
fn midi_to_freq(midi: f32) -> f32 {
    440.0 * 2.0_f32.powf((midi - 69.0) / 12.0)
}
```

### Beat-to-Time Conversion
```rust
fn beats_to_seconds(beats: f32, bpm: f32) -> f32 {
    beats * 60.0 / bpm
}
```

## Key Files

| File | Role |
|------|------|
| `src-tauri/src/audio/parser.rs` | Sonic Pi DSL → ParsedCommand |
| `src-tauri/src/audio/synth.rs` | Oscillators, envelopes, filters |
| `src-tauri/src/audio/effects.rs` | Effect processing chain |
| `src-tauri/src/audio/engine.rs` | cpal audio callback, voice mixing |
| `src-tauri/src/audio/sample.rs` | Sample loading and playback |
| `src-tauri/src/audio/sc_engine.rs` | SuperCollider OSC communication |
| `.github/copilot-instructions.md` | Full Sonic Pi language reference |
| `parity/PARITY_MATRIX.md` | Feature parity status |
| `docs/fidelity-roadmap.md` | Implementation roadmap |
