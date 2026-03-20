---
name: pibeat-integration
description: "PiBeat full-stack integration patterns. Use when working on Tauri IPC boundary, data flow between Rust and TypeScript, polling loops, cross-window communication, build/dev workflow, file dialogs, keyboard shortcuts, SuperCollider engine switching, or end-to-end feature wiring. Covers invoke patterns, event system, real-time data flow, and deployment."
---

# PiBeat Integration Skill

Complete knowledge of how PiBeat's Rust backend and React frontend communicate — Tauri IPC patterns, polling loops, event-driven updates, cross-window messaging, and end-to-end feature wiring.

## When to Use This Skill

- Wiring a new Tauri command from Rust to TypeScript
- Debugging IPC serialization issues
- Working on real-time data flow (waveform, active lines, logs, status)
- Adding cross-window communication (detached panels, band visualizer)
- Configuring Tauri permissions, capabilities, or windows
- Working on build/dev tooling (Vite, Cargo, scripts)
- Implementing file dialogs or keyboard shortcuts
- Switching between cpal and SuperCollider engines
- End-to-end testing of features that span both stacks

## IPC Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Frontend (TypeScript)                                       │
│                                                              │
│  Component → Store Action → invoke('command', { args })      │
│                                ↕ (async, JSON serialized)    │
├──────────────────────────────────────────────────────────────┤
│  Tauri IPC Bridge (message passing, not FFI)                 │
├──────────────────────────────────────────────────────────────┤
│  Backend (Rust)                                              │
│                                                              │
│  #[tauri::command] fn command(args, state: State<Arc<...>>)  │
│                        → Result<T, String>                   │
└─────────────────────────────────────────────────────────────┘
```

### Invoke Pattern

**Rust side** (`lib.rs`):
```rust
#[tauri::command]
fn my_command(arg1: String, arg2: f32, state: tauri::State<Arc<AppState>>) -> Result<MyResult, String> {
    // Access state via state.inner()
    Ok(result)
}

// Register in plugin builder:
.invoke_handler(tauri::generate_handler![
    my_command,
    // ... all other commands
])
```

**TypeScript side** (store or component):
```typescript
import { invoke } from '@tauri-apps/api/core';

// In Zustand action:
const result = await invoke<MyResult>('my_command', { arg1: 'value', arg2: 1.5 });
// ⚠️ Parameter names must match Rust fn params exactly (snake_case)
// ⚠️ Types are JSON-serialized: Rust's HashMap → JS object, Vec → array, Option → nullable
```

### Serialization Rules

| Rust Type | TypeScript Type | Notes |
|-----------|----------------|-------|
| `String` | `string` | UTF-8 |
| `f32` / `f64` | `number` | JSON numbers |
| `bool` | `boolean` | |
| `Vec<T>` | `T[]` | |
| `HashMap<K, V>` | `Record<K, V>` | Keys must be strings |
| `Option<T>` | `T \| null` | |
| `Result<T, String>` | Promise resolves `T` or rejects with string | |
| Custom `#[derive(Serialize)]` struct | Matching interface | Field names auto snake_case→camelCase? **NO** — Tauri preserves Rust field names |

**⚠️ Important**: Tauri v2 preserves Rust field names as-is. If Rust uses `snake_case`, TypeScript must also use `snake_case` when consuming the response. PiBeat uses snake_case consistently.

## Command Registry

All commands registered in `lib.rs`:
```rust
.invoke_handler(tauri::generate_handler![
    run_code,
    stop_audio,
    pause_audio,
    resume_audio,
    set_volume,
    set_bpm,
    start_recording,
    stop_recording,
    get_waveform,
    get_status,
    get_active_lines,
    get_logs,
    clear_logs,
    list_samples,
    play_sample_file,
    get_sample_durations,
    set_effects,
    set_user_samples_dir,
    get_user_samples_dir,
    scan_user_samples,
    discover_user_samples,
    analyze_user_sample,
    get_env_var,
    preview_synth,
    init_sc,
    toggle_sc_engine,
    get_sc_status,
    reload_synthdefs,
    get_performance_snapshot,
    // ... visual engine commands
])
```

