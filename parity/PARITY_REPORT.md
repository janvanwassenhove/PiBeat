# PiBeat Sonic Pi Parity Report

## Summary

This report documents the parser's compatibility with Sonic Pi syntax and identifies gaps between PiBeat's implementation and Sonic Pi's behavior.

## Test Results (Example Files)

| File | Notes | Samples | Status | Previous |
|------|-------|---------|--------|----------|
| Test1 | 32,480 | 48,288 | ✅ Full | 32,480/48,288 |
| Test2 | 1,920 | 1,500 | ✅ Full | 1,920/1,500 |
| Test3 | 6,070 | 4,037 | ✅ Much improved! | 32/8 |
| Test4 | 5,500 | 1,660 | ✅ Full | 5,500/1,660 |
| Test5 | 124 | 5 | ⚠️ Complex threading | 104/5 |

## Session Fixes (2024-02-28)

### 1. Fixed: `get()` in Arithmetic Expressions
**Problem:** User-defined functions like `amp_mod(v)` returning `v * get(:master_amp)` would not evaluate correctly. The `get()` call was being matched inside arithmetic expressions, causing only the `get()` value to be returned, losing the multiplication.

**Solution:** Modified `resolve_numeric()` to only match standalone `get()` calls, not embedded ones.

**Impact:** `amp_mod(2)` now correctly returns `2.0` when `master_amp = 1.0`.

### 2. Fixed: `.times do |i|` Loop Detection
**Problem:** Loops with block variables like `3.times do |i|` were not being detected because `try_extract_times_count()` checked for lines ending with `"do"`, but lines with block variables end with `"|i|"`.

**Solution:** Changed the check from `ends_with("do")` to `contains(" do")`.

**Impact:** Loop variables like `i` are now correctly available inside the loop body.

### 3. Fixed: `in_thread` Variable Scoping
**Problem:** Variable modifications inside `in_thread` blocks (like fade-out threads setting `master_amp`) leaked to the parent scope. This caused `master_amp` to be 0 after parsing a fade-out thread, making all subsequent `amp_mod()` calls return 0.

**Solution:** Save and restore variables on `in_thread` block entry/exit, similar to how synth defaults are scoped.

**Impact:** Threads no longer pollute parent scope variables. Test3 went from 32 notes to 6,070 notes!

### 4. Fixed: Single-Line `if` Block Expansion  
**Problem:** Single-line if blocks like `if get(:stop_all) || get(:pause_all); sleep 1; next; end` were being split incorrectly by semicolons, causing `sleep 1` and `next` to execute unconditionally instead of only when the condition was true.

**Solution:** Updated `join_continuation_lines()` to detect Ruby single-line if/unless blocks and expand them into proper multi-line block structure with `then`/`end`.

**Impact:** Guard clauses in live_loops now work correctly, allowing loops to proceed when conditions are false.

### 5. Fixed: `scale()`/`chord()` in `play_pattern_timed`
**Problem:** `play_pattern_timed scale(:c4, :major), [0.25]` only produced 1 note instead of 8. The `extract_array(line, 0)` was incorrectly grabbing the timing array `[0.25]` as the notes, because for `scale()/chord()` calls there is no bracketed notes array — the first `[...]` IS the timing.

**Solution:** Check if `rest` starts with `scale(`/`chord(` before trying `extract_array`. When function-based notes are used, resolve them via `resolve_to_list()` and use `extract_array(line, 0)` for timings instead of index 1.

**Impact:** `play_pattern_timed scale(:c4, :major), [0.25]` now correctly produces 8 notes (C4-C5).

### 6. Fixed: `time_warp`/`at` Block Clock Advancement
**Problem:** `time_warp` and `at` blocks were advancing the parent timeline clock. In Sonic Pi, both constructs schedule events at offsets but do NOT advance the parent clock — code after a `time_warp`/`at` block runs from the same timeline position as before the block.

**Solution:** In `commands_to_audio`, save `time_offset` before processing `AtBlock`, offset inner events by the saved parent position, then restore `time_offset` to the saved value.

**Impact:** `time_warp` and `at` blocks now have correct Sonic Pi timing semantics.

### 7. Fixed: Empty Code Handling  
**Problem:** `parse_code("")` and comment-only code returned `Err` instead of an empty result, causing unnecessary error handling in callers.

**Solution:** Changed `validate_and_parse` to return `Ok(ParseResult { commands: vec![], warnings: vec![] })` for empty/comment-only code.

**Impact:** Empty buffers and comment-only code no longer produce errors.

### 8. Fixed: do/end Validator for if/while/unless Blocks
**Problem:** The do/end validator only counted `do` keywords as block openers. Ruby `if`, `while`, `unless`, `until`, `begin`, `case` blocks don't use `do` but still close with `end`, causing false "missing end" errors on files like Test5. Conversely, single-line forms like `if cond; action; end` incorrectly counted `if` as a block opener without matching `end` (since `end` is only counted when alone on a line).

