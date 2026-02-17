# PiBeat Parser Limitations

## Problem Summary

The AI agent generated Sonic Pi code using advanced features that PiBeat's parser previously did not support. Many of these have now been implemented.

## Newly Supported Features

### 1. **`define` Blocks (Custom Functions)** ✅
```ruby
# ✅ NOW SUPPORTED:
define :guitar_riff do
  with_fx :distortion, distort: 0.8 do
    play_pattern_timed [:E2, :G2, :A2], [0.5, 0.5, 0.25], release: 0.3
  end
end

# Call the function by name:
guitar_riff
4.times do
  guitar_riff
end
```

### 2. **Randomization** (`one_in()`, `rrand()`, `rand()`, `dice()`) ✅
```ruby
# ✅ NOW SUPPORTED:
sample :drum_cymbal_hard, amp: 2 if one_in(3)
play :c4, amp: rrand(0.5, 1.0)
sample :hihat, rate: 1 + rrand(-0.02, 0.03)
play :e4, amp: rand(1.0)
```

### 3. **Ring Buffers** (`ring()`) ✅
```ruby
# ✅ NOW SUPPORTED:
kick_pat = ring(1, 0, 0, 0, 0, 1, 0, 0)
# Ring values are stored and can be accessed via .tick
```

### 4. **Euclidean Rhythms** (`spread()`) ✅
```ruby
# ✅ NOW SUPPORTED:
snare_pat = spread(3, 8)
# Generates Bjorklund/Euclidean rhythm patterns stored as ring buffers
```

### 5. **`if` Blocks** ✅
```ruby
# ✅ NOW SUPPORTED:
if one_in(3) do
  sample :drum_cymbal_hard, amp: 2
end

# Trailing if on single lines:
sample :bd_haus, amp: 2 if one_in(2)
```

### 6. **`in_thread` Blocks** ✅
```ruby
# ✅ NOW SUPPORTED:
in_thread do
  10.times do
    guitar_riff
  end
end
```

## Remaining Limitations

### Synchronization (`cue`, `sync:`)
```ruby
# ⚠️ PARSED BUT IGNORED:
# cue/sync are recognized but treated as no-ops
cue :beat_bar
live_loop :kick, sync: :beat_bar do
```

### `control` (runtime synth modification)
```ruby
# ⚠️ PARSED BUT IGNORED:
s = play :c4, sustain: 10
control s, cutoff: 80
# control is recognized but does not modify a running synth
```

### MIDI Commands
```ruby
# ⚠️ PARSED BUT IGNORED:
midi :c4, channel: 1
midi_note_on :c4
# MIDI output is recognized but no MIDI output is generated
```

### `at` Blocks
```ruby
# ⚠️ NOT SUPPORTED:
at [1, 2, 3] do |t|
  play :c4
end
```

### Lambdas / Procs
```ruby
# ⚠️ NOT SUPPORTED:
my_func = lambda { play :c4 }
my_func.call
```

### `time_warp` / `with_swing`
```ruby
# ⚠️ PARSED BUT IGNORED:
time_warp 0.5 do
  play :c4
end
```

## What IS Supported

PiBeat now supports this **comprehensive subset** of Sonic Pi:

✅ **Basic commands:**
- `play`, `sample`, `sleep`, `use_bpm`, `use_synth`, `set_volume`

✅ **Custom functions:**
- `define :name do ... end` — define reusable functions
- Function calls by name (e.g., `guitar_riff`)

✅ **Randomization:**
- `one_in(n)` — probabilistic evaluation (1 in n chance)
- `rrand(min, max)` — random float in range
- `rrand_i(min, max)` — random integer in range
- `rand(max)` — random float 0 to max
- `rand_i(max)` — random integer 0 to max
- `dice(n)` — random integer 1 to n