## Real-Time Data Flow

### Polling Loops (App.tsx)

PiBeat uses client-side polling (not push-based events) for real-time data:

| Data | Interval | Condition | Store Action |
|------|----------|-----------|-------------|
| Waveform | 50ms | `isPlaying` | `updateWaveform()` → `invoke('get_waveform')` |
| Active lines | 50ms | `isPlaying` | `updateActiveLines()` → `invoke('get_active_lines')` |
| Engine status | 1000ms | Always | `fetchStatus()` → `invoke('get_status')` |
| Logs | On demand | After run | `fetchLogs()` → `invoke('get_logs')` |
| SC status | 1000ms | SC enabled | `fetchScStatus()` → `invoke('get_sc_status')` |

```typescript
// Typical polling pattern in App.tsx:
useEffect(() => {
  if (!isPlaying) return;
  const id = setInterval(() => {
    updateWaveform();
    updateActiveLines();
  }, 50);
  return () => clearInterval(id);
}, [isPlaying]);
```

### Data Flow: Code Execution

```
User clicks "Run"
  → Toolbar onClick → store.runCode()
    → invoke('run_code', { code: activeBuffer.code })
      → Rust: parse_code() → commands_to_audio() → spawn scheduler thread
      → Rust returns RunResult { success, logs, duration_estimate, effective_bpm }
    → Store updates: isPlaying=true, logs, setupTimeMs
    → Polling starts: waveform (50ms), activeLines (50ms), status (1s)
    → Scheduler thread sends AudioCommands to engine via crossbeam channel
    → cpal audio callback reads commands, mixes voices, outputs audio
```

### Data Flow: User Samples

```
User sets sample directory
  → invoke('set_user_samples_dir', { dir })
  → invoke('discover_user_samples')  ← Fast: returns file list with metadata
  → For each sample: invoke('analyze_user_sample', { path })  ← Slow: BPM detection, tagging
  → Results cached in store + localStorage (incremental scanning)
```

## Cross-Window Communication

### Detachable Panels
- Panels can be "detached" into separate Tauri windows
- `DetachablePanel` HOC manages window lifecycle
- `detachedPanels: Record<string, boolean>` tracked in store + localStorage

### Event System (Tauri Events)
```typescript
// Send event to all windows:
import { emit } from '@tauri-apps/api/event';
await emit('panel-update', { panelId: 'sampleBrowser', data: ... });

// Listen in any window:
import { listen } from '@tauri-apps/api/event';
const unlisten = await listen('panel-update', (event) => {
  // Handle update
});
```

### Band Visualizer Window
- Separate window via `bandMain.tsx` entry point
- Receives performance events from Rust's `VisualEngine`
- Communication: Rust → `EventPublisher` (bounded channel, try_send) → Tauri event → JS
- Never blocks audio: `try_send` drops events if channel full

## Tauri Configuration (`tauri.conf.json`)

### Key Settings
```json
{
  "app": {
    "windows": [
      { "label": "main", "title": "PiBeat", "width": 1400, "height": 900 }
    ]
  },
  "bundle": {
    "identifier": "com.pibeat.app"
  }
}
```

### Permissions / Capabilities
- File dialog access (save/load buffers, select sample dir)
- Global shortcuts (Ctrl+Enter = run, Ctrl+. = stop)
- Environment variable reading (API keys)
- Window management (create/close for detached panels)

## Plugin Integration

### File Dialogs (`@tauri-apps/plugin-dialog`)
```typescript
import { open, save } from '@tauri-apps/plugin-dialog';

// Open file
const path = await open({ filters: [{ name: 'Ruby', extensions: ['rb'] }] });

// Save file
const path = await save({ filters: [{ name: 'Ruby', extensions: ['rb'] }] });
```

