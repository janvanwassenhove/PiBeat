# PiBeat - Copilot Instructions

## Project Overview

PiBeat is a desktop music coding application built with **Tauri v2** (Rust backend) and **React + TypeScript** frontend. It emulates Sonic Pi's live-coding paradigm, letting users write Ruby-like music code in a Monaco editor and hear the results in real time via a custom Rust audio engine.

## Architecture

### Frontend (React + TypeScript + Vite)
- **State management**: Zustand (single store in `src/store.ts`)
- **Editor**: Monaco Editor with custom `sonicpi` language definition
- **UI Framework**: Plain React with CSS custom properties (dark theme)
- **Tauri IPC**: `@tauri-apps/api/core` invoke calls to Rust backend

### Backend (Rust / Tauri)
- **Audio engine**: `src-tauri/src/audio/engine.rs` — playback, mixing, master volume
- **Parser**: `src-tauri/src/audio/parser.rs` — parses Sonic Pi syntax into audio commands
- **Synth**: `src-tauri/src/audio/synth.rs` — oscillators (sine, saw, square, triangle, super_saw, etc.)
- **Effects**: `src-tauri/src/audio/effects.rs` — reverb, delay, distortion, LPF, HPF
- **Samples**: `src-tauri/src/audio/sample.rs` — WAV sample playback
- **Recorder**: `src-tauri/src/audio/recorder.rs` — live recording to WAV

### Key Frontend Components
| Component | File | Purpose |
|-----------|------|---------|
| App | `src/App.tsx` | Main layout with header, body (editor + panels), footer |
| Toolbar | `src/components/Toolbar.tsx` | Run/Stop/Record buttons, volume, BPM, panel toggles |
| BufferTabs | `src/components/BufferTabs.tsx` | Sonic Pi-style numbered buffer tabs (0-9) |
| CodeEditor | `src/components/CodeEditor.tsx` | Monaco editor with sonicpi language, completions, theme |
| WaveformVisualizer | `src/components/WaveformVisualizer.tsx` | Real-time waveform canvas |
| LogPanel | `src/components/LogPanel.tsx` | Timestamped log output |
| SampleBrowser | `src/components/SampleBrowser.tsx` | Browse and preview samples |
| EffectsPanel | `src/components/EffectsPanel.tsx` | Global effect knobs |
| HelpPanel | `src/components/HelpPanel.tsx` | Quick reference for Sonic Pi syntax |
| AgentChat | `src/components/AgentChat.tsx` | AI assistant chat for code help and refactoring |

### Store Shape (Zustand)
```
buffers[], activeBufferId, isPlaying, isRecording, masterVolume, bpm,
waveform[], logs[], samples[], effects{}, showSampleBrowser, showEffectsPanel,
showHelp, showAgentChat, agentMessages[]
```

### Tauri Commands (invoke)
- `run_code(code)` → `RunResult`
- `stop_audio()`
- `set_volume(volume)`, `set_bpm(bpm)`
- `start_recording()`, `stop_recording(path?)`
- `get_waveform()`, `get_status()`, `get_logs()`, `clear_logs()`
- `list_samples()`, `play_sample_file(path)`
- `set_effects({...})`
- `get_env_var(key)` → `string | null` — Read system environment variables (used for API keys)

## Sonic Pi Language Reference

This section is the comprehensive knowledge base for the AI agent chat feature. The agent uses this to provide accurate code suggestions, refactorings, and explanations.

### Core Concepts

Sonic Pi code runs top-to-bottom. `sleep` advances the timeline. All durations are in beats (relative to BPM). Notes can be MIDI numbers or symbol names (`:c4`, `:fs3`).

### Playing Notes
```ruby
play :c4                           # Play middle C
play 60                            # MIDI note 60 = C4
play :c4, amp: 0.5                 # Half volume
play :c4, sustain: 1               # Hold for 1 beat
play :c4, attack: 0.1, release: 0.5
play :c4, pan: -1                  # Hard left
play chord(:c4, :major)            # Play a chord
play_pattern_timed [:c4,:e4,:g4], [0.5]
```

### Sleep / Timing
```ruby
sleep 0.5      # Wait half a beat
sleep 1        # Wait one beat
use_bpm 140    # Set tempo
```

