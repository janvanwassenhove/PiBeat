---
name: performance-validation
description: "Validates PiBeat performance matches or exceeds the original Sonic Pi IDE. Use when asked to benchmark audio latency, measure CPU usage, profile Rust hot paths, optimize parser throughput, check real-time scheduling accuracy, reduce memory allocations, or ensure glitch-free audio playback. Covers parser performance, audio engine throughput, scheduling precision, and resource consumption."
---

# Performance Validation Skill

Ensures PiBeat's Rust audio engine achieves equivalent or better performance compared to the original Sonic Pi IDE — covering parser throughput, audio latency, scheduling precision, CPU usage, and memory efficiency.

## When to Use This Skill

- Benchmarking parser speed (lines/second, commands/second)
- Measuring audio latency (time from `run_code` to first sound)
- Profiling CPU usage during playback of complex compositions
- Checking for audio glitches, dropouts, or buffer underruns
- Optimizing hot paths in the Rust backend
- Comparing PiBeat resource usage against Sonic Pi
- Ensuring real-time scheduling meets timing requirements
- Investigating memory allocation patterns

## Prerequisites

- Rust toolchain with `cargo bench` support
- `criterion` crate (for micro-benchmarks, if configured)
- PowerShell for running validation scripts
- Optional: `cargo flamegraph` for profiling
- Optional: `perf` (Linux) or ETW (Windows) for system-level profiling

## Performance Targets

### Sonic Pi Baseline
| Metric | Sonic Pi (Ruby + SuperCollider) | PiBeat Target (Rust) |
|--------|-------------------------------|---------------------|
| Parse latency (simple) | ~5ms | < 2ms |
| Parse latency (complex, 100+ lines) | ~50ms | < 10ms |
| First-sound latency | ~100-200ms | < 50ms |
| Audio buffer size | 512 samples @ 44.1kHz (~11.6ms) | 512 samples (~11.6ms) |
| CPU usage (idle) | ~2-5% | < 2% |
| CPU usage (complex piece) | ~15-30% | < 15% |
| Memory baseline | ~150MB (Ruby + SC server) | < 50MB |
| Voice polyphony | 128+ | 128+ |
| Scheduling jitter | < 5ms | < 2ms |

### Critical Thresholds
| Metric | Acceptable | Glitch-free | Excellent |
|--------|-----------|-------------|-----------|
| Buffer underrun rate | < 1/min | 0 in 5min | 0 in 30min |
| Audio callback duration | < 80% buffer | < 60% buffer | < 40% buffer |
| Parser throughput | > 1000 lines/s | > 5000 lines/s | > 10000 lines/s |
| Scheduling accuracy | ±5ms | ±2ms | ±1ms |

## Step-by-Step Workflows

### Workflow 1: Quick Performance Smoke Test

1. Compile in release mode:
   ```bash
   cd src-tauri && cargo build --release
   ```
2. Run the full test suite and note timing:
   ```bash
   cargo test --release -- --nocapture 2>&1 | Select-String "test result|running"
   ```
3. Check that all tests complete promptly — slow tests indicate performance issues

### Workflow 2: Parser Throughput Benchmark

1. Create a benchmark that parses example files repeatedly:
   ```rust
   // In src-tauri/benches/parser_bench.rs (or inline test)
   let code = std::fs::read_to_string("../examples/DiscoTest").unwrap();
   let start = std::time::Instant::now();
   for _ in 0..1000 {
       parse_code(&code);
   }
   let elapsed = start.elapsed();
   println!("Parse rate: {:.0} iterations/sec", 1000.0 / elapsed.as_secs_f64());
   ```
2. Run with `cargo bench` or `cargo test --release <bench_test> -- --nocapture`
3. Target: > 5000 parses/second for a typical 20-line file

### Workflow 3: Audio Latency Measurement

1. Measure time from `run_code()` invocation to first `AudioCommand` emission:
   ```rust
   let start = std::time::Instant::now();
   let commands = parse_code("play :c4");
   let audio = commands_to_audio(&commands, 60.0);
   let parse_latency = start.elapsed();
   // parse_latency should be < 1ms for simple code
   ```
