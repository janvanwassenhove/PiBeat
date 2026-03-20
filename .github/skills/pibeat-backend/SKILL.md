---
name: pibeat-backend
description: "PiBeat Rust/Tauri audio backend development. Use when working on the audio engine, parser, synthesizers, effects, samples, recorder, SuperCollider integration, visualization, Tauri commands, or any src-tauri/ Rust code. Covers cpal real-time audio, 42 oscillator types, 13 effects, DSL parser (3500 lines), voice mixing, scheduling, and the full AppState."
---

# PiBeat Backend Development Skill

Complete knowledge of the PiBeat Rust backend — the audio engine, Sonic Pi DSL parser, synthesizers, effects, sample system, SuperCollider integration, and all Tauri IPC commands.

## When to Use This Skill

- Implementing or modifying the Sonic Pi parser (`parser.rs`)
- Adding new synth types or modifying oscillator algorithms (`synth.rs`)
- Working on audio effects (`effects.rs`)
- Modifying the audio engine, voice system, or scheduling (`engine.rs`)
- Adding or changing Tauri commands (`lib.rs`)
- Working on sample loading, procedural generation (`sample.rs`)
- SuperCollider integration or SynthDef generation (`sc_engine.rs`, `sc_synthdefs.rs`)
- Recording functionality (`recorder.rs`)
- Visualization event system (`visualizer.rs`)
- Debugging audio artifacts, timing, or playback issues
- Performance optimization of the audio pipeline

## Module Map

```
src-tauri/
├── Cargo.toml          # Dependencies
├── tauri.conf.json     # App config, permissions, windows
├── build.rs            # Tauri build script
├── src/
│   ├── main.rs         # Entry point (5 lines)
│   ├── lib.rs          # AppState, 16+ Tauri commands, scheduling
│   └── audio/
│       ├── mod.rs      # Module declarations
│       ├── parser.rs   # Sonic Pi DSL parser (~3500 lines)
│       ├── engine.rs   # cpal audio engine (~2000 lines)
│       ├── synth.rs    # 42 oscillators + ADSR + SVF (~2000 lines)
│       ├── effects.rs  # 13 effects + biquad filter (~2500 lines)
│       ├── sample.rs   # WAV/MP3 + procedural samples (~1500 lines)
│       ├── sc_engine.rs    # SuperCollider OSC (~1500 lines)
│       ├── sc_synthdefs.rs # SynthDef generation (~1000 lines)
│       ├── recorder.rs     # WAV recording (~70 lines)
│       └── visualizer.rs   # Band visualization events (~500 lines)
├── tests/
│   ├── parity_validation.rs   # Sonic Pi parity tests
│   ├── fidelity_snapshots.rs  # Golden JSON snapshot tests
│   ├── example_parsing.rs     # Example file parsing tests
│   ├── disco_groove_test.rs   # Pattern validation
│   └── audio_compare.rs       # Audio comparison utilities
└── sc-bundle/          # Bundled SuperCollider binaries + SynthDefs
```

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| tauri | 2.x | IPC framework, window management |
| cpal | 0.15 | Cross-platform audio I/O (44.1 kHz stereo) |
| hound | 3.5 | WAV file read/write |
| minimp3 | 0.5 | MP3 decoding |
| rosc | 0.10 | OSC protocol for SuperCollider |
| rand | 0.8 | Randomization (rrand, dice, one_in, etc.) |
| parking_lot | 0.12 | Fast mutexes, no poisoning |
| crossbeam-channel | 0.5 | Lock-free bounded message passing |
| rubato | 0.15 | Sample rate conversion |
| dasp_* | 0.11 | DSP primitives (interpolation, signal, sample) |
| walkdir | 2 | Recursive directory traversal |
| serde/serde_json | 1.x | Serialization for Tauri IPC |

## AppState (`lib.rs`)

```rust
struct AppState {
    engine: AudioEngine,                          // cpal audio engine
    sc_engine: Mutex<Option<ScEngine>>,           // Optional SuperCollider
    use_sc: AtomicBool,                           // Engine selector
    sc_bundle_dir: Mutex<Option<PathBuf>>,        // SC binaries path
    recorder: Recorder,                           // WAV recorder
    samples_dir: PathBuf,                         // Built-in samples dir
    loaded_samples: Mutex<HashMap<String, (Vec<f32>, u32)>>,  // Sample cache
    sample_durations: Mutex<HashMap<String, f32>>,             // Duration cache
    session_id: Mutex<u64>,                       // Playback session counter
    log_messages: Mutex<Vec<LogEntry>>,           // Runtime logs
    user_samples_dir: Mutex<Option<PathBuf>>,     // User sample folder
    active_line_intervals: Mutex<Vec<LineInterval>>,  // Code highlighting data
    playback_start: Mutex<Option<Instant>>,       // Playback start time
    is_paused: AtomicBool,                        // Pause state
    visual_engine: VisualEngine,                  // Band visualization
    visual_publisher: EventPublisher,             // Event sender (never blocks)
}
```

