---
name: pibeat-frontend
description: "PiBeat React/TypeScript frontend development. Use when working on React components, Zustand store, Monaco editor, CSS theming, LLM/agent integration, timeline views, panel system, or any src/ TypeScript code. Covers all 18 components, store shape, Tauri invoke patterns, detachable panels, and real-time polling loops."
---

# PiBeat Frontend Development Skill

Complete knowledge of the PiBeat React 19 + TypeScript frontend — every component, store action, LLM integration detail, and UI pattern.

## When to Use This Skill

- Creating or modifying React components in `src/components/`
- Changing Zustand store state, actions, or selectors
- Working on the Monaco editor (language definition, completions, themes)
- Modifying LLM/agent integration (`src/llm.ts`, `src/agent.ts`)
- Adjusting timeline parsing/sync (`src/timelineParser.ts`, `src/timelineSync.ts`)
- Styling changes (CSS custom properties, themes)
- Adding new panel types or detachable windows
- Working on waveform visualization, logging, or sample browsing
- Debugging Tauri invoke calls from the frontend

## Project Structure

```
src/
├── main.tsx                 # Entry point, renders <App />
├── bandMain.tsx             # Entry for detached band visualizer window
├── App.tsx                  # Main layout: header, body (editor+panels), footer
├── App.css                  # Global styles, theme variables, layout
├── store.ts                 # Zustand store (50+ fields, 70+ actions)
├── llm.ts                   # LLM API calls (OpenAI, Anthropic)
├── agent.ts                 # Reactive agent logic, quick actions, system prompt
├── timelineParser.ts        # Parse Sonic Pi code → timeline clips
├── timelineSync.ts          # Bidirectional code ↔ timeline sync
├── vite-env.d.ts            # Vite type declarations
└── components/
    ├── AgentChat.tsx         # AI chat panel
    ├── BandControlPanel.tsx  # Band visualizer controls
    ├── BandVisualizer.tsx    # Audio-reactive band animation
    ├── BandVisualizerWindow.tsx # Detached band window
    ├── BufferTabs.tsx        # Code buffer tab bar
    ├── CodeEditor.tsx        # Monaco editor wrapper
    ├── CuePanel.tsx          # Cue/sync event viewer
    ├── DetachablePanel.tsx   # HOC for pop-out panel windows
    ├── EffectsPanel.tsx      # Global effect knobs
    ├── HelpPanel.tsx         # Sonic Pi quick reference
    ├── LogPanel.tsx          # Log output with timestamps
    ├── PanelHost.tsx         # Hosts detached panels
    ├── SampleBrowser.tsx     # Built-in sample browser
    ├── SynthBrowser.tsx      # Synth type browser with preview
    ├── TimelineView.tsx      # Visual timeline editor
    ├── Toolbar.tsx           # Main toolbar with all controls
    ├── UserSamplePanel.tsx   # User sample browser
    └── WaveformVisualizer.tsx # Real-time audio scope
```

## Dependencies

| Package | Version | Purpose |
|---------|---------|---------|
| react | 19.1 | UI framework |
| react-dom | 19.1 | DOM rendering |
| zustand | 5.x | State management |
| @monaco-editor/react | 4.7 | Code editor |
| @tauri-apps/api | 2.x | Tauri IPC (invoke) |
| @tauri-apps/plugin-dialog | 2.x | File open/save dialogs |
| @tauri-apps/plugin-global-shortcut | 2.x | Keyboard shortcuts |
| openai | 6.x | OpenAI API SDK |
| @anthropic-ai/sdk | 0.74 | Claude API SDK |
| react-icons | 5.x | Font Awesome icons (fa) |
| typescript | 5.8 | Type checking (strict mode) |
| vite | 7.x | Build + dev server |

## Store Reference (`src/store.ts`)

### Type Definitions

