import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';
import type { LLMProvider, ModelId } from './llm';

export type AppTheme = 'pibeat' | 'sonicpi' | 'amber';

// ---- User Sample Cache Helpers ----
const SAMPLE_CACHE_KEY = 'pibeat-user-samples-cache';

function loadSampleCache(dir: string): Record<string, CachedSample> {
  try {
    const raw = localStorage.getItem(SAMPLE_CACHE_KEY);
    if (!raw) return {};
    const data = JSON.parse(raw);
    // Cache is keyed by directory; only return if directory matches
    if (data._dir !== dir) return {};
    const cache: Record<string, CachedSample> = {};
    for (const [path, entry] of Object.entries(data)) {
      if (path === '_dir') continue;
      cache[path] = entry as CachedSample;
    }
    return cache;
  } catch {
    return {};
  }
}

function saveSampleCache(dir: string, samples: UserSampleInfo[], discovered: DiscoveredSample[]) {
  try {
    // Build a lookup of file metadata by path
    const metaByPath: Record<string, { file_size: number; modified_ms: number }> = {};
    for (const d of discovered) {
      metaByPath[d.path] = { file_size: d.file_size, modified_ms: d.modified_ms };
    }
    const data: Record<string, unknown> = { _dir: dir };
    for (const s of samples) {
      const meta = metaByPath[s.path];
      if (meta && s.duration_secs > 0) {
        // Only cache fully analyzed samples
        data[s.path] = {
          file_size: meta.file_size,
          modified_ms: meta.modified_ms,
          info: s,
        };
      }
    }
    localStorage.setItem(SAMPLE_CACHE_KEY, JSON.stringify(data));
  } catch (e) {
    console.warn('[SampleCache] Failed to save cache:', e);
  }
}

function clearSampleCache() {
  localStorage.removeItem(SAMPLE_CACHE_KEY);
}

/** Load all cached UserSampleInfo entries for a given directory (for instant startup). */
function loadCachedSampleInfos(dir: string): UserSampleInfo[] {
  try {
    const raw = localStorage.getItem(SAMPLE_CACHE_KEY);
    if (!raw) return [];
    const data = JSON.parse(raw);
    if (data._dir !== dir) return [];
    const infos: UserSampleInfo[] = [];
    for (const [key, entry] of Object.entries(data)) {
      if (key === '_dir') continue;
      const cached = entry as CachedSample;
      if (cached.info) infos.push(cached.info);
    }
    return infos;
  } catch {
    return [];
  }
}
// ---- End Cache Helpers ----

// ---- Buffer Cache Helpers ----
const BUFFER_CACHE_KEY = 'pibeat-buffers-cache';

function loadBufferCache(): Buffer[] | null {
  try {
    const raw = localStorage.getItem(BUFFER_CACHE_KEY);
    if (!raw) return null;
    const data = JSON.parse(raw) as Buffer[];
    if (!Array.isArray(data) || data.length === 0) return null;
    return data;
  } catch {
    return null;
  }
}

function saveBufferCache(buffers: Buffer[]) {
  try {
    localStorage.setItem(BUFFER_CACHE_KEY, JSON.stringify(buffers));
  } catch (e) {
    console.warn('[BufferCache] Failed to save cache:', e);
  }
}
// ---- End Buffer Cache Helpers ----

export interface LogEntry {
  timestamp: number;
  level: string;
  message: string;
}

export interface SampleInfo {
  name: string;
  path: string;
  category: string;
}

export interface UserSampleInfo {
  name: string;
  path: string;
  file_type: string;
  duration_secs: number;
  sample_rate: number;
  bpm_estimate: number | null;
  audio_type: string;
  feeling: string;
  tags: string[];
  folder: string;
}

export interface DiscoveredSample {
  name: string;
  path: string;
  file_type: string;
  folder: string;
  file_size: number;
  modified_ms: number;
}

// Cached sample entry stored in localStorage for incremental scanning
interface CachedSample {
  file_size: number;
  modified_ms: number;
  info: UserSampleInfo;
}