## Parser (`parser.rs`)

### Pipeline
```
parse_code(code: &str) → Vec<ParsedCommand>
  ├── Splits code into lines
  ├── Handles block tracking (do/end depth)
  ├── For each line: parse_line(line, context) → Option<ParsedCommand>
  └── Resolves nested blocks (live_loop bodies, with_fx bodies, etc.)

commands_to_audio(commands: &[ParsedCommand], bpm: f32) → Vec<(f32, AudioCommand)>
  ├── Walks ParsedCommand tree
  ├── Tracks beat position, current synth, FX stack
  ├── Evaluates expressions (arithmetic, rrand, variables)
  ├── Expands loops (live_loop, times, loop)
  └── Produces beat-timed AudioCommand events
```

### ParsedCommand Enum (Full)
```rust
enum ParsedCommand {
    PlayNote { note: f64, params: HashMap<String, Expr> },
    PlaySample { name: String, params: HashMap<String, Expr> },
    Sleep(Expr),
    UseSynth(String),
    UseBpm(Expr),
    SetVolume(Expr),
    WithFx { name: String, params: HashMap<String, Expr>, body: Vec<ParsedCommand> },
    LiveLoop { name: String, body: Vec<ParsedCommand> },
    Loop { body: Vec<ParsedCommand> },
    Times { count: Expr, body: Vec<ParsedCommand> },
    InThread { body: Vec<ParsedCommand> },
    PlayChord { notes: Vec<Expr>, params: HashMap<String, Expr> },
    PlayPatternTimed { notes: Vec<Expr>, durations: Vec<Expr>, params: HashMap<String, Expr> },
    Define { name: String, params: Vec<String>, body: Vec<ParsedCommand> },
    FunctionCall { name: String, args: Vec<Expr> },
    Variable { name: String, value: Expr },
    Ring { name: String, values: Vec<Expr> },
    Spread { hits: Expr, total: Expr, name: Option<String> },
    Choose { list: Vec<Expr> },
    Scale { root: Expr, scale_type: String },
    Rrand { min: Expr, max: Expr },
    RrandI { min: Expr, max: Expr },
    Rand(Expr),
    RandI(Expr),
    Dice(Expr),
    OneIn(Expr),
    UseRandomSeed(Expr),
    If { condition: Expr, body: Vec<ParsedCommand>, else_body: Option<Vec<ParsedCommand>> },
    While { condition: Expr, body: Vec<ParsedCommand> },
    Each { collection: Expr, var: String, body: Vec<ParsedCommand> },
    Set { key: String, value: Expr },
    Get { key: String },
    AtBlock { times: Vec<Expr>, body: Vec<ParsedCommand> },
    TimeWarp { offset: Expr, body: Vec<ParsedCommand> },
    Cue(String),
    Sync(String),
    Control { /* ... */ },
    Stop,
    Next,
    Puts(String),
    Print(String),
    SynthDefaults { params: HashMap<String, Expr> },
    SampleDefaults { params: HashMap<String, Expr> },
}
```

### Key Parser Functions
| Function | Purpose |
|----------|---------|
| `parse_code()` | Entry point — handles multi-line code |
| `parse_line()` | Single line → ParsedCommand (main dispatch) |
| `parse_synth_name()` | Maps string to OscillatorType (42 synths) |
| `parse_play_chord()` | `play chord(:c4, :major)` handling |
| `parse_play_pattern_timed()` | Pattern playback parsing |
| `parse_note_value()` | Note name/MIDI/symbol → frequency |
| `parse_params()` | `key: value` parameter extraction |
| `parse_expression()` | Arithmetic, function calls, method chains |
| `parse_block()` | `do ... end` block collection |
| `commands_to_audio()` | ParsedCommand → timed AudioCommand events |
| `validate_and_parse()` | parse_code + validation warnings |

