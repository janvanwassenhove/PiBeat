# Performance Audit

An audit of what PiBeat spends time and CPU on, covering both **visual loading**
(how long the window takes to become useful) and **playback** (what runs while
music is playing, and while it is not).

Each finding below states what was measured or read, what it cost, and what
changed. Numbers come from a headless Chromium run against the Vite dev server
with the Tauri IPC layer mocked, and from reading the scheduler code paths.

---

## Visual loading

### Monaco was fetched from a CDN at every start

`@monaco-editor/react` defaults to `@monaco-editor/loader`, which pulls Monaco
from jsDelivr at runtime. PiBeat never called `loader.config({ monaco })`, so
the editor — the middle of the entire window — could not appear until a network
round trip completed, and **could not appear at all offline**. For a packaged
desktop app that is a correctness problem as much as a speed one.

Monaco is now bundled. `src/monacoSetup.ts` binds the wrapper to the local copy
and imports `monaco-editor/esm/vs/editor/edcore.main` rather than the default
entry: the complete editor (find, folding, suggest, multi-cursor) without the
TypeScript, CSS, HTML and JSON language services. Those services and their web
workers are roughly 9 MB of code for languages PiBeat never opens — it only
edits its own Sonic Pi dialect, registered as a Monarch grammar. `monaco-editor`
is now a declared dependency instead of an implicitly hoisted peer.

### Nothing rendered until the whole bundle had parsed

The editor now loads behind `React.lazy` (`CodeEditorLazy.tsx`) with a skeleton
holding its place, so the toolbar, buffer tabs, scope and log panel paint first.

| | Before | After |
|---|---|---|
| Entry chunk | 672 kB (168 kB gzip) | 207 kB (61 kB gzip) |
| App shell visible | after entry chunk + Monaco CDN fetch | ~350 ms |
| Editor interactive | after CDN fetch | ~1.2 s |

### The LLM SDKs were in the startup bundle

`openai`, `@anthropic-ai/sdk` and `@google/genai` total ~450 kB and are useless
unless the user actually sends a message to that provider. `src/llm.ts` now
imports them with `import type` and pulls the runtime in with `import()` at
first use. React is split into its own chunk so it can be fetched and compiled
in parallel with the app code.

---

## Idle cost

### The scope polled and redrew forever

`WaveformVisualizer` ran a 33 ms `get_waveform` poll and a `requestAnimationFrame`
redraw loop for as long as the app was open, playing or not. Each frame built
three gradients and a shadow-blurred stroke to draw a flat line. On the
SuperCollider path every poll also pumped the OSC socket.

Both are now gated on playback, with a final fetch and repaint after stopping so
the tail of the sound is what stays on screen, and a repaint from the resize
observer since there is no animation frame to cover the clear.

**Measured IPC calls over 3 seconds idle: 93 → 3.**

### The visual engine animated a band nobody was watching

`VisualEngine::run_loop` rebuilt a full `PerformanceSnapshot` — cloning the
config, every band member, lighting and crowd state — at the target frame rate
whenever it was enabled, with no idea whether the band panel was even open.
Between runs the snapshot was byte-identical every time.

It now drops to a 10 Hz tick once playback has stopped *and* every director has
decayed to rest, and wakes immediately on the next event.

---

## Playback

### Per-event logging on the scheduler thread

Every scheduled note went through `eprintln!` in two places — the scheduler and
`ScEngine::play_note` — plus one per FX push/pop and one per runtime variable.
A dense piece meant tens of thousands of formatted writes through a **locked**
stderr handle, on the very thread responsible for dispatching notes on time.

These now go through `trace!` (`src-tauri/src/trace.rs`), which costs one
relaxed atomic load when off. Enable with `PIBEAT_TRACE=1`. Messages that fire
once per run — parse summary, engine selection, errors — still print
unconditionally; they are what makes a bug report useful.

### The scheduler spin-waited before every note

The dispatch loop slept until 18 ms before each event's due time, then burned a
core in `std::hint::spin_loop()` for the remainder. With thousands of events
that is a hot core for the length of the piece.

Timing no longer depends on waking up at the right instant: events go to scsynth
in timestamped OSC bundles and it places them on the exact sample. The loop
sleeps until the dispatch window and no longer spins. (This is also the parity
fix — see `parity/PARITY_MATRIX.md`.)

### `play_note` drained the OSC socket on every note

After each `/s_new`, `play_note` flipped the socket to non-blocking, tried up to
five `recvfrom` calls looking for `/fail`, then flipped it back — around seven
syscalls per note, on the scheduling thread. Worse, it *consumed* replies that
`process_incoming` needed.

Removed. Failures still reach the log panel: `process_incoming` handles `/fail`
and the once-per-second status poll drains them.

### `session_id` was a mutex checked twice per event

Scheduler threads compare `session_id` against the session they started for, to
notice cancellation — twice per scheduled event, through a `parking_lot::Mutex`
also taken by `run_code` and `stop_audio`. Now an `AtomicU64`.

### `get_active_lines` was quadratic

Called on a 50 ms poll for the whole of playback. It deduplicated results with
`Vec::contains` and scanned intervals past the current time. Intervals are now
sorted by start time when built, so the scan stops at the first one that has not
begun, and dedup uses a bitmap.

---

## Frontend re-renders

Every component subscribed to the **entire** store: `const { … } = useStore()`
with no selector re-renders on any state change whatsoever. A log line arriving,
the 1 Hz status tick, an active-line update — each re-rendered the whole tree,
including the component hosting Monaco.

All 16 call sites now select the fields they use through `useShallow`.

---

## Not addressed

- **The built-in (cpal) engine has no master limiter.** The Sonic Pi master
  chain is implemented on the SuperCollider path only.
- **The cpal scheduler still spin-waits.** It has no equivalent of an OSC
  timetag to hand the work to — precision there genuinely does depend on the
  thread waking on time. Fixing it properly means moving scheduling into the
  audio callback, which is a larger change than this audit took on.
- **`live_loop` expansion is eager.** A loop without `stop` is unrolled to 500
  iterations up front, which is why a long piece can generate 100k events and
  why the command cap exists. A lazy scheduler would remove the cap and the
  memory, and is the single biggest remaining structural change.