export interface ScanProgress {
  scanned: number;
  total: number;
  phase: 'discovering' | 'analyzing' | 'done';
}

export interface EngineStatus {
  is_playing: boolean;
  master_volume: number;
  bpm: number;
  is_recording: boolean;
}

export interface RunResult {
  success: boolean;
  message: string;
  logs: LogEntry[];
  duration_estimate: number;
  effective_bpm: number;
  setup_time_ms: number;
}

export interface ScStatus {
  available: boolean;
  booted: boolean;
  enabled: boolean;
  message: string;
}

export interface Buffer {
  id: number;
  name: string;
  code: string;
}

export interface AgentMessage {
  role: 'user' | 'assistant';
  content: string;
}

export interface CueEvent {
  id: number;
  name: string;
  timestamp: number;
  buffer?: string;
}

interface EffectSettings {
  reverb_mix: number;
  delay_time: number;
  delay_feedback: number;
  distortion: number;
  lpf_cutoff: number;
  hpf_cutoff: number;
}

interface AppStore {
  // Buffers (like Sonic Pi's multiple code buffers)
  buffers: Buffer[];
  activeBufferId: number;
  
  // Engine status
  isPlaying: boolean;
  isPaused: boolean;
  playingBufferId: number | null;
  isRecording: boolean;
  masterVolume: number;
  bpm: number;
  setupTimeMs: number;
  
  // SuperCollider status
  scStatus: ScStatus;
  
  // Waveform
  waveform: number[];
  
  // Logs
  logs: LogEntry[];
  
  // Samples
  samples: SampleInfo[];
  
  // User Samples
  userSamples: UserSampleInfo[];
  userSamplesDir: string | null;
  userSamplesLoading: boolean;
  userSamplesScanProgress: ScanProgress | null;
  showUserSamplePanel: boolean;
  
  // Effects
  effects: EffectSettings;
  
  // UI state
  theme: AppTheme;
  viewMode: 'code' | 'timeline';
  showSampleBrowser: boolean;
  showSynthBrowser: boolean;
  showEffectsPanel: boolean;
  showHelp: boolean;
  showAgentChat: boolean;
  showCuePanel: boolean;
  showBandVisualizer: boolean;

  // Detached panels
  detachedPanels: Record<string, boolean>;
  
  // Agent
  agentMessages: AgentMessage[];
  agentProvider: LLMProvider;
  agentModel: ModelId;
  
  // Cues
  cueEvents: CueEvent[];
  
  // Sample duration cache for timeline visualization
  sampleDurations: Record<string, number>;
  sampleDurationsLoading: boolean;
  
  // Active lines for code highlighting during playback
  activeLines: number[];
  
  // Error line highlight (set when user clicks an error/warning in log)
  errorLine: number | null;
  
  // Actions
  setActiveBuffer: (id: number) => void;
  updateBufferCode: (id: number, code: string) => void;
  renameBuffer: (id: number, name: string) => void;
  addBuffer: () => void;
  removeBuffer: (id: number) => void;
  saveBufferToFile: (id?: number) => Promise<void>;
  loadBufferFromFile: (id?: number) => Promise<void>;
  
  runCode: () => Promise<void>;
  stopAudio: () => Promise<void>;
  pauseAudio: () => Promise<void>;
  resumeAudio: () => Promise<void>;
  
  setVolume: (vol: number) => Promise<void>;
  setBpm: (bpm: number) => Promise<void>;
  
  startRecording: () => Promise<void>;
  stopRecording: (path?: string) => Promise<void>;
  
  updateWaveform: () => Promise<void>;
  updateActiveLines: () => Promise<void>;
  fetchStatus: () => Promise<void>;
  fetchSamples: () => Promise<void>;
  fetchLogs: () => Promise<void>;
  clearLogs: () => Promise<void>;
  
  setEffects: (effects: Partial<EffectSettings>) => Promise<void>;
  
  playSampleFile: (path: string) => Promise<void>;
  
