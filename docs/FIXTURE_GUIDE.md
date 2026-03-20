# PiBeat Fidelity Fixture Guide

This guide explains how to create, run, and validate fidelity fixtures for ensuring parity with Sonic Pi.

## Quick Start

```bash
# Run all fidelity tests
.\validate-parity.ps1 -Full

# Run just snapshot tests
.\validate-parity.ps1 -Snapshots

# Test a specific fixture
.\validate-parity.ps1 -Fixture play_note_basic -Verbose
```

## Fixture Structure

### Location
All fixtures live in `fidelity/fixtures/` as `.rb` files containing valid Sonic Pi code.

### Naming Convention
```
<category>_<feature>_<variant>.rb

Examples:
- play_note_basic.rb
- play_chord_major.rb
- sample_with_params.rb
- with_fx_nested.rb
- live_loop_basic.rb
- ring_basic.rb
```

### Fixture Template
```ruby
# Fixture: <fixture_name>
# Tests: <what behavior is being tested>
# Expected: <what event stream should be produced>

<Sonic Pi code>
```

## Creating a New Fixture

### Step 1: Create the Fixture File

Create `fidelity/fixtures/<feature_name>.rb`:

```ruby
# Fixture: use_synth_defaults
# Tests: Global synth parameter defaults
# Expected: All subsequent play commands use default amp 0.5

use_synth_defaults amp: 0.5
play :c4
sleep 0.5
play :e4
```

### Step 2: Add Snapshot Test

Add to `src-tauri/tests/fidelity_snapshots.rs`:

```rust
// ============================================================================
// FIXTURE: use_synth_defaults
// ============================================================================
#[test]
fn snapshot_use_synth_defaults() {
    let code = "use_synth_defaults amp: 0.5\nplay :c4\nsleep 0.5\nplay :e4";
    let evts = events(code, DEFAULT_BPM);
    let notes = note_events(&evts);
    
    assert_eq!(notes.len(), 2, "should produce 2 notes");
    assert_eq!(notes[0].3, 0.5, "first note amp should be 0.5");
    assert_eq!(notes[1].3, 0.5, "second note amp should be 0.5");
}
```

### Step 3: Run the Test

```bash
cd src-tauri
cargo test snapshot_use_synth_defaults -- --nocapture
```

### Step 4: If Test Fails — Implement the Feature

1. Find the relevant code in `src-tauri/src/audio/parser.rs`
2. Add handling for the new syntax
3. Re-run tests until green
4. Update `docs/fidelity-progress.md` with change log

### Step 5: Update Documentation

Update `parity/PARITY_MATRIX.md` with the new feature status.

## Fixture Categories

### Basic Play Commands
| Fixture | Feature | Status |
|---------|---------|--------|
| `play_note_basic.rb` | `play :note` | ✅ |
| `play_note_params.rb` | `play :note, amp: x, ...` | ✅ |
| `play_midi_number.rb` | `play 60` | ✅ |
| `play_chord_major.rb` | `play chord(:c4, :major)` | ✅ |
| `play_chord_minor7.rb` | `play chord(:a4, :minor7)` | ✅ |
| `play_pattern_timed.rb` | `play_pattern_timed` | ✅ |

### Sample Commands
| Fixture | Feature | Status |
|---------|---------|--------|
| `sample_basic.rb` | `sample :kick` | ✅ |
| `sample_with_params.rb` | `sample :kick, amp: x, rate: y` | ✅ |
| `sample_beat_stretch.rb` | `sample :loop, beat_stretch: 4` | ⬜ Needed |
| `sample_start_finish.rb` | `sample :kick, start: 0.2, finish: 0.8` | ⬜ Needed |

### Timing & Loops
| Fixture | Feature | Status |
|---------|---------|--------|
| `sleep_basic.rb` | `sleep 0.5` | ✅ |
| `use_bpm.rb` | `use_bpm 120` | ✅ |
| `times_loop.rb` | `3.times do ... end` | ✅ |
| `live_loop_basic.rb` | `live_loop :name do ... end` | ✅ |
| `live_loop_multiple.rb` | Multiple concurrent live_loops | ✅ |
| `in_thread_basic.rb` | `in_thread do ... end` | ✅ |
| `while_loop.rb` | `while condition do ... end` | ⬜ Needed |

### Effects
| Fixture | Feature | Status |
|---------|---------|--------|
| `with_fx_reverb.rb` | `with_fx :reverb do ... end` | ✅ |
| `with_fx_nested.rb` | Nested FX blocks | ✅ |
| `with_fx_all.rb` | All FX types | ⬜ Needed |