2. For end-to-end latency (including audio device), measure from Tauri command invocation to first non-zero audio sample in the cpal callback

### Workflow 4: CPU/Memory Profiling

1. **Flamegraph** (Linux/macOS):
   ```bash
   cargo flamegraph --test parity_validation -- parity_complex_composition
   ```
2. **Windows ETW profiling**:
   ```powershell
   # Use Windows Performance Recorder or cargo-instruments
   cargo build --release
   # Run the app and use Task Manager / Process Explorer for CPU%
   ```
3. **Memory allocation tracking**:
   ```rust
   // Add to test: count allocations using a custom allocator
   // Or use DHAT: cargo test --features dhat-heap
   ```

### Workflow 5: Scheduling Precision Test

1. Parse a multi-loop composition with precise timing:
   ```ruby
   use_bpm 120
   live_loop :beat do
     sample :kick
     sleep 0.25
   end
   ```
2. Verify `commands_to_audio()` produces events at exact beat times:
   - At BPM 120: beat duration = 0.5s
   - `sleep 0.25` = 0.125s
   - Events should be at 0.0, 0.125, 0.25, 0.375, ... seconds
3. Check that cpal/SC scheduler delivers events within ±2ms of target time

### Workflow 6: Stress Test — Maximum Polyphony

1. Create a stress test fixture:
   ```ruby
   # 50 simultaneous voices
   50.times do
     play rrand_i(40, 80), sustain: 4, release: 1
   end
   ```
2. Measure:
   - Does the audio callback complete within buffer time (11.6ms)?
   - Are all voices audible (no voice stealing glitches)?
   - CPU usage during peak polyphony
3. Verify voice mixing in `engine.rs` handles concurrent voices without overflow

## Optimization Checklist

### Parser Optimizations
- [ ] String matching uses efficient patterns (not repeated regex compilation)
- [ ] Line splitting avoids unnecessary allocation
- [ ] `parse_note_value()` uses lookup table for common notes
- [ ] Block nesting doesn't cause O(n²) behavior
- [ ] Variable resolution is O(1) via HashMap

### Audio Engine Optimizations
- [ ] Voice buffer reuse (avoid per-frame allocation)
- [ ] SIMD-friendly sample processing loops
- [ ] Lock-free communication between UI thread and audio thread
- [ ] Audio callback does no heap allocation
- [ ] Effect processing chain avoids unnecessary copies

### Scheduling Optimizations
- [ ] Events pre-sorted by time (not searched linearly each frame)
- [ ] `timeBeginPeriod(1)` on Windows for timer precision
- [ ] Coarse sleep + spin-wait pattern for precise timing
- [ ] No mutex contention in the audio callback hot path

## Troubleshooting

| Problem | Cause | Solution |
|---------|-------|----------|
| Audio glitches | Callback exceeds buffer time | Profile callback; reduce voice count or optimize mixing |
| High CPU idle | Background processing | Check for busy-wait loops or unnecessary polling |
| Parser slow on large files | O(n²) pattern matching | Profile with flamegraph; optimize hot regex patterns |
| Timing drift | Sleep inaccuracy | Use spin-wait for sub-ms precision |
| Memory growth | Allocation leak in audio thread | Use pre-allocated buffers; avoid `Vec::push` in callback |
| First-sound delay | Lazy initialization | Pre-initialize audio device, synth tables, sample cache |

## Key Files

| File | Role |
|------|------|
| `src-tauri/src/audio/engine.rs` | Audio callback hot path, voice mixing |
| `src-tauri/src/audio/parser.rs` | Parser throughput bottleneck |
| `src-tauri/src/audio/synth.rs` | Per-voice DSP processing |
| `src-tauri/src/audio/effects.rs` | Effect chain processing |
| `src-tauri/src/audio/sample.rs` | Sample decoding and playback |
| `src-tauri/tests/parity_validation.rs` | Used as performance smoke test |
| `scripts/full-parity-check.ps1` | Includes compilation timing |