  toggleViewMode: () => void;
  setViewMode: (mode: 'code' | 'timeline') => void;
  setTheme: (theme: AppTheme) => void;
  toggleSampleBrowser: () => void;
  toggleSynthBrowser: () => void;
  toggleEffectsPanel: () => void;
  toggleHelp: () => void;
  toggleAgentChat: () => void;
  toggleCuePanel: () => void;
  toggleUserSamplePanel: () => void;
  toggleBandVisualizer: () => void;
  toggleDetachPanel: (panelId: string) => void;

  previewSynth: (synthName: string) => Promise<void>;

  // User Samples actions
  setUserSamplesDir: (dir: string) => Promise<void>;
  scanUserSamples: () => Promise<void>;
  fullRescanUserSamples: () => Promise<void>;
  loadUserSamplesDir: () => Promise<void>;

  addAgentMessage: (msg: AgentMessage) => void;
  clearAgentMessages: () => void;
  setAgentProvider: (provider: LLMProvider) => void;
  setAgentModel: (model: ModelId) => void;

  addLog: (level: string, message: string) => void;
  setErrorLine: (line: number | null) => void;

  // Sample duration actions
  fetchSampleDurations: (names: string[]) => Promise<void>;
  
  // SuperCollider actions
  initSuperCollider: () => Promise<void>;
  toggleScEngine: (enabled: boolean) => Promise<void>;
  fetchScStatus: () => Promise<void>;
  
  // Cue actions
  addCue: (name: string, buffer?: string) => void;
  clearCues: () => void;
}

const DEFAULT_CODE = `# Welcome to PiBeat!
# Write code to make music, just like Sonic Pi

# Play a simple melody
use_synth :sine
play :c4, amp: 0.5, sustain: 0.3
sleep 0.5
play :e4, amp: 0.5, sustain: 0.3
sleep 0.5
play :g4, amp: 0.5, sustain: 0.3
sleep 0.5
play :c5, amp: 0.7, sustain: 0.8
`;

const DEMO_BEAT = `# Drum Beat Pattern
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
`;

const DEMO_SYNTH = `# Synth Pad
use_synth :super_saw
play :c4, amp: 0.3, sustain: 2, attack: 0.5, release: 1
sleep 0.5
play :e4, amp: 0.3, sustain: 2, attack: 0.5, release: 1
sleep 0.5
play :g4, amp: 0.3, sustain: 2, attack: 0.5, release: 1
`;