### State & Randomness
| Fixture | Feature | Status |
|---------|---------|--------|
| `variable_assignment.rb` | `x = :c4; play x` | ✅ |
| `rrand_seeded.rb` | `use_random_seed; rrand` | ✅ |
| `one_in_conditional.rb` | `if one_in(3) do ... end` | ✅ |
| `ring_basic.rb` | `ring(:c4, :e4, :g4)` | ✅ |
| `spread_euclidean.rb` | `spread(3, 8)` | ✅ |
| `set_get.rb` | `set :key, value; get(:key)` | ⬜ Needed |
| `choose_array.rb` | `choose([:c4, :e4, :g4])` | ⬜ Needed |

### Functions
| Fixture | Feature | Status |
|---------|---------|--------|
| `define_function.rb` | `define :melody do ... end` | ✅ |
| `define_with_params.rb` | `define :play_note do |n| ... end` | ⬜ Needed |

### Synths
| Fixture | Feature | Status |
|---------|---------|--------|
| `use_synth_saw.rb` | `use_synth :saw` | ✅ |
| `use_synth_multiple.rb` | Multiple synth changes | ✅ |
| `use_synth_defaults.rb` | `use_synth_defaults amp: 0.5` | ⬜ Needed |
| `all_synths.rb` | Test all synth types | ⬜ Needed |

### Advanced
| Fixture | Feature | Status |
|---------|---------|--------|
| `at_block.rb` | `at [1, 2, 3] do ... end` | ⬜ Needed |
| `time_warp.rb` | `time_warp 0.5 do ... end` | ⬜ Needed |
| `sync_cue.rb` | `sync :foo; cue :foo` | ⬜ Needed |

## Audio Comparison Fixtures

For audio parity validation, you also need:

### Reference Renders
1. Open fixture in Sonic Pi IDE
2. Run the code
3. Record audio using Sonic Pi's Rec button
4. Export to `fidelity/renders/reference/<fixture>.wav`

### Candidate Renders
1. Run the fixture through PiBeat
2. Record the output
3. Save to `fidelity/renders/candidate/<fixture>.wav`

### Running Comparison
```bash
cd src-tauri
cargo test --test audio_compare
```

## Event Stream JSON Snapshots

For critical fixtures, save the expected event stream:

### Generate JSON Snapshot
```rust
// In test code, serialize events to JSON
let evts = events(code, DEFAULT_BPM);
let json = serde_json::to_string_pretty(&evts).unwrap();
// Save to fidelity/event_stream/<fixture>.json
```

### Existing Event Stream Snapshots
```
fidelity/event_stream/
├── default_envelope.json
├── define_function.json
├── play_chord_major.json
├── play_midi_number.json
├── play_note_basic.json
├── play_pattern_timed.json
├── sleep_basic.json
├── times_loop.json
├── use_bpm.json
├── use_synth_saw.json
└── with_fx_reverb.json
```

## Stress Test Fixtures

For performance validation, create stress tests in `fidelity/fixtures/stress/`:

```ruby
# stress_rapid_notes.rb
# Tests: High event rate (1000+ events/sec at 120 BPM)
use_bpm 120
live_loop :stress do
  64.times do
    play :c4, release: 0.05, amp: 0.2
    sleep 0.0625  # 64th notes
  end
end
```

## Debugging Fixtures

### Enable Debug Logging
```powershell
$env:RUST_LOG = "debug"
cargo test snapshot_<name> -- --nocapture
```

### Check Parsed Commands
Add temporary debug output in the test:
```rust
#[test]
fn snapshot_debug() {
    let code = "...";
    let parsed = parse_code(code).expect("parse should succeed");
    println!("Parsed: {:?}", parsed);  // See parsed AST
    let evts = commands_to_audio(&parsed, 60.0);
    for (t, cmd) in &evts {
        println!("t={:.3} {:?}", t, cmd);  // See event stream
    }
}
```

## CI Integration

The fidelity tests can be run in CI:

```yaml
# .github/workflows/fidelity.yml
name: Fidelity Tests
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cd src-tauri && cargo test --test fidelity_snapshots
      - run: cd src-tauri && cargo test --test audio_compare
```

## Contributing New Fixtures

1. **Check existing coverage** — Review `parity/PARITY_MATRIX.md`
2. **Identify gap** — Find untested Sonic Pi feature
3. **Create minimal fixture** — Smallest code that exercises the feature
4. **Add test** — Write Rust test in `fidelity_snapshots.rs`
5. **Validate in Sonic Pi** — Ensure fixture runs correctly in Sonic Pi IDE
6. **Submit PR** — Include fixture, test, and matrix update