### Synths
```ruby
use_synth :sine        # Smooth sine wave
use_synth :saw         # Bright sawtooth
use_synth :square      # Hollow square wave
use_synth :triangle    # Soft triangle wave
use_synth :noise       # White noise
use_synth :pulse       # Pulse wave
use_synth :super_saw   # Detuned supersaw (fat)
use_synth :tb303       # Acid bass
use_synth :prophet     # Prophet-style synth
use_synth :blade       # Blade Runner pad
use_synth :pluck       # Plucked string
use_synth :fm          # FM synthesis
use_synth :beep        # Simple beep
```

### Samples
```ruby
sample :kick                       # Kick drum
sample :snare                      # Snare
sample :hihat                      # Hi-hat
sample :clap                       # Clap
sample :kick, amp: 0.8, rate: 1.5  # Modify playback
sample :loop_amen                  # Classic breakbeat
sample :loop_breakbeat
sample :ambi_choir                 # Ambient pad
sample :bass_hit_c                 # Bass hit
```

**⚠️ Unsupported sample parameters:**
```ruby
# ❌ NOT SUPPORTED:
sample :kick, beat_stretch: 4      # NOT IMPLEMENTED
sample :kick, pitch: 0.5           # NOT IMPLEMENTED
sample :kick, start: 0.25          # NOT IMPLEMENTED
sample :kick, finish: 0.75         # NOT IMPLEMENTED
sample :kick, lpf: 80              # NOT IMPLEMENTED (use with_fx :lpf instead)
```

**✅ SUPPORTED sample parameters:**
- `amp` — Volume (0.0 – 1.0+)
- `rate` — Playback speed (0.1 – 4.0)
- `pan` — Stereo panning (-1 to 1)

### Common Parameters
| Parameter | Description | Range |
|-----------|-------------|-------|
| `amp` | Volume | 0.0 – 1.0+ |
| `pan` | Stereo panning | -1 (left) to 1 (right) |
| `attack` | Fade-in time (beats) | 0+ |
| `decay` | Decay time | 0+ |
| `sustain` | Hold time | 0+ |
| `release` | Fade-out time | 0+ |
| `rate` | Sample playback speed | 0.1 – 4.0 |
| `cutoff` | Low-pass filter cutoff | 0 – 130 (MIDI note) |
| `res` | Filter resonance | 0.0 – 1.0 |

### Loops
```ruby
# Live loop — runs continuously, can be hot-reloaded
live_loop :beat do
  sample :kick
  sleep 0.5
  sample :hihat, amp: 0.6
  sleep 0.5
end

# Simple loop
loop do
  play :c4
  sleep 1
end

# Numbered iteration
3.times do
  play :e4
  sleep 0.25
end
```

### Effects (FX)
```ruby
with_fx :reverb, mix: 0.5, room: 0.8 do
  play :c4
end

with_fx :echo, time: 0.25, feedback: 0.6 do
  play :c4
end

with_fx :distortion, distort: 0.5 do
  play :c3
end

with_fx :lpf, cutoff: 80 do
  play :c4
end

with_fx :hpf, cutoff: 50 do
  play :c4
end

with_fx :flanger, phase: 0.5 do
  play :c4
end

with_fx :slicer, phase: 0.25 do
  play :c4, sustain: 2
end

with_fx :bitcrusher, bits: 8 do
  play :c4
end
```

### Chords & Scales
```ruby
play chord(:c4, :major)            # C major chord
play chord(:a3, :minor)            # A minor chord
play chord(:d4, :dom7)             # D dominant 7th

play_pattern_timed scale(:c4, :major), [0.25]
play_pattern_timed scale(:a4, :minor_pentatonic), [0.125]
```

### Randomisation
**✅ NOW SUPPORTED** — The parser supports randomisation functions:
```ruby
# ✅ SUPPORTED:
use_random_seed 42                 # Parsed (seed not tracked, but no error)
play :c4, amp: rrand(0.5, 1.0)     # Random float in range
play :c4, amp: rrand_i(50, 80)     # Random integer in range
play :c4, amp: rand(1.0)           # Random float 0 to max
sample :kick, amp: rrand(0.5, 1)   # rrand in parameters
sample :kick if one_in(3)           # Trailing if with one_in
play :c4, amp: dice(6)             # Random 1 to n

# ✅ SUPPORTED — if blocks with one_in:
if one_in(3) do
  sample :drum_cymbal_hard, amp: 2
end

# ⚠️ NOT YET SUPPORTED:
play choose([:c4, :e4, :g4])       # choose() NOT IMPLEMENTED
```