### Global Shortcuts (`@tauri-apps/plugin-global-shortcut`)
```typescript
import { register } from '@tauri-apps/plugin-global-shortcut';
await register('CommandOrControl+Enter', () => store.getState().runCode());
await register('CommandOrControl+.', () => store.getState().stopAudio());
```

## Build & Development

### Dev Workflow
```bash
# Start Tauri dev (hot-reload for both Rust and TypeScript):
npm run tauri dev

# Frontend only (no Rust backend):
npm run dev

# TypeScript check:
npx tsc --noEmit

# Rust check:
cd src-tauri && cargo check

# Run Rust tests:
cd src-tauri && cargo test

# Build release:
npm run tauri build
```

### Build Pipeline
```
Vite builds TypeScript/React → dist/
Cargo builds Rust → src-tauri/target/
Tauri bundles both → installer (.msi on Windows, .dmg on macOS)
```

### Environment Variables
| Variable | Purpose | Used By |
|----------|---------|---------|
| `OPENAI_API_KEY` | OpenAI LLM access | `src/llm.ts` via `invoke('get_env_var')` |
| `ANTHROPIC_API_KEY` | Anthropic LLM access | `src/llm.ts` via `invoke('get_env_var')` |
| `VITE_OPENAI_API_KEY` | Build-time fallback | `import.meta.env` |
| `VITE_ANTHROPIC_API_KEY` | Build-time fallback | `import.meta.env` |

## SuperCollider Engine Switching

### Toggle Flow
```
User toggles SC in Toolbar
  → store.toggleScEngine(true)
    → invoke('toggle_sc_engine', { enabled: true })
      → Rust: state.use_sc.store(true)
      → If no ScEngine yet: init_sc() → find_sc_bundle_dir() or system SC
      → Load SynthDefs → verify /status
    → fetchScStatus() starts polling
  → Next run_code() dispatches to ScEngine instead of AudioEngine
```

### Fallback
- If SC `/status` fails → auto-fallback to cpal
- `use_sc` flag controls routing in `run_code()` scheduler

## Wiring a New End-to-End Feature (Checklist)

1. **Rust command** (`lib.rs`):
   - Implement `#[tauri::command]` function
   - Add to `generate_handler![]` list
   - Define request/response types with `#[derive(Serialize, Deserialize)]`

2. **Tauri permissions** (`tauri.conf.json` or capabilities):
   - Add any new OS-level permissions needed

3. **Store action** (`src/store.ts`):
   - Add state fields for the feature
   - Add action that calls `invoke()` with correct command name and params
   - Handle errors with try/catch

4. **UI component** (`src/components/`):
   - Create or modify component
   - Bind to store via `useStore()` selectors
   - Call store actions on user interaction

5. **Toolbar/Panel integration**:
   - Add toggle if it's a panel feature
   - Add to `App.tsx` layout

6. **Testing**:
   - Rust unit/integration tests in `src-tauri/tests/`
   - Manual verification via `npm run tauri dev`

## Troubleshooting

| Issue | Likely Cause | Fix |
|-------|-------------|-----|
| `invoke()` returns undefined | Wrong command name (case-sensitive) | Match snake_case exactly |
| Serialization error | Type mismatch Rust↔TS | Check all fields match (names + types) |
| Command not found | Not in `generate_handler![]` | Add to handler list |
| Permission denied | Missing Tauri capability | Add to `tauri.conf.json` capabilities |
| Polling doesn't start | Missing `isPlaying` dependency | Check useEffect deps array |
| Detached panel empty | PanelHost doesn't render component | Add panel to PanelHost switch |
| Window doesn't open | Wrong label or missing config | Check `tauri.conf.json` windows |
| Event not received | Wrong event name or scope | Match emit/listen event names |
| SC commands fail | scsynth not found | Check sc-bundle/ or system PATH |
| Build fails | Vite + Cargo version mismatch | Check tauri.conf.json `beforeBuildCommand` |