### Expression Evaluation
The parser includes a full expression evaluator supporting:
- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Comparisons: `<`, `>`, `<=`, `>=`, `==`, `!=`
- Method chains: `.choose`, `.tick`, `.look`, `.shuffle`, `.reverse`, `.first`, `.last`
- Function calls: `rrand()`, `rand()`, `dice()`, `one_in()`, `choose()`
- List literals: `[1, 2, 3]`
- Variable references
- Nested parentheses

## Audio Engine (`engine.rs`)

### AudioCommand Enum
```rust
enum AudioCommand {
    PlayNote { freq: f32, amp: f32, dur: f32, osc_type: OscillatorType, env: Envelope, pan: f32, params: Vec<(String, f32)> },
    PlaySample { data: Arc<Vec<f32>>, sample_rate: u32, amp: f32, rate: f32, pan: f32, env: Envelope, start: f32, finish: f32 },
    SetBpm(f32),
    SetVolume(f32),
    SetEffect { rm: f32, room: f32, dt: f32, df: f32, dist: f32, lpf: f32, hpf: f32 },
    FxStart { fx_type: String, params: Vec<(String, f32)> },
    FxEnd,
    Stop,
}
```

### Voice System
- **128 concurrent voices** (pre-allocated pool)
- Each voice has: oscillator state, ADSR envelope, filter state, panning
- Sample voices: rate conversion, start/finish trimming, beat_stretch
- Per-voice FX: `with_fx` blocks route through separate reverb/delay buses

### Mixing Pipeline
```
Per voice: oscillator → ADSR envelope → SVF filter → pan
           ↓
All voices summed (left + right channels)
           ↓
Master FX chain: reverb → delay → LPF → HPF → distortion → slicer →
                 bitcrusher → compressor → normaliser → flanger →
                 chorus → ring_mod → wobble → octaver
           ↓
Master volume → cpal output buffer
```

### Scheduling
- Separate scheduler thread per `run_code` invocation
- Commands sorted by beat time, dispatched via `crossbeam-channel`
- `session_id` prevents stale threads from interfering
- Windows: `timeBeginPeriod(1)` for 1ms timer resolution
- Pause support via `is_paused` AtomicBool (scheduler busy-waits)

## Synthesizers (`synth.rs`)

### 42 Oscillator Types
| Category | Types |
|----------|-------|
| Basic | Sine, Saw, Square, Triangle, Noise, Pulse |
| Detuned | SuperSaw, DSaw, DPulse, DTri, TechSaws, Hoover |
| FM | FM, ModFM, ModSine, ModSaw, ModTri, ModPulse |
| Classic | TB303, Prophet, Blade, Pluck, Piano |
| Ambient | DarkAmbience, Hollow, Growl, Zawa |
| Bells | PrettyBell, DullBell |
| Chiptune | ChipLead, ChipBass, ChipNoise |
| Special | SubPulse, GabberKick, Beep |
| Noise | BrownNoise, PinkNoise, GreyNoise, ClipNoise |

### Key Algorithms
- **PolyBLEP**: Anti-aliasing for all band-limited oscillators
- **SVF (Cytomic/Simper)**: State-variable filter — stable at all frequencies, supports LPF/HPF/BPF with Q control
- **ADSR Envelope**: Linear segments — attack, decay, sustain (hold time), release; sustain_level controls amplitude
- **Karplus-Strong**: `Pluck` and `Piano` synthesis with feedback delay line
- **SuperSaw**: 7-oscillator ensemble with detuning (comparator-based waveshaping)
- **Voss-McCartney**: Pink noise generation algorithm
- **FM Synthesis**: Modulation index + ratio parameters

### Envelope Defaults (match Sonic Pi v4.x)
| Param | Default | Unit |
|-------|---------|------|
| attack | 0.0 | beats |
| decay | 0.0 | beats |
| sustain | 0.0 | beats (hold time) |
| sustain_level | 1.0 | amplitude |
| release | 1.0 | beats |

## Effects (`effects.rs`)