```typescript
type AppTheme = 'pibeat' | 'sonicpi' | 'amber';

interface Buffer { id: number; name: string; code: string; }
interface LogEntry { timestamp: number; level: string; message: string; }
interface SampleInfo { name: string; category: string; }
interface UserSampleInfo {
  name: string; path: string; file_type: string; duration_secs: number;
  sample_rate: number; bpm_estimate: number | null; audio_type: string;
  feeling: string; tags: string[]; folder: string;
}
interface DiscoveredSample { name: string; path: string; file_type: string; folder: string; file_size: number; modified_ms: number; }
interface ScanProgress { current: number; total: number; current_file: string; }
interface EngineStatus { is_playing: boolean; master_volume: number; bpm: number; is_recording: boolean; }
interface RunResult { success: boolean; message: string; logs: LogEntry[]; duration_estimate: number; effective_bpm: number; setup_time_ms: number; }
interface ScStatus { running: boolean; version: string; latency: number; }
interface AgentMessage { role: 'user' | 'assistant'; content: string; }
interface CueEvent { name: string; timestamp: number; buffer?: string; }
interface EffectSettings { reverb_mix: number; delay_time: number; delay_feedback: number; distortion: number; lpf_cutoff: number; hpf_cutoff: number; }
```

### State Fields (50+)

| Category | Fields |
|----------|--------|
| Buffers | `buffers: Buffer[]`, `activeBufferId: number` |
| Engine | `isPlaying`, `isPaused`, `isRecording`, `masterVolume`, `bpm`, `setupTimeMs` |
| SC | `scStatus: ScStatus` |
| Audio data | `waveform: number[]`, `logs: LogEntry[]`, `samples: SampleInfo[]` |
| Active lines | `activeLines: number[]`, `errorLine: number \| null` |
| User samples | `userSamples`, `userSamplesDir`, `userSamplesLoading`, `userSamplesScanProgress` |
| Effects | `effects: EffectSettings` |
| UI toggles | `showSampleBrowser`, `showSynthBrowser`, `showEffectsPanel`, `showHelp`, `showAgentChat`, `showCuePanel`, `showBandVisualizer`, `showUserSamplePanel` |
| Detached | `detachedPanels: Record<string, boolean>` |
| Agent | `agentMessages`, `agentProvider: LLMProvider`, `agentModel: ModelId` |
| View | `theme: AppTheme`, `viewMode: 'code' \| 'timeline'` |
| Cache | `sampleDurations: Record<string, number>`, `sampleDurationsLoading` |

### Action Groups

**Buffer management**: `setActiveBuffer`, `updateBufferCode`, `renameBuffer`, `addBuffer`, `removeBuffer`, `saveBufferToFile`, `loadBufferFromFile`

**Playback**: `runCode` (invokes `run_code`), `stopAudio`, `pauseAudio`, `resumeAudio`

**Settings**: `setVolume` (invokes `set_volume`), `setBpm` (invokes `set_bpm`)

**Recording**: `startRecording`, `stopRecording`

**Polling**: `updateWaveform` (50ms during playback), `updateActiveLines` (50ms), `fetchStatus` (1s), `fetchLogs`, `fetchSamples`

**Effects**: `setEffects` (invokes `set_effects` with partial EffectSettings)

**Samples**: `playSampleFile`, `fetchSampleDurations`

**User samples**: `setUserSamplesDir`, `scanUserSamples`, `fullRescanUserSamples`, `loadUserSamplesDir`

**Panel toggles**: `toggleSampleBrowser`, `toggleSynthBrowser`, `toggleEffectsPanel`, `toggleHelp`, `toggleAgentChat`, `toggleCuePanel`, `toggleUserSamplePanel`, `toggleBandVisualizer`, `toggleDetachPanel`

**Agent**: `addAgentMessage`, `clearAgentMessages`, `setAgentProvider`, `setAgentModel`

**SC**: `initSuperCollider`, `toggleScEngine`, `fetchScStatus`

**Misc**: `previewSynth`, `addLog`, `setErrorLine`, `clearLogs`, `addCue`, `clearCues`, `setViewMode`, `setTheme`, `toggleViewMode`

## Component Patterns

### Tauri Invoke Pattern
```typescript
import { invoke } from '@tauri-apps/api/core';

// Inside store actions:
const result = await invoke<RunResult>('run_code', { code: buffer.code });
// All invoke calls use snake_case command names matching Rust #[tauri::command] fns
```

### Real-Time Polling (App.tsx)
```typescript
// Waveform: ~50ms interval when playing
useEffect(() => {
  if (!isPlaying) return;
  const id = setInterval(() => updateWaveform(), 50);
  return () => clearInterval(id);
}, [isPlaying]);

// Active lines: ~50ms interval 
// Status: 1000ms interval
// Logs: polled on demand or interval
```