### Rings & Ticks
**✅ PARTIALLY SUPPORTED** — Ring buffers and Euclidean rhythms are now parsed:
```ruby
# ✅ SUPPORTED:
notes = ring(:c4, :e4, :g4, :b4)   # Ring buffer values stored
rhythm = spread(3, 8)              # Euclidean rhythm generation
kick_pat = ring(1, 0, 0, 0)        # Numeric ring patterns

# ⚠️ PARTIAL — .tick/.look cycling approximated:
play notes.tick                    # Approximated probabilistically
play notes.look                    # Approximated probabilistically
```

**✅ You CAN also use explicit sequencing:**
```ruby
live_loop :arp do
  play :c4
  sleep 0.25
  play :e4
  sleep 0.25
  play :g4
  sleep 0.25
  play :b4
  sleep 0.25
end
```

### Threads & Sync
**⚠️ PARTIALLY IMPLEMENTED** — `in_thread` works, but `cue`/`sync` are NOT implemented:
```ruby
# ✅ SUPPORTED:
in_thread do
  loop do
    sample :kick
    sleep 1
  end
end

in_thread do
  loop do
    play :c4
    sleep 0.5
  end
end

# ❌ NOT SUPPORTED:
in_thread do
  sync :go             # NOT IMPLEMENTED
  play :c4
end
cue :go                # NOT IMPLEMENTED

live_loop :kick, sync: :bar do   # sync: parameter NOT IMPLEMENTED
  sample :kick
  sleep 1
end
```

**✅ WORKAROUND:** Use separate `live_loop` blocks without `sync` — they'll run concurrently:
```ruby
live_loop :kick do
  sample :kick
  sleep 1
end

live_loop :snare do
  sleep 1
  sample :snare
  sleep 1
end
```

### Variables & Functions
```ruby
my_note = :c4
play my_note

define :melody do
  play :c4
  sleep 0.25
  play :e4
  sleep 0.25
  play :g4
  sleep 0.5
end
melody   # Call the function
```

### Common Patterns & Idioms

**Basic beat:**
```ruby
live_loop :drums do
  sample :kick
  sleep 0.5
  sample :hihat, amp: 0.6
  sleep 0.25
  sample :hihat, amp: 0.4
  sleep 0.25
  sample :snare
  sleep 0.5
  sample :hihat, amp: 0.6
  sleep 0.25
  sample :hihat, amp: 0.4
  sleep 0.25
end
```

**Arpeggiated synth:**
```ruby
# ✅ WORKING VERSION (without ring/tick):
live_loop :arp do
  use_synth :saw
  play :c4, amp: 0.3, release: 0.2
  sleep 0.125
  play :e4, amp: 0.3, release: 0.2
  sleep 0.125
  play :g4, amp: 0.3, release: 0.2
  sleep 0.125
  play :b4, amp: 0.3, release: 0.2
  sleep 0.125
  play :c5, amp: 0.3, release: 0.2
  sleep 0.125
end
```

**Ambient pad:**
```ruby
live_loop :pad do
  use_synth :blade
  with_fx :reverb, mix: 0.7, room: 0.9 do
    play chord(:c4, :minor7), amp: 0.2, attack: 2, sustain: 4, release: 2
    sleep 8
  end
end
```

**Acid bass:**
```ruby
# ✅ WORKING VERSION (without ring/tick/rrand):
live_loop :acid do
  use_synth :tb303
  8.times do
    play :c2, cutoff: 70, release: 0.2, amp: 0.5
    sleep 0.25
  end
  2.times do
    play :eb2, cutoff: 85, release: 0.2, amp: 0.5
    sleep 0.25
  end
  2.times do
    play :f2, cutoff: 100, release: 0.2, amp: 0.5
    sleep 0.25
  end
end
```

### Refactoring Tips for the Agent

**⚠️ Parser Notes**

The PiBeat parser supports an **extended subset** of Sonic Pi. When generating or refactoring code:

**✅ NOW SUPPORTED:**
- ✅ `define :name do ... end` — Custom function definitions and calls
- ✅ `ring()` — Ring buffer creation and storage
- ✅ `spread()` — Euclidean rhythm pattern generation
- ✅ `rrand()`, `rrand_i()`, `rand()`, `rand_i()`, `dice()` — Randomisation
- ✅ `one_in(n)` — Probabilistic evaluation (in `if` blocks and trailing `if`)
- ✅ `if condition do ... end` — Block conditionals
- ✅ Trailing `if` on single lines (e.g., `sample :x if one_in(3)`)

**⚠️ PARTIAL / LIMITED:**
- ⚠️ `.tick`, `.look` — Ring values stored but runtime cycling is approximated
- ⚠️ `cue`, `sync:` — Recognized but treated as no-ops
- ⚠️ `beat_stretch:`, `pitch:`, `start:`, `finish:` — Parsed but not audio-applied

**❌ NOT SUPPORTED:**
- ❌ `choose()` — Use explicit values instead 
- ❌ `at` blocks — Not fully implemented

When the agent suggests refactorings, it should follow these principles:

1. **Use explicit timing** — Write out `sleep` calls for clear timing
2. **Use `define` for reusable sections** — Define functions and call them by name
3. **Use randomisation freely** — `rrand()`, `one_in()`, `rand()` all work
4. **Nest `with_fx` sensibly** — Avoid deeply nested FX chains; prefer sequential application
5. **Parameterise values** — Extract magic numbers into named variables or `define` blocks
6. **Use `live_loop` over `loop`** — Enables hot-reloading and is idiomatic Sonic Pi
7. **Add `sleep` inside every loop** — Forgetting `sleep` causes infinite loops
8. **Break long code into multiple `live_loop` blocks** — One for drums, one for bass, one for melody, etc.
9. **Keep patterns simple** — Complexity should come from layering multiple simple loops, not complex logic

## Coding Conventions

- **TypeScript**: strict mode, functional React components, Zustand for state
- **CSS**: CSS custom properties, BEM-like class naming, no CSS modules
- **Rust**: standard Tauri v2 patterns, `#[tauri::command]` for IPC
- **Icons**: `react-icons/fa` (Font Awesome)
- **No external UI libraries** — plain HTML/CSS components

## Agent Chat / LLM Integration

The agent chat system (`src/components/AgentChat.tsx`, `src/llm.ts`, `src/agent.ts`) supports:
- **Multi-provider**: OpenAI, Anthropic, or local rule-based fallback
- **Reactive agent pattern**: Self-reflection with up to 2 iteration cycles
- **API key sources** (priority order):
  1. System environment variables (via Tauri `get_env_var` command)
  2. Vite `.env` file (build-time)
  3. localStorage (Settings UI)

### Important Implementation Notes:
- **GPT-5 models** (gpt-5.2, gpt-5-mini, gpt-5-nano):
  - Use `max_completion_tokens: 8192` (not `max_tokens`)
  - Only support default `temperature: 1.0` (custom values not allowed)
  - Higher token limit critical to avoid truncation with long system context
  - The `callOpenAI()` function auto-detects via `config.model.startsWith('gpt-5')`
- **GPT-4 models** (gpt-4o, gpt-4o-mini):
  - Use legacy `max_tokens: 4096`
  - Support custom `temperature: 0.7`
- **Claude models** (all versions):
  - Use `max_tokens: 4096` (Anthropic's standard limit)
  - Use `temperature: 1.0` (Anthropic's default)
  - System message passed separately via `system` parameter
- Empty strings `""` are treated as no API key (converted to `undefined`)
- Comprehensive console logging for debugging (search for `[LLM]`, `[getApiKey]`, `[AgentChat]`, `[callOpenAI]`, `[callAnthropic]` prefixes)
- Token limits must be high enough to accommodate 4920-char system context + user message + response

## When Adding New Features

1. Update this file with the new feature's description, component location, and any new Tauri commands
2. If adding new Sonic Pi syntax support, add it to the language reference section above
3. Add completions to `CodeEditor.tsx` if new keywords are introduced
4. Update the store interface in `src/store.ts` if new state is needed