### 13 Effect Processors
| Effect | Algorithm | Key Parameters |
|--------|-----------|---------------|
| Reverb | Schroeder (8 comb + 3 allpass) | mix, room, damp |
| Delay/Echo | Feedback delay line | time, feedback, mix |
| Distortion | Soft clipping (tanh) | distort, mix |
| LPF | Biquad (2nd order) | cutoff, res |
| HPF | Biquad (2nd order) | cutoff, res |
| Slicer | LFO gating (square/saw/tri) | phase, wave, mix |
| Bitcrusher/Krush | Bit depth + sample rate reduction | bits, sample_rate |
| Compressor | Envelope follower | threshold, slope, attack, release |
| Normaliser | Look-ahead limiter | level |
| Flanger | Modulated short delay + feedback | rate, depth, feedback, mix |
| Chorus | 3-voice detuned delays | rate, depth, mix |
| Ring Mod | Amplitude modulation | freq, mix |
| Wobble | LFO-modulated lowpass | rate, depth, mix |
| Octaver | Sub (flip-flop) + super (squaring) | sub_amp, super_amp, mix |
| Pan | Equal-power stereo | pan (-1 to 1) |

### BiquadFilter
```rust
struct BiquadFilter {
    b0: f32, b1: f32, b2: f32, a1: f32, a2: f32,
    x1: f32, x2: f32, y1: f32, y2: f32,
}
// Modes: LowPass, HighPass — with frequency and Q parameters
```

## Sample System (`sample.rs`)

### Built-in Sample Categories (14)
`drums`, `bd` (bass drums), `sn` (snares), `hat` (hi-hats), `elec`, `ambi`, `bass`, `loop`, `perc`, `tabla`, `vinyl`, `glitch`, `misc`, `mehackit`

### File Format Support
- **WAV**: Via `hound` crate (PCM 8/16/24/32-bit, float 32-bit)
- **MP3**: Via `minimp3` crate

### Procedural Sample Generation
When WAV files aren't available, the system generates samples procedurally:
- **Kicks**: Sine sweep (180→40 Hz) with exponential decay
- **Snares**: Noise burst + sine body (200 Hz)
- **Hi-hats**: Filtered noise with fast decay
- **Claps**: Noise burst with pre-delay ripples
- **Pads/Bass**: Synthesized tones with envelopes

### Sample Parameters
| Param | Purpose | Range |
|-------|---------|-------|
| `amp` | Volume | 0.0+ |
| `rate` | Playback speed | 0.1–4.0 |
| `pan` | Stereo position | -1 to 1 |
| `sustain` | Truncate to N beats | 0+ |
| `beat_stretch` | Fit sample into N beats | 0+ |
| `start` | Start position (normalized) | 0.0–1.0 |
| `finish` | End position (normalized) | 0.0–1.0 |
| `pitch` | Semitone shift (via rate) | any |
| `rpitch` | Rate-based pitch shift | any |
| `attack/release` | ADSR on sample | 0+ |

## SuperCollider Integration (`sc_engine.rs`)

### Two Modes
1. **Bundled**: Ships with scsynth + UGen plugins + pre-compiled SynthDefs in `sc-bundle/`
2. **System**: Falls back to system SuperCollider installation

### OSC Communication
- **Protocol**: UDP to scsynth (ports 57110/57120)
- **Commands**: `/s_new` (play synth), `/b_allocRead` (load buffer), `/n_free` (free node)
- **Node hierarchy**: SOURCE_GROUP (synths) → FX_GROUP (effects)
- **FX bus stack**: Nested `with_fx` creates private audio buses

### Health Monitoring
- Periodic `/status` queries
- Automatic fallback to cpal if SC is unresponsive
- SynthDefs re-loaded before each `run_code` invocation

## Tauri Commands Reference

### Code Execution
```rust
#[tauri::command]
fn run_code(code: String, state: State<Arc<AppState>>) -> Result<RunResult, String>
// Parses code, starts scheduler thread, returns RunResult
```

### Playback Control
```rust
fn stop_audio(state: ...) -> Result<(), String>
fn pause_audio(state: ...) -> Result<(), String>
fn resume_audio(state: ...) -> Result<(), String>
fn set_volume(volume: f32, state: ...) -> Result<(), String>
fn set_bpm(bpm: f32, state: ...) -> Result<(), String>
```

### Recording
```rust
fn start_recording(state: ...) -> Result<(), String>
fn stop_recording(path: Option<String>, state: ...) -> Result<String, String>
```

### Status & Data
```rust
fn get_waveform(state: ...) -> Result<Vec<f32>, String>     // 2048 samples
fn get_status(state: ...) -> Result<EngineStatus, String>
fn get_active_lines(state: ...) -> Result<Vec<usize>, String>
fn get_logs(state: ...) -> Result<Vec<LogEntry>, String>
fn clear_logs(state: ...) -> Result<(), String>
```