export const useStore = create<AppStore>((set, get) => ({
  buffers: loadBufferCache() || [
    { id: 0, name: 'Buffer 0', code: DEFAULT_CODE },
    { id: 1, name: 'Buffer 1', code: DEMO_BEAT },
    { id: 2, name: 'Buffer 2', code: DEMO_SYNTH },
    { id: 3, name: 'Buffer 3', code: '# Empty buffer\n' },
  ],
  activeBufferId: 0,
  isPlaying: false,
  isPaused: false,
  playingBufferId: null,
  isRecording: false,
  masterVolume: 1.0,
  bpm: 120,
  setupTimeMs: 0,
  scStatus: { available: false, booted: false, enabled: false, message: 'Not initialized' },
  waveform: new Array(2048).fill(0),
  logs: [],
  samples: [],
  effects: {
    reverb_mix: 0.0,
    delay_time: 0.0,
    delay_feedback: 0.0,
    distortion: 0.0,
    lpf_cutoff: 20000,
    hpf_cutoff: 20,
  },
  viewMode: 'code',
  theme: (localStorage.getItem('pibeat-theme') as AppTheme) || 'pibeat',
  showSampleBrowser: false,
  showSynthBrowser: false,
  showEffectsPanel: false,
  showHelp: false,
  showAgentChat: false,
  showCuePanel: false,
  showBandVisualizer: false,
  showUserSamplePanel: false,
  detachedPanels: JSON.parse(localStorage.getItem('pibeat-detached-panels') || '{}'),
  userSamples: (() => {
    const dir = localStorage.getItem('pibeat-user-samples-dir');
    return dir ? loadCachedSampleInfos(dir) : [];
  })(),
  userSamplesDir: localStorage.getItem('pibeat-user-samples-dir'),
  userSamplesLoading: false,
  userSamplesScanProgress: null,
  agentMessages: [],
  agentProvider: 'local',
  agentModel: 'local-rules',
  cueEvents: [],
  sampleDurations: {},
  sampleDurationsLoading: false,
  activeLines: [],
  errorLine: null,

  setActiveBuffer: (id) => set({ activeBufferId: id }),

  updateBufferCode: (id, code) => {
    set((state) => {
      const newBuffers = state.buffers.map(b => b.id === id ? { ...b, code } : b);
      saveBufferCache(newBuffers);
      return { buffers: newBuffers };
    });
  },

  renameBuffer: (id, name) => {
    set((state) => {
      const newBuffers = state.buffers.map(b => b.id === id ? { ...b, name } : b);
      saveBufferCache(newBuffers);
      return { buffers: newBuffers };
    });
  },

  addBuffer: () => set((state) => {
    const maxId = state.buffers.length > 0 ? Math.max(...state.buffers.map(b => b.id)) : -1;
    const newBuffers = [...state.buffers, {
      id: maxId + 1,
      name: `Buffer ${maxId + 1}`,
      code: '# New buffer\n',
    }];
    saveBufferCache(newBuffers);
    return { buffers: newBuffers, activeBufferId: maxId + 1 };
  }),

  removeBuffer: (id) => set((state) => {
    if (state.buffers.length <= 1) return state; // Keep at least one buffer
    const newBuffers = state.buffers.filter(b => b.id !== id);
    const newActive = state.activeBufferId === id ? newBuffers[0]?.id ?? 0 : state.activeBufferId;
    saveBufferCache(newBuffers);
    return { buffers: newBuffers, activeBufferId: newActive };
  }),

  saveBufferToFile: async (id?) => {
    const state = get();
    const bufferId = id ?? state.activeBufferId;
    const buffer = state.buffers.find(b => b.id === bufferId);
    if (!buffer) return;

    try {
      const filePath = await save({
        title: 'Save Sonic Pi Code',
        defaultPath: `${buffer.name.replace(/[^a-zA-Z0-9_-]/g, '_')}.sonicpi`,
        filters: [
          { name: 'Sonic Pi Files', extensions: ['sonicpi'] },
          { name: 'Ruby Files', extensions: ['rb'] },
          { name: 'All Files', extensions: ['*'] },
        ],
      });
      if (!filePath) return; // User cancelled

      await invoke('save_code_to_file', { path: filePath, content: buffer.code });
      get().addLog('info', `Saved buffer "${buffer.name}" to ${filePath}`);
    } catch (e: any) {
      get().addLog('error', `Save failed: ${e}`);
    }
  },

  loadBufferFromFile: async (id?) => {
    const state = get();
    const bufferId = id ?? state.activeBufferId;

    try {
      const filePath = await open({
        title: 'Open Sonic Pi Code',
        multiple: false,
        filters: [
          { name: 'Sonic Pi Files', extensions: ['sonicpi'] },
          { name: 'Ruby Files', extensions: ['rb'] },
          { name: 'All Files', extensions: ['*'] },
        ],
      });
      if (!filePath) return; // User cancelled

      const path = typeof filePath === 'string' ? filePath : filePath;
      const code = await invoke<string>('read_code_from_file', { path });

      // Extract filename for buffer name
      const fileName = path.split(/[\\/]/).pop() || 'Loaded';
      const baseName = fileName.replace(/\.(sonicpi|rb)$/i, '');

      set((s) => {
        const newBuffers = s.buffers.map(b =>
          b.id === bufferId ? { ...b, code, name: baseName } : b
        );
        saveBufferCache(newBuffers);
        return { buffers: newBuffers };
      });
      get().addLog('info', `Loaded "${fileName}" into buffer`);
    } catch (e: any) {
      get().addLog('error', `Load failed: ${e}`);
    }
  },

  runCode: async () => {
    const state = get();
    const buffer = state.buffers.find(b => b.id === state.activeBufferId);
    if (!buffer) return;

    // Extract cues from code (live_loop names and explicit cue calls)
    const liveLoopMatches = buffer.code.matchAll(/live_loop\s+:(\w+)/g);
    for (const m of liveLoopMatches) {
      get().addCue(m[1], buffer.name);
    }
    const cueMatches = buffer.code.matchAll(/\bcue\s+:(\w+)/g);
    for (const m of cueMatches) {
      get().addCue(m[1], buffer.name);
    }

    try {
      const result = await invoke<RunResult>('run_code', { code: buffer.code });
      set({ isPlaying: true, isPaused: false, playingBufferId: buffer.id, bpm: result.effective_bpm || get().bpm, setupTimeMs: result.setup_time_ms || 0 });
      if (result.logs.length > 0) {
        set((s) => ({
          logs: [...s.logs, ...result.logs].slice(-500),
        }));
      }
      get().addLog('info', result.message);
      // Log duration estimate
      if (result.duration_estimate > 0) {
        get().addLog('info', `Estimated duration: ${result.duration_estimate.toFixed(1)}s`);
      }
    } catch (e: any) {
      const errorMsg = typeof e === 'string' ? e : e?.message || JSON.stringify(e);
      get().addLog('error', `Code error: ${errorMsg}`);
      console.error('[runCode] Backend error:', e);
      set({ isPlaying: false, playingBufferId: null });
    }
  },

  stopAudio: async () => {
    try {
      await invoke('stop_audio');
      set({ isPlaying: false, isPaused: false, playingBufferId: null, activeLines: [] });
      get().addLog('info', 'Stopped');
    } catch (e: any) {
      get().addLog('error', `Error stopping: ${e}`);
    }
  },

  pauseAudio: async () => {
    try {
      await invoke('pause_audio');
      set({ isPaused: true });
      get().addLog('info', 'Paused');
    } catch (e: any) {
      get().addLog('error', `Pause error: ${e}`);
    }
  },

  resumeAudio: async () => {
    try {
      await invoke('resume_audio');
      set({ isPaused: false });
      get().addLog('info', 'Resumed');
    } catch (e: any) {
      get().addLog('error', `Resume error: ${e}`);
    }
  },

  setVolume: async (vol) => {
    try {
      await invoke('set_volume', { volume: vol });
      set({ masterVolume: vol });
    } catch (e) {
      console.error(e);
    }
  },

  setBpm: async (bpm) => {
    try {
      await invoke('set_bpm', { bpm });
      set({ bpm });
    } catch (e) {
      console.error(e);
    }
  },

  startRecording: async () => {
    try {
      await invoke('start_recording');
      set({ isRecording: true });
      get().addLog('info', 'Recording started');
    } catch (e: any) {
      get().addLog('error', `Recording error: ${e}`);
    }
  },

  stopRecording: async (path?) => {
    try {
      let savePath = path ?? null;
      if (!savePath) {
        const chosen = await save({
          title: 'Save Recording',
          defaultPath: 'recording.wav',
          filters: [{ name: 'WAV Audio', extensions: ['wav'] }],
        });
        if (!chosen) {
          // User cancelled — keep recording
          return;
        }
        savePath = chosen;
      }
      const result = await invoke<string>('stop_recording', { path: savePath });
      set({ isRecording: false });
      get().addLog('info', `Recording saved: ${result}`);
    } catch (e: any) {
      get().addLog('error', `Save error: ${e}`);
    }
  },

  updateWaveform: async () => {
    // Waveform is now fetched directly by WaveformVisualizer via refs.
    // This store action is kept for backward compatibility but is a no-op.
  },

  updateActiveLines: async () => {
    try {
      const activeLines = await invoke<number[]>('get_active_lines');
      // Only update state if the lines actually changed (avoid unnecessary re-renders)
      const prev = get().activeLines;
      if (
        prev.length !== activeLines.length ||
        prev.some((v, i) => v !== activeLines[i])
      ) {
        set({ activeLines });
      }
    } catch (e) {
      if (get().activeLines.length > 0) {
        set({ activeLines: [] });
      }
    }
  },

  fetchStatus: async () => {
    try {
      const status = await invoke<EngineStatus>('get_status');
      // Only update state if values actually changed
      const s = get();
      if (
        s.isPlaying !== status.is_playing ||
        s.masterVolume !== status.master_volume ||
        s.bpm !== status.bpm ||
        s.isRecording !== status.is_recording
      ) {
        set({
          isPlaying: status.is_playing,
          isPaused: status.is_playing ? s.isPaused : false,
          playingBufferId: status.is_playing ? s.playingBufferId : null,
          masterVolume: status.master_volume,
          bpm: status.bpm,
          isRecording: status.is_recording,
        });
      }
    } catch (e) {
      // Ignore
    }
  },

  fetchSamples: async () => {
    try {
      const samples = await invoke<SampleInfo[]>('list_samples');
      set({ samples });
    } catch (e) {
      console.error(e);
    }
  },

  fetchLogs: async () => {
    try {
      const logs = await invoke<LogEntry[]>('get_logs');
      set({ logs });
    } catch (e) {
      // Ignore
    }
  },

  clearLogs: async () => {
    try {
      await invoke('clear_logs');
      set({ logs: [] });
    } catch (e) {
      // Ignore
    }
  },

  setEffects: async (partial) => {
    const current = get().effects;
    const newEffects = { ...current, ...partial };
    set({ effects: newEffects });
    try {
      await invoke('set_effects', newEffects);
    } catch (e) {
      console.error(e);
    }
  },

  playSampleFile: async (path) => {
    try {
      await invoke('play_sample_file', { path });
    } catch (e: any) {
      get().addLog('error', `Failed to play sample: ${e}`);
    }
  },

  toggleViewMode: () => set((s) => ({ viewMode: s.viewMode === 'code' ? 'timeline' : 'code' })),
  setViewMode: (mode) => set({ viewMode: mode }),
  setTheme: (theme) => {
    localStorage.setItem('pibeat-theme', theme);
    set({ theme });
    emit('theme-changed', { theme });
  },
  toggleSampleBrowser: () => set((s) => ({ showSampleBrowser: !s.showSampleBrowser })),
  toggleSynthBrowser: () => set((s) => ({ showSynthBrowser: !s.showSynthBrowser })),
  toggleEffectsPanel: () => set((s) => ({ showEffectsPanel: !s.showEffectsPanel })),
  toggleHelp: () => set((s) => ({ showHelp: !s.showHelp })),
  toggleAgentChat: () => set((s) => ({ showAgentChat: !s.showAgentChat })),
  toggleCuePanel: () => set((s) => ({ showCuePanel: !s.showCuePanel })),
  toggleUserSamplePanel: () => set((s) => ({ showUserSamplePanel: !s.showUserSamplePanel })),
  toggleBandVisualizer: () => set((s) => ({ showBandVisualizer: !s.showBandVisualizer })),
  toggleDetachPanel: (panelId) => set((s) => {
    const newDetached = { ...s.detachedPanels };
    if (newDetached[panelId]) {
      delete newDetached[panelId];
    } else {
      newDetached[panelId] = true;
    }
    localStorage.setItem('pibeat-detached-panels', JSON.stringify(newDetached));
    return { detachedPanels: newDetached };
  }),

  previewSynth: async (synthName) => {
    try {
      await invoke('preview_synth', { synthName });
    } catch (e: any) {
      get().addLog('error', `Failed to preview synth: ${e}`);
    }
  },

  setUserSamplesDir: async (dir: string) => {
    try {
      await invoke('set_user_samples_dir', { dir });
      localStorage.setItem('pibeat-user-samples-dir', dir);
      set({ userSamplesDir: dir });
      get().addLog('info', `User samples directory set to: ${dir}`);
      // Auto-scan after setting directory
      await get().scanUserSamples();
    } catch (e: any) {
      get().addLog('error', `Failed to set user samples directory: ${e}`);
    }
  },

  scanUserSamples: async () => {
    const dir = get().userSamplesDir;
    if (!dir) {
      get().addLog('error', 'No user samples directory set');
      return;
    }
    try {
      // Ensure backend knows the directory
      await invoke('set_user_samples_dir', { dir });

      // Phase 1: Fast discovery — get file listing with metadata (no loading UI yet)
      const discovered = await invoke<DiscoveredSample[]>('discover_user_samples');
      const total = discovered.length;

      // Phase 2: Load cache and determine what needs analysis
      const cache = loadSampleCache(dir);
      const cachedPaths = new Set(Object.keys(cache));
      const discoveredPaths = new Set(discovered.map((d) => d.path));

      const toAnalyze: DiscoveredSample[] = [];
      const cachedResults: UserSampleInfo[] = [];

      for (const d of discovered) {
        const cached = cache[d.path];
        if (cached && cached.file_size === d.file_size && cached.modified_ms === d.modified_ms) {
          // File unchanged — use cached analysis
          cachedResults.push(cached.info);
        } else {
          // New or modified file — needs analysis
          toAnalyze.push(d);
        }
      }

      // Removed files are simply not in discoveredPaths — they won't appear in results

      const removedCount = [...cachedPaths].filter((p) => !discoveredPaths.has(p)).length;
      const unchangedCount = cachedResults.length;
      const newOrChangedCount = toAnalyze.length;

      if (newOrChangedCount === 0 && removedCount === 0) {
        // Nothing changed — silently update from cache without showing loading UI
        set({ userSamples: cachedResults });
        get().addLog('info', `${total} samples up to date (no changes detected)`);
        return;
      }

      // Only now show loading UI — there are actual files to analyze
      set({ userSamplesLoading: true, userSamplesScanProgress: { scanned: unchangedCount, total, phase: 'analyzing' } });

      // Build initial state: cached results + placeholders for items to analyze
      const placeholders: UserSampleInfo[] = toAnalyze.map((d) => ({
        name: d.name,
        path: d.path,
        file_type: d.file_type,
        folder: d.folder,
        duration_secs: 0,
        sample_rate: 0,
        bpm_estimate: null,
        audio_type: 'unknown',
        feeling: 'neutral',
        tags: [],
      }));
      const allSamples = [...cachedResults, ...placeholders];
      set({
        userSamples: allSamples,
        userSamplesScanProgress: { scanned: unchangedCount, total, phase: 'analyzing' },
      });
      get().addLog(
        'info',
        `Found ${total} files: ${unchangedCount} cached, ${newOrChangedCount} to analyze` +
          (removedCount > 0 ? `, ${removedCount} removed` : '')
      );

      // Phase 3: Analyze only new/changed files in batches
      const BATCH_SIZE = 5;
      let scanned = unchangedCount;
      for (let i = 0; i < toAnalyze.length; i += BATCH_SIZE) {
        const batch = toAnalyze.slice(i, i + BATCH_SIZE);
        const results = await Promise.allSettled(
          batch.map((d) => invoke<UserSampleInfo>('analyze_user_sample', { path: d.path }))
        );

        // Merge analyzed results into the samples array
        const currentSamples = [...get().userSamples];
        for (let j = 0; j < results.length; j++) {
          const result = results[j];
          if (result.status === 'fulfilled') {
            const analyzed = result.value;
            const idx = currentSamples.findIndex((s) => s.path === analyzed.path);
            if (idx !== -1) {
              currentSamples[idx] = analyzed;
            }
          }
        }
        scanned += batch.length;
        set({
          userSamples: currentSamples,
          userSamplesScanProgress: { scanned: Math.min(scanned, total), total, phase: 'analyzing' },
        });
      }

      // Save updated cache
      saveSampleCache(dir, get().userSamples, discovered);

      set({ userSamplesLoading: false, userSamplesScanProgress: { scanned: total, total, phase: 'done' } });
      // Clear progress after a short delay
      setTimeout(() => set({ userSamplesScanProgress: null }), 2000);
      get().addLog('info', `Scan complete: ${newOrChangedCount} analyzed, ${unchangedCount} from cache`);
    } catch (e: any) {
      set({ userSamplesLoading: false, userSamplesScanProgress: null });
      get().addLog('error', `Failed to scan user samples: ${e}`);
    }
  },

  fullRescanUserSamples: async () => {
    clearSampleCache();
    set({ userSamples: [] });
    get().addLog('info', 'Cache cleared — starting full rescan...');
    await get().scanUserSamples();
  },

  loadUserSamplesDir: async () => {
    const savedDir = localStorage.getItem('pibeat-user-samples-dir');
    if (savedDir) {
      set({ userSamplesDir: savedDir });
      try {
        await invoke('set_user_samples_dir', { dir: savedDir });
        // Don't auto-scan on startup - user can click the scan button
        // to avoid freezing if the directory is large
      } catch {
        // Directory might not exist anymore, just silently fail
      }
    }
  },

  addAgentMessage: (msg) => set((s) => ({
    agentMessages: [...s.agentMessages, msg],
  })),
  clearAgentMessages: () => set({ agentMessages: [] }),
  setAgentProvider: (provider) => set({ agentProvider: provider }),
  setAgentModel: (model) => set({ agentModel: model }),

  addLog: (level, message) => set((state) => ({
    logs: [...state.logs, {
      timestamp: Date.now(),
      level,
      message,
    }].slice(-500),
  })),

  setErrorLine: (line) => set({ errorLine: line }),

  fetchSampleDurations: async (names: string[]) => {
    if (names.length === 0) return;
    // Filter out names we already have cached
    const cached = get().sampleDurations;
    const needed = names.filter(n => cached[n] === undefined);
    if (needed.length === 0) return;

    set({ sampleDurationsLoading: true });
    try {
      const durations = await invoke<Record<string, number>>('get_sample_durations', { names: needed });
      set((state) => ({
        sampleDurations: { ...state.sampleDurations, ...durations },
        sampleDurationsLoading: false,
      }));
    } catch (e) {
      console.warn('[store] Failed to fetch sample durations:', e);
      set({ sampleDurationsLoading: false });
    }
  },
  
  addCue: (name, buffer) => {
    const state = get();
    const newCue: CueEvent = {
      id: state.cueEvents.length > 0 ? Math.max(...state.cueEvents.map(c => c.id)) + 1 : 1,
      name,
      timestamp: Date.now(),
      buffer,
    };
    set((s) => ({
      cueEvents: [...s.cueEvents, newCue].slice(-100), // Keep last 100 cues
    }));
    get().addLog('comment', `Cue: ${name}`);
  },
  
  clearCues: () => set({ cueEvents: [] }),

  // SuperCollider actions
  initSuperCollider: async () => {
    try {
      const status = await invoke<ScStatus>('init_supercollider');
      set({ scStatus: status });
      get().addLog('info', status.message);
    } catch (e: any) {
      get().addLog('error', `SuperCollider init failed: ${e}`);
      set({ scStatus: { available: false, booted: false, enabled: false, message: `Error: ${e}` } });
    }
  },

  toggleScEngine: async (enabled: boolean) => {
    try {
      const status = await invoke<ScStatus>('toggle_sc_engine', { enabled });
      set({ scStatus: status });
      get().addLog('info', status.message);
    } catch (e: any) {
      get().addLog('error', `Failed to toggle SC engine: ${e}`);
    }
  },

  fetchScStatus: async () => {
    try {
      const status = await invoke<ScStatus>('sc_status');
      set({ scStatus: status });
    } catch (e: any) {
      // Silently fail — SC may not be available
    }
  },
}));