**Solution:** Modified `validate_and_parse()` to count `if`/`while`/`unless`/`until`/`begin`/`case` as block openers when they start a line and the line does NOT end with `end` (excluding single-line forms) and does NOT end with `do` (to avoid double-counting).

**Impact:** Test3 (single-line if guards) and Test5 (multi-line if/while blocks) both pass. All 13 example_parsing tests now pass.

### 9. Fixed: Per-Sample ADSR Envelope
**Problem:** Samples had no ADSR envelope support. Parameters like `attack:`, `decay:`, `sustain_level:`, `release:` on `sample` commands were ignored.

**Solution:** Added end-to-end ADSR support for samples:
- Parser extracts `attack:`, `decay:`, `sustain_level:`, `release:` params from sample commands → `Option<Envelope>`
- `commands_to_audio` converts envelope from beats to seconds
- Engine's `SamplePlayback` applies amplitude modulation: attack ramp 0→1, decay ramp 1→sustain_level, sustain hold, release ramp sustain_level→0

**Impact:** Samples now support ADSR envelopes, closing the P2 gap. 5 new tests validate the feature.

## Known Limitations

### P0: Parser Gaps (Syntax Not Accepted)

| Feature | Status | Notes |
|---------|--------|-------|
| `sync/cue` | ⚠️ Parsed, Logged | Commands are recognized, logged; do not block/synchronize |
| `stop` inside live_loop | ⚠️ Partial | May not properly terminate loop |
| Single-line `if` with `next` | ⚠️ Limited | `if cond; action; next; end` on one line |

### P1: Semantic Gaps (Accepted but Runs Wrong)

| Feature | Status | Notes |
|---------|--------|-------|
| `sync: :bar` on live_loop | ⚠️ Parsed | Parameter stored, logged; starts immediately |
| `get(:var)` returns complex expression | ⚠️ Partial | If var holds unevaluated expression string, arithmetic fails |
| `control` command | ⚠️ Parsed, No-op | See Design Decisions below |

### P2: Audio Mismatches

| Feature | Status | Notes |
|---------|--------|-------|
| `beat_stretch:` | ✅ Implemented | Rate adjusted based on sample duration/BPM |
| `start:` / `finish:` | ✅ Implemented | Audio trimming with fade-out |
| `lpf:` on samples | ✅ Implemented | Applied via per-voice FX chain |

### P3: Sample Mismatches

| Feature | Status | Notes |
|---------|--------|-------|
| External file paths | ✅ Supported | File paths in quotes work |
| Symbol lookup | ✅ Supported | `:kick`, `:snare`, etc. |
| `pitch:` parameter | ⚠️ Via rate | Implemented as rate adjustment |

## Test Coverage

### Parser Tests (48 tests)
- All passing ✅
- Includes: `test_amp_mod_user_function_in_params`, `test_set_with_loop_variable_arithmetic`

### Parity Validation Tests (169 tests)
- All passing ✅
- Covers: synths, FX, samples, timing, envelopes, conditionals, ADSR, at/time_warp, variables, define, loops

### Example File Tests (13 tests)
- All passing ✅
- Test1-Test5 parse without errors

### Fidelity Snapshot Tests (57 tests)
- All passing ✅

## Design Decisions

### Why `control` is a No-op

The `control` command in Sonic Pi modifies parameters of a *running* synth node in real-time. PiBeat uses a pre-computed audio timeline model, which means:

1. All notes are scheduled before playback begins
2. There's no concept of "running synth nodes" that can be modified
3. Real-time parameter changes would require a fundamentally different architecture

**Workaround:** Instead of `control`, use multiple notes with explicit timing:

```ruby
# Instead of:
s = play :c4, sustain: 10
sleep 1
control s, note: :e4

# Use:
play :c4, sustain: 1
sleep 1
play :e4, sustain: 9
```

### Why `sync/cue` Doesn't Block

Similar to `control`, Sonic Pi's `sync/cue` provides real-time thread coordination. In our pre-computed model:

- All `live_loop` timelines are computed before playback
- `sync` cannot block waiting for a `cue` since both are evaluated at parse time
- Loops start immediately rather than waiting for sync signals

**Workaround:** Use explicit `sleep` timing to coordinate loops:

```ruby
# Instead of:
live_loop :drums, sync: :go do ... end
cue :go

# Use:
live_loop :drums do
  sleep 4  # Wait 4 beats before starting
  ...
end
```

## Files Modified This Session

- `src-tauri/src/audio/parser.rs`: Added `time_warp` block support, `sync_with` field for loops
- `src-tauri/src/audio/engine.rs`: Added `start:`, `finish:` sample trimming with fade-out
- `src-tauri/src/lib.rs`: Added `beat_stretch` rate calculation using sample duration