### Samples
```rust
fn list_samples(state: ...) -> Result<Vec<SampleInfo>, String>
fn play_sample_file(path: String, state: ...) -> Result<(), String>
fn get_sample_durations(names: Vec<String>, state: ...) -> Result<HashMap<String, f32>, String>
fn set_user_samples_dir(dir: String, state: ...) -> Result<(), String>
fn get_user_samples_dir(state: ...) -> Result<Option<String>, String>
fn scan_user_samples(state: ...) -> Result<Vec<UserSampleInfo>, String>
fn discover_user_samples(state: ...) -> Result<Vec<DiscoveredSample>, String>
fn analyze_user_sample(path: String, state: ...) -> Result<UserSampleInfo, String>
```

### Effects
```rust
fn set_effects(effects: EffectSettings, state: ...) -> Result<(), String>
// EffectSettings: reverb_mix, delay_time, delay_feedback, distortion, lpf_cutoff, hpf_cutoff
```

### SuperCollider
```rust
fn init_sc(state: ...) -> Result<String, String>
fn toggle_sc_engine(enabled: bool, state: ...) -> Result<(), String>
fn get_sc_status(state: ...) -> Result<ScStatus, String>
fn reload_synthdefs(state: ...) -> Result<(), String>
```

### System
```rust
fn get_env_var(key: String) -> Result<Option<String>, String>
fn preview_synth(name: String, state: ...) -> Result<(), String>
```

## Real-Time Audio Constraints (CRITICAL)

The cpal audio callback runs every ~5.8ms (512 samples @ 44.1kHz). Inside this callback:

| Rule | Reason |
|------|--------|
| **NO heap allocation** | `Vec::push`, `String::new`, `Box::new` can trigger OS allocator locks |
| **NO mutex locking** | Can cause priority inversion, unbounded latency |
| **NO I/O** | File reads, network, logging — all can block |
| **NO system calls** | `sleep()`, `thread::yield_now()` — unpredictable latency |
| **Use `try_recv()`** | Non-blocking channel reads for commands |
| **Pre-allocate** | All voice pools, effect buffers at init time |
| **Budget: ~23μs/sample** | 1/44100 seconds per sample computation |

### Patterns for Safe Audio Code
```rust
// ✅ Good: Lock-free command reception
match command_rx.try_recv() {
    Ok(cmd) => handle_command(cmd),
    Err(_) => {} // No command, continue mixing
}

// ✅ Good: Pre-allocated voice pool
let mut voices: [Voice; 128] = Default::default();

// ❌ Bad: Allocating in callback
let mut buffer = Vec::new(); // NEVER in audio callback

// ❌ Bad: Mutex in callback  
let data = state.lock(); // NEVER in audio callback
```

## Testing

### Test Suites
| File | Tests | Purpose |
|------|-------|---------|
| `parity_validation.rs` | 10+ | Sonic Pi v4.x behavior parity |
| `fidelity_snapshots.rs` | Per-fixture | Golden JSON comparison |
| `example_parsing.rs` | 6+ | Example file parsing (Test1-5, DiscoTest) |
| `disco_groove_test.rs` | 1+ | Specific pattern validation |
| `audio_compare.rs` | Utilities | Audio output comparison helpers |

### Running Tests
```bash
cd src-tauri
cargo test                           # All tests
cargo test --test parity_validation  # Parity only
cargo test --test fidelity_snapshots # Snapshots only
cargo test --release                 # Release mode (perf)
cargo test -- --nocapture            # See println! output
```

### Fixture System
- `.rb` fixtures in `fidelity/fixtures/` — Sonic Pi code samples
- `.json` golden files in `fidelity/event_stream/` — expected ParsedCommand output
- Tests parse fixtures, compare against golden JSON

## Troubleshooting

| Issue | Likely Cause | Fix |
|-------|-------------|-----|
| Audio clicks/pops | Buffer underrun or voice allocation in callback | Pre-allocate, optimize per-sample code |
| Wrong pitch | `parse_note_value()` mapping error | Check note → frequency table |
| Effect not applied | FX name not in parser's match arm | Add to `parse_line()` FX handling |
| Sample not found | Path resolution or missing procedural fallback | Check `sample.rs` lookup chain |
| SC timeout | scsynth not responding | Check port, restart SC, or fall back to cpal |
| Scheduling drift | Float precision in beat→time conversion | Use `f64` for timing calculations |
| Test snapshot fail | Parser output changed | Regenerate golden JSON or verify correctness |
| Compile error after synth add | Missing match arm | Update `generate_sample()` and `parse_synth_name()` |