✅ **Pattern / List Constructors:**
- `ring(values...)` — create ring buffer
- `spread(pulses, steps)` — Euclidean rhythm generation
- `knit(:note, count, ...)` — repeat-interleave pattern
- `range(start, end, step)` — numeric range
- `line(start, finish, steps: n)` — linear interpolation
- `[:c4, :e4, :g4]` — inline array literals

✅ **Scales & Chords (50+ scales, 20+ chord types):**
- `scale(:c4, :minor_pentatonic)` — generate scale notes
- `chord(:e3, :minor7)` — generate chord notes
- Standalone assignment: `notes = scale(:c4, :minor)`
- In-place: `play scale(:c4, :major).choose`
- Scales: major, minor, pentatonic, blues, chromatic, dorian, phrygian, lydian, mixolydian, locrian, harmonic_minor, melodic_minor, whole_tone, diminished, augmented, hirajoshi, iwato, yo, pelog, chinese, enigmatic, spanish, gypsy, etc.
- Chords: major, minor, dim, aug, dom7, min7, maj7, dim7, aug7, sus2, sus4, m9, m11, m13, 7sus2, 7sus4, add9, add11, etc.

✅ **List/Ring Methods:**
- `.choose` — random selection
- `.pick(n)` — pick n random values
- `.tick` — cycling through values via global tick counter
- `.look` — peek current tick value without advancing
- `.first` / `.last` — first/last element
- `.reverse` — reverse order
- `.shuffle` — random order
- `.min` / `.max` — min/max element
- `.ring` / `.stretch(n)` / `.repeat(n)` — list manipulation

✅ **Conditionals:**
- `if condition do ... end` — block conditionals
- `if ... elsif ... else ... end` — full branching
- `unless condition do ... end` — negated conditionals
- Trailing `if` on single lines (e.g., `sample :x if one_in(3)`)
- Trailing `unless` on single lines (e.g., `play :c4 unless false`)
- Numeric comparisons: `>=`, `<=`, `!=`, `==`, `>`, `<`

✅ **Effects:**
- `with_fx :reverb`, `with_fx :echo`, `with_fx :distortion`, `with_fx :lpf`, `with_fx :hpf`

✅ **Loops & Iteration:**
- `live_loop`, `loop do`, `N.times do`, `in_thread`
- `.each do |x| ... end` — iterate over arrays/rings
- `.each_with_index do |x, i| ... end` — iterate with index

✅ **Block Scoping:**
- `with_synth :name do ... end` — temporary synth change
- `with_bpm N do ... end` — temporary BPM change
- `with_bpm_mul N do ... end` — temporary BPM multiplier

✅ **Defaults:**
- `use_synth_defaults amp: 0.5, release: 1.0` — set default synth params
- `use_sample_defaults amp: 0.5` — set default sample params
- `use_merged_synth_defaults` / `use_merged_sample_defaults` — merge into existing defaults

✅ **Sample parameters:**
- `amp`, `rate`, `pan`, `rpitch` (semitone-based rate)
- `start`, `finish` (playback region 0-1)
- `beat_stretch`, `pitch_stretch`

✅ **Note parameters:**
- `amp`, `pan`, `attack`, `decay`, `sustain`, `release`, `cutoff`

✅ **Variables:**
- Simple variable assignment: `my_note = :c4`
- String concatenation: `sample_path + "file.wav"`
- Array/ring assignment: `notes = [:c4, :e4, :g4]`
- Scale/chord assignment: `notes = scale(:c4, :minor)`
- Knit/range/line assignment: `pattern = knit(:e3, 3, :c3, 1)`

✅ **State & Tick:**
- `tick` / `look` — global tick counter
- `set :key, value` / `get[:key]` — shared state (parsed, stored)

✅ **Pragmas (parsed, no-op):**
- `use_random_seed`, `use_random_source`, `use_timing_guarantees`
- `use_arg_checks`, `use_debug`, `use_cue_logging`
- `use_external_synths`, `use_arg_bpm_scaling`
- `sample_duration`
