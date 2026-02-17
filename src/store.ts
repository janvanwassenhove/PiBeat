import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { LLMProvider, ModelId } from './llm';

export type AppTheme = 'pibeat' | 'sonicpi' | 'amber';

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
  
  // Agent
  agentMessages: AgentMessage[];
  agentProvider: LLMProvider;
  agentModel: ModelId;
  
  // Cues
  cueEvents: CueEvent[];
  
  // Actions
  setActiveBuffer: (id: number) => void;
  updateBufferCode: (id: number, code: string) => void;
  addBuffer: () => void;
  removeBuffer: (id: number) => void;
  
  runCode: () => Promise<void>;
  stopAudio: () => Promise<void>;
  
  setVolume: (vol: number) => Promise<void>;
  setBpm: (bpm: number) => Promise<void>;
  
  startRecording: () => Promise<void>;
  stopRecording: (path?: string) => Promise<void>;
  
  updateWaveform: () => Promise<void>;
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

  previewSynth: (synthName: string) => Promise<void>;

  // User Samples actions
  setUserSamplesDir: (dir: string) => Promise<void>;
  scanUserSamples: () => Promise<void>;
  loadUserSamplesDir: () => Promise<void>;

  addAgentMessage: (msg: AgentMessage) => void;
  clearAgentMessages: () => void;
  setAgentProvider: (provider: LLMProvider) => void;
  setAgentModel: (model: ModelId) => void;

  addLog: (level: string, message: string) => void;
  
  // SuperCollider actions
  initSuperCollider: () => Promise<void>;
  toggleScEngine: (enabled: boolean) => Promise<void>;
  fetchScStatus: () => Promise<void>;
  
  // Cue actions
  addCue: (name: string, buffer?: string) => void;
  clearCues: () => void;
}

const DEFAULT_CODE = `# Welcome to PiBeat! 🎵
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
  buffers: [
    { id: 0, name: 'Buffer 0', code: DEFAULT_CODE },
    { id: 1, name: 'Buffer 1', code: DEMO_BEAT },
    { id: 2, name: 'Buffer 2', code: DEMO_SYNTH },
    { id: 3, name: 'Buffer 3', code: '# Empty buffer\n' },
    { id: 4, name: 'Buffer 4', code: '# Empty buffer\n' },
    { id: 5, name: 'Buffer 5', code: '# Empty buffer\n' },
    { id: 6, name: 'Buffer 6', code: '# Empty buffer\n' },
    { id: 7, name: 'Buffer 7', code: '# Empty buffer\n' },
    { id: 8, name: 'Buffer 8', code: '# Empty buffer\n' },
    { id: 9, name: 'Buffer 9', code: '# Empty buffer\n' },
  ],
  activeBufferId: 0,
  isPlaying: false,
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
  showUserSamplePanel: false,
  userSamples: [],
  userSamplesDir: localStorage.getItem('pibeat-user-samples-dir'),
  userSamplesLoading: false,
  agentMessages: [],
  agentProvider: 'local',
  agentModel: 'local-rules',
  cueEvents: [],

  setActiveBuffer: (id) => set({ activeBufferId: id }),

  updateBufferCode: (id, code) => set((state) => ({
    buffers: state.buffers.map(b => b.id === id ? { ...b, code } : b),
  })),

  addBuffer: () => set((state) => {
    const maxId = Math.max(...state.buffers.map(b => b.id));
    return {
      buffers: [...state.buffers, {
        id: maxId + 1,
        name: `Buffer ${maxId + 1}`,
        code: '# New buffer\n',
      }],
    };
  }),

  removeBuffer: (id) => set((state) => ({
    buffers: state.buffers.filter(b => b.id !== id),
    activeBufferId: state.activeBufferId === id ? state.buffers[0]?.id ?? 0 : state.activeBufferId,
  })),

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
      set({ isPlaying: true, bpm: result.effective_bpm || get().bpm, setupTimeMs: result.setup_time_ms || 0 });
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
      set({ isPlaying: false });
    }
  },

  stopAudio: async () => {
    try {
      await invoke('stop_audio');
      set({ isPlaying: false });
      get().addLog('info', 'Stopped');
    } catch (e: any) {
      get().addLog('error', `Error stopping: ${e}`);
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
      get().addLog('info', '🔴 Recording started');
    } catch (e: any) {
      get().addLog('error', `Recording error: ${e}`);
    }
  },

  stopRecording: async (path?) => {
    try {
      const result = await invoke<string>('stop_recording', { path: path ?? null });
      set({ isRecording: false });
      get().addLog('info', `Recording saved: ${result}`);
    } catch (e: any) {
      get().addLog('error', `Save error: ${e}`);
    }
  },

  updateWaveform: async () => {
    try {
      const waveform = await invoke<number[]>('get_waveform');
      set({ waveform });
    } catch (e) {
      // Ignore waveform errors
    }
  },

  fetchStatus: async () => {
    try {
      const status = await invoke<EngineStatus>('get_status');
      set({
        isPlaying: status.is_playing,
        masterVolume: status.master_volume,
        bpm: status.bpm,
        isRecording: status.is_recording,
      });
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
  },
  toggleSampleBrowser: () => set((s) => ({ showSampleBrowser: !s.showSampleBrowser })),
  toggleSynthBrowser: () => set((s) => ({ showSynthBrowser: !s.showSynthBrowser })),
  toggleEffectsPanel: () => set((s) => ({ showEffectsPanel: !s.showEffectsPanel })),
  toggleHelp: () => set((s) => ({ showHelp: !s.showHelp })),
  toggleAgentChat: () => set((s) => ({ showAgentChat: !s.showAgentChat })),
  toggleCuePanel: () => set((s) => ({ showCuePanel: !s.showCuePanel })),
  toggleUserSamplePanel: () => set((s) => ({ showUserSamplePanel: !s.showUserSamplePanel })),

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
    set({ userSamplesLoading: true });
    try {
      // Ensure backend knows the directory
      await invoke('set_user_samples_dir', { dir });
      const samples = await invoke<UserSampleInfo[]>('scan_user_samples');
      set({ userSamples: samples, userSamplesLoading: false });
      get().addLog('info', `Scanned ${samples.length} user samples`);
    } catch (e: any) {
      set({ userSamplesLoading: false });
      get().addLog('error', `Failed to scan user samples: ${e}`);
    }
  },

  loadUserSamplesDir: async () => {
    const savedDir = localStorage.getItem('pibeat-user-samples-dir');
    if (savedDir) {
      set({ userSamplesDir: savedDir });
      try {
        await invoke('set_user_samples_dir', { dir: savedDir });
        // Auto-scan saved directory
        await get().scanUserSamples();
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
    get().addLog('comment', `🎯 Cue: ${name}`);
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