### Detachable Panel Pattern
```tsx
<DetachablePanel
  panelId="sampleBrowser"
  title="Sample Browser"
  isOpen={showSampleBrowser}
  isDetached={detachedPanels['sampleBrowser']}
  onToggleDetach={() => toggleDetachPanel('sampleBrowser')}
  onClose={() => toggleSampleBrowser()}
>
  <SampleBrowser />
</DetachablePanel>
```
- `detachedPanels` persisted in `localStorage`
- Detached panels open as separate Tauri windows
- Cross-window communication via Tauri events

### Monaco Editor Configuration (CodeEditor.tsx)
- Custom language ID: `sonicpi`
- Token provider for Ruby-like syntax highlighting
- Completion provider with all Sonic Pi keywords, synths, samples, effects
- Three themes: `pibeat-dark`, `sonicpi-dark`, `amber-dark`
- Active line highlighting via `deltaDecorations` (green background during playback)
- Error line highlighting (red background on log click)

## LLM Integration

### Architecture (`src/llm.ts`)
```typescript
type LLMProvider = 'openai' | 'anthropic' | 'local';
type ModelId = 'gpt-5.2' | 'gpt-5-mini' | 'gpt-5-nano' | 'gpt-4o' | 'gpt-4o-mini' 
             | 'claude-sonnet-4-5' | 'claude-sonnet-4' | 'claude-haiku' | /* etc */;

// API key resolution (priority):
// 1. System env var via invoke('get_env_var', { key }) 
// 2. Vite import.meta.env.VITE_*
// 3. localStorage
```

### GPT-5 vs GPT-4 Differences
| Feature | GPT-5 variants | GPT-4 variants |
|---------|---------------|---------------|
| Token param | `max_completion_tokens: 8192` | `max_tokens: 4096` |
| Temperature | Only `1.0` (default) | Custom `0.7` |
| Detection | `model.startsWith('gpt-5')` | Everything else |

### Agent System (`src/agent.ts`)
- **System prompt**: ~850 lines of Sonic Pi knowledge base (from copilot-instructions.md)
- **Reactive pattern**: User message → LLM response → optional self-reflection (up to 2 rounds)
- **Quick actions**: 14+ pre-built actions (generate beat, create intro, fade in/out, etc.)
- **Code context**: Reads current buffer, extracts BPM/key/synths, generates compatible code
- **Code insertion**: Extracted code blocks from LLM response inserted into active buffer

## CSS Theming

### Theme Variables (App.css)
```css
[data-theme="pibeat"] {
  --bg-primary: #1a1a2e;    --bg-secondary: #16213e;
  --accent: #4a9eff;        --text-primary: #e0e0e0;
}
[data-theme="sonicpi"] {
  --bg-primary: #1a2e1a;    --bg-secondary: #162e16;
  --accent: #4aff4a;        --text-primary: #e0e0e0;
}
[data-theme="amber"] {
  --bg-primary: #2e2a1a;    --bg-secondary: #2e2816;
  --accent: #ffaa4a;        --text-primary: #e0e0e0;
}
```

### Conventions
- All colors via CSS custom properties
- No CSS modules (plain CSS with BEM-like naming)
- Scrollbar styling via `::-webkit-scrollbar`
- Responsive panels with flex layout
- Consistent border-radius, padding across panels

## Troubleshooting

| Issue | Likely Cause | Fix |
|-------|-------------|-----|
| Panel doesn't show | Missing toggle in `App.tsx` | Add conditional render + store toggle |
| Invoke fails silently | Wrong command name or params | Check snake_case name matches Rust fn |
| Waveform not updating | Polling not started | Verify `isPlaying` dependency in useEffect |
| Theme not applied | Missing CSS var | Add to all three `[data-theme]` blocks |
| LLM returns nothing | API key not found | Check env → .env → localStorage chain |
| Detached panel blank | `PanelHost` not rendering | Check panel registration in PanelHost |
| Active lines wrong | Line intervals stale | Verify `get_active_lines` polling |
| Agent code garbled | Token limit too low | Increase max_completion_tokens for GPT-5 |
