import React, { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

// ─── Types ──────────────────────────────────────────────────────────────────

type DanceStyle = 'bounce' | 'headbang' | 'sway' | 'robot' | 'funk' | 'rave';
type VisualEffect = 'scanlines' | 'pixel_rain' | 'star_field' | 'fire_trails' | 'mirror_ball' | 'neon_glow';
type StageDecor = 'retro_stage' | 'oscilloscope' | 'space_scene' | 'city_night' | 'matrix' | 'underwater';
type CameraMode = 'full_stage' | 'stage_view' | 'close_up' | 'zoom_character' | 'auto';

interface VisualEngineConfig {
  target_fps: number;
  energy_decay: number;
  idle_timeout: number;
  crowd_enabled: boolean;
  lighting_enabled: boolean;
  dance_style: DanceStyle;
  visual_effects: VisualEffect[];
  decor: StageDecor;
  camera_mode: CameraMode;
  camera_focus: string | null;
  visible_members: Record<string, boolean>;
}

interface PerformanceSnapshot {
  band: { role: string; energy: number; animation_state: { state: string } }[];
  lighting: { brightness: number; strobe_active: boolean };
  crowd: { excitement: number; jumping_count: number; wave_active: boolean };
  energy: number;
  bpm: number;
  beat_position: number;
  is_playing: boolean;
  frame: number;
  dance_style: DanceStyle;
  active_effects: VisualEffect[];
  decor: StageDecor;
  camera_mode: CameraMode;
  camera_focus: string | null;
}

const DANCE_STYLES: { value: DanceStyle; label: string; icon: React.ReactNode }[] = [
  { value: 'bounce', label: 'Bounce', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><path d="M8 12V4M5 7l3-3 3 3"/><circle cx="8" cy="14" r="1" fill="currentColor" stroke="none"/></svg> },
  { value: 'headbang', label: 'Headbang', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><circle cx="8" cy="5" r="3"/><path d="M5 8v4M11 8v4M4 14h3M9 14h3"/></svg> },
  { value: 'sway', label: 'Sway', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><path d="M2 8c2-3 4 3 6 0s4 3 6 0"/></svg> },
  { value: 'robot', label: 'Robot', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"><rect x="4" y="5" width="8" height="7" rx="1"/><line x1="8" y1="5" x2="8" y2="2"/><circle cx="8" cy="1.5" r="1" fill="currentColor" stroke="none"/><circle cx="6.5" cy="8" r="0.8" fill="currentColor" stroke="none"/><circle cx="9.5" cy="8" r="0.8" fill="currentColor" stroke="none"/><line x1="6" y1="12" x2="6" y2="14"/><line x1="10" y1="12" x2="10" y2="14"/></svg> },
  { value: 'funk', label: 'Funk', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><path d="M8 1l1.5 4.5H14l-3.5 2.8 1.3 4.2L8 10l-3.8 2.5 1.3-4.2L2 5.5h4.5z"/></svg> },
  { value: 'rave', label: 'Rave', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"><path d="M9 1L6 7h4l-3 8 7-9H9.5z"/></svg> },
];

const VISUAL_EFFECTS: { value: VisualEffect; label: string; icon: React.ReactNode }[] = [
  { value: 'scanlines', label: 'Scanlines', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2"><line x1="2" y1="3" x2="14" y2="3"/><line x1="2" y1="6" x2="14" y2="6"/><line x1="2" y1="9" x2="14" y2="9"/><line x1="2" y1="12" x2="14" y2="12"/></svg> },
  { value: 'pixel_rain', label: 'Pixel Rain', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><line x1="4" y1="2" x2="4" y2="5"/><line x1="8" y1="4" x2="8" y2="7"/><line x1="12" y1="1" x2="12" y2="4"/><line x1="6" y1="8" x2="6" y2="11"/><line x1="10" y1="9" x2="10" y2="12"/><line x1="3" y1="11" x2="3" y2="14"/></svg> },
  { value: 'star_field', label: 'Star Field', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" stroke="none"><circle cx="3" cy="4" r="1"/><circle cx="8" cy="2" r="1.2"/><circle cx="13" cy="5" r="0.8"/><circle cx="5" cy="10" r="1"/><circle cx="11" cy="11" r="0.7"/><circle cx="8" cy="7" r="0.6"/></svg> },
  { value: 'fire_trails', label: 'Fire Trails', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><path d="M8 14c-3 0-4-2.5-4-5 0-3 3-5 3-7 1 2 3 1 3 4 .5-.8 1.5-1 2 0 .5 2-1 3-1 3s1 1 1 2.5c0 1.5-1.5 2.5-4 2.5z"/></svg> },
  { value: 'mirror_ball', label: 'Mirror Ball', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2"><circle cx="8" cy="8" r="5"/><ellipse cx="8" cy="8" rx="5" ry="2"/><ellipse cx="8" cy="8" rx="2" ry="5"/><line x1="8" y1="3" x2="8" y2="1"/></svg> },
  { value: 'neon_glow', label: 'Neon Glow', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><circle cx="8" cy="8" r="3"/><line x1="8" y1="1" x2="8" y2="3"/><line x1="8" y1="13" x2="8" y2="15"/><line x1="1" y1="8" x2="3" y2="8"/><line x1="13" y1="8" x2="15" y2="8"/><line x1="3.5" y1="3.5" x2="5" y2="5"/><line x1="11" y1="11" x2="12.5" y2="12.5"/></svg> },
];

const STAGE_DECORS: { value: StageDecor; label: string; icon: React.ReactNode }[] = [
  { value: 'retro_stage', label: 'Retro Stage', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"><path d="M2 12h12M4 12V6l4-3 4 3v6"/><line x1="8" y1="3" x2="8" y2="1"/></svg> },
  { value: 'oscilloscope', label: 'Oscilloscope', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><rect x="1" y="3" width="14" height="10" rx="1"/><path d="M3 8h2l1.5-3 2 6 1.5-3H13"/></svg> },
  { value: 'space_scene', label: 'Space Scene', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><circle cx="6" cy="8" r="4"/><path d="M10 4a5 5 0 010 8"/><circle cx="13" cy="3" r="0.8" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="0.6" fill="currentColor" stroke="none"/></svg> },
  { value: 'city_night', label: 'City Night', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round"><path d="M1 14V8h3v6M5 14V5h3v9M9 14V7h3v7M13 14V9h2v5"/><circle cx="10" cy="2" r="1" fill="currentColor" stroke="none"/></svg> },
  { value: 'matrix', label: 'Matrix', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"><text x="1" y="6" fontSize="6" fill="currentColor" fontFamily="monospace" stroke="none">01</text><text x="8" y="6" fontSize="6" fill="currentColor" fontFamily="monospace" stroke="none">10</text><text x="4" y="13" fontSize="6" fill="currentColor" fontFamily="monospace" stroke="none">11</text></svg> },
  { value: 'underwater', label: 'Underwater', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><path d="M1 5c2-1.5 3 1.5 5 0s3 1.5 5 0s3 1.5 5 0"/><path d="M1 9c2-1.5 3 1.5 5 0s3 1.5 5 0s3 1.5 5 0"/><path d="M1 13c2-1.5 3 1.5 5 0s3 1.5 5 0s3 1.5 5 0"/></svg> },
];

const CAMERA_MODES: { value: CameraMode; label: string; desc: string; icon: React.ReactNode }[] = [
  { value: 'full_stage', label: 'Full Stage', desc: 'Full band view with crowd', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"><rect x="1" y="3" width="14" height="10" rx="1"/><line x1="5" y1="13" x2="5" y2="15"/><line x1="11" y1="13" x2="11" y2="15"/></svg> },
  { value: 'stage_view', label: 'Stage View', desc: 'Tighter framing, no crowd', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="4" width="10" height="8" rx="1"/></svg> },
  { value: 'close_up', label: 'Close Up', desc: 'Follow most active member', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><circle cx="8" cy="8" r="4"/><line x1="8" y1="1" x2="8" y2="4"/><line x1="8" y1="12" x2="8" y2="15"/><line x1="1" y1="8" x2="4" y2="8"/><line x1="12" y1="8" x2="15" y2="8"/></svg> },
  { value: 'zoom_character', label: 'Zoom Character', desc: 'Lock onto a specific member', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><circle cx="7" cy="7" r="4"/><line x1="10" y1="10" x2="14" y2="14"/></svg> },
  { value: 'auto', label: 'Auto', desc: 'Cycle views based on energy', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"><path d="M12 3l2 2-2 2"/><path d="M4 5h10"/><path d="M4 13l-2-2 2-2"/><path d="M12 11H2"/></svg> },
];

const FOCUS_TARGETS: { value: string; label: string; icon: React.ReactNode }[] = [
  { value: 'drummer', label: 'Drums', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><ellipse cx="8" cy="10" rx="5" ry="2.5"/><line x1="3" y1="10" x2="3" y2="6"/><line x1="13" y1="10" x2="13" y2="6"/><ellipse cx="8" cy="6" rx="5" ry="2.5"/><line x1="6" y1="1" x2="9" y2="4"/><line x1="10" y1="1" x2="7" y2="4"/></svg> },
  { value: 'percussionist', label: 'Percussion', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><path d="M4 3l4 10M12 3L8 13"/><circle cx="4" cy="2" r="1.5"/><circle cx="12" cy="2" r="1.5"/></svg> },
  { value: 'bassist', label: 'Bass', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"><path d="M5 2v10c0 1.5 1.5 2.5 3 2.5s3-1 3-2.5V6"/><circle cx="5" cy="2" r="1"/></svg> },
  { value: 'guitarist', label: 'Guitar', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"><path d="M11 1l-2 2 1 1-3 3c-2-.5-4 1-4 3s1.5 3.5 3.5 3.5 3-2 3-4l3-3 1 1 2-2z"/></svg> },
  { value: 'keyboard', label: 'Keys', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round"><rect x="1" y="4" width="14" height="8" rx="1"/><line x1="4" y1="4" x2="4" y2="12"/><line x1="7" y1="4" x2="7" y2="12"/><line x1="10" y1="4" x2="10" y2="12"/><line x1="13" y1="4" x2="13" y2="12"/><rect x="3" y="4" width="2" height="5" fill="currentColor"/><rect x="8" y="4" width="2" height="5" fill="currentColor"/><rect x="11" y="4" width="2" height="5" fill="currentColor"/></svg> },
  { value: 'vocalist', label: 'Vocals', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"><rect x="6" y="1" width="4" height="7" rx="2"/><path d="M4 7v1a4 4 0 008 0V7"/><line x1="8" y1="12" x2="8" y2="15"/><line x1="5" y1="15" x2="11" y2="15"/></svg> },
  { value: 'dj', label: 'DJ', icon: <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"><circle cx="8" cy="9" r="5"/><circle cx="8" cy="9" r="1.5"/><path d="M4 2C5 3 5.5 4 4.5 5"/><path d="M12 2c-1 1-1.5 2-.5 3"/></svg> },
];

// ─── Component ──────────────────────────────────────────────────────────────

const BandControlPanel: React.FC<{ visible: boolean }> = ({ visible }) => {
  const [config, setConfig] = useState<VisualEngineConfig>({
    target_fps: 30,
    energy_decay: 0.05,
    idle_timeout: 0.5,
    crowd_enabled: true,
    lighting_enabled: true,
    dance_style: 'bounce',
    visual_effects: [],
    decor: 'retro_stage',
    camera_mode: 'full_stage',
    camera_focus: null,
    visible_members: {},
  });
  const [enabled, setEnabled] = useState(true);
  const [windowOpen, setWindowOpen] = useState(false);
  const [snapshot, setSnapshot] = useState<PerformanceSnapshot | null>(null);
  const bandWindowRef = useRef<WebviewWindow | null>(null);
  const pollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Load initial config
  useEffect(() => {
    if (!visible) return;
    invoke<VisualEngineConfig>('get_visual_config').then(setConfig).catch(() => {});
    invoke<boolean>('get_visual_enabled').then(setEnabled).catch(() => {});
  }, [visible]);

  // Poll for live stats
  useEffect(() => {
    if (!visible) return;
    let running = true;
    const poll = async () => {
      if (!running) return;
      try {
        const snap = await invoke<PerformanceSnapshot>('get_visual_snapshot');
        setSnapshot(snap);
      } catch { /* ignore */ }
      if (running) {
        pollTimerRef.current = setTimeout(poll, 250);
      }
    };
    poll();
    return () => {
      running = false;
      if (pollTimerRef.current) clearTimeout(pollTimerRef.current);
    };
  }, [visible]);

  // Push config changes to Rust
  const pushConfig = useCallback(async (newConfig: VisualEngineConfig) => {
    setConfig(newConfig);
    try {
      const confirmed = await invoke<VisualEngineConfig>('set_visual_config', { config: newConfig });
      setConfig(confirmed);
    } catch { /* ignore */ }
  }, []);

  // Toggle engine enabled
  const toggleEnabled = useCallback(async () => {
    try {
      const result = await invoke<boolean>('set_visual_enabled', { enabled: !enabled });
      setEnabled(result);
    } catch { /* ignore */ }
  }, [enabled]);

  // Open / close detached band window
  const toggleBandWindow = useCallback(async () => {
    if (windowOpen && bandWindowRef.current) {
      try {
        await bandWindowRef.current.close();
      } catch { /* already closed */ }
      bandWindowRef.current = null;
      setWindowOpen(false);
      return;
    }

    try {
      // Check if window already exists
      const existing = await WebviewWindow.getByLabel('band-visualizer');
      if (existing) {
        await existing.setFocus();
        bandWindowRef.current = existing;
        setWindowOpen(true);
        return;
      }
    } catch { /* doesn't exist */ }

    try {
      const win = new WebviewWindow('band-visualizer', {
        url: '/band.html',
        title: 'PiBeat — Band Visualizer',
        width: 800,
        height: 500,
        minWidth: 400,
        minHeight: 250,
        resizable: true,
        decorations: false,
        center: true,
        transparent: false,
      });

      // Listen for close
      win.once('tauri://destroyed', () => {
        bandWindowRef.current = null;
        setWindowOpen(false);
      });

      bandWindowRef.current = win;
      setWindowOpen(true);
    } catch (e) {
      console.error('[BandControlPanel] Failed to create band window:', e);
    }
  }, [windowOpen]);

  // Close band window when control panel is hidden
  useEffect(() => {
    if (!visible && bandWindowRef.current) {
      bandWindowRef.current.close().catch(() => {});
      bandWindowRef.current = null;
      setWindowOpen(false);
    }
  }, [visible]);

  if (!visible) return null;

  const memberColors: Record<string, string> = {
    drummer: '#ff6b6b',
    percussionist: '#ff9f43',
    bassist: '#4ecdc4',
    guitarist: '#ffe66d',
    keyboard: '#a78bfa',
    vocalist: '#f472b6',
    dj: '#00d4ff',
  };

  const memberLabels: Record<string, string> = {
    drummer: 'Drums',
    percussionist: 'Perc',
    bassist: 'Bass',
    guitarist: 'Guitar',
    keyboard: 'Keys',
    vocalist: 'Vox',
    dj: 'DJ',
  };

  const toggleEffect = (effect: VisualEffect) => {
    const effects = config.visual_effects.includes(effect)
      ? config.visual_effects.filter(e => e !== effect)
      : [...config.visual_effects, effect];
    pushConfig({ ...config, visual_effects: effects });
  };

  const toggleMemberVisibility = (role: string) => {
    const vis = { ...config.visible_members };
    vis[role] = vis[role] === false ? true : false;
    pushConfig({ ...config, visible_members: vis });
  };

  const isMemberVisible = (role: string): boolean => {
    return config.visible_members[role] !== false;
  };

  return (
    <div className="band-control-panel">
      <div className="band-control-header">
        <span className="band-control-title">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" style={{ verticalAlign: 'middle', marginRight: 4 }}>
            <circle cx="5" cy="5" r="2" /><line x1="5" y1="7" x2="5" y2="12" />
            <circle cx="11" cy="5" r="2" /><line x1="11" y1="7" x2="11" y2="12" />
            <line x1="3" y1="10" x2="7" y2="10" /><line x1="9" y1="10" x2="13" y2="10" />
          </svg>
          Band Visualizer
        </span>
      </div>

      {/* Window launch button */}
      <div className="band-control-section">
        <button
          className={`band-window-btn ${windowOpen ? 'open' : ''}`}
          onClick={toggleBandWindow}
        >
          {windowOpen ? 'Close Window' : 'Open Band Window'}
        </button>
      </div>

      {/* Engine toggle */}
      <div className="band-control-section">
        <div className="band-control-row">
          <span className="band-control-label">Engine</span>
          <button
            className={`band-toggle-btn ${enabled ? 'on' : 'off'}`}
            onClick={toggleEnabled}
          >
            {enabled ? 'ON' : 'OFF'}
          </button>
        </div>
      </div>

      {/* FPS */}
      <div className="band-control-section">
        <div className="band-control-row">
          <span className="band-control-label">FPS</span>
          <input
            type="range"
            min="10"
            max="60"
            step="5"
            value={config.target_fps}
            onChange={(e) => pushConfig({ ...config, target_fps: parseInt(e.target.value) })}
            className="band-slider"
          />
          <span className="band-control-value">{config.target_fps}</span>
        </div>
      </div>

      {/* Energy Decay */}
      <div className="band-control-section">
        <div className="band-control-row">
          <span className="band-control-label">Decay</span>
          <input
            type="range"
            min="0.01"
            max="0.2"
            step="0.01"
            value={config.energy_decay}
            onChange={(e) => pushConfig({ ...config, energy_decay: parseFloat(e.target.value) })}
            className="band-slider"
          />
          <span className="band-control-value">{config.energy_decay.toFixed(2)}</span>
        </div>
      </div>

      {/* Idle Timeout */}
      <div className="band-control-section">
        <div className="band-control-row">
          <span className="band-control-label">Idle</span>
          <input
            type="range"
            min="0.1"
            max="2.0"
            step="0.1"
            value={config.idle_timeout}
            onChange={(e) => pushConfig({ ...config, idle_timeout: parseFloat(e.target.value) })}
            className="band-slider"
          />
          <span className="band-control-value">{config.idle_timeout.toFixed(1)}s</span>
        </div>
      </div>

      {/* Feature toggles */}
      <div className="band-control-section">
        <div className="band-control-row">
          <span className="band-control-label">Crowd</span>
          <button
            className={`band-toggle-btn ${config.crowd_enabled ? 'on' : 'off'}`}
            onClick={() => pushConfig({ ...config, crowd_enabled: !config.crowd_enabled })}
          >
            {config.crowd_enabled ? 'ON' : 'OFF'}
          </button>
        </div>
        <div className="band-control-row">
          <span className="band-control-label">Lighting</span>
          <button
            className={`band-toggle-btn ${config.lighting_enabled ? 'on' : 'off'}`}
            onClick={() => pushConfig({ ...config, lighting_enabled: !config.lighting_enabled })}
          >
            {config.lighting_enabled ? 'ON' : 'OFF'}
          </button>
        </div>
      </div>

      {/* Decor / Dance / Effects / Members / Camera — icon rows */}
      <div className="band-control-section">
        <div className="band-icon-row">
          <span className="band-icon-row-label">Decor</span>
          <div className="band-icon-group">
            {STAGE_DECORS.map(d => (
              <button
                key={d.value}
                className={`band-icon-btn ${config.decor === d.value ? 'active' : ''}`}
                onClick={() => pushConfig({ ...config, decor: d.value })}
                title={d.label}
              >{d.icon}</button>
            ))}
          </div>
        </div>

        <div className="band-icon-row">
          <span className="band-icon-row-label">Dance</span>
          <div className="band-icon-group">
            {DANCE_STYLES.map(s => (
              <button
                key={s.value}
                className={`band-icon-btn ${config.dance_style === s.value ? 'active' : ''}`}
                onClick={() => pushConfig({ ...config, dance_style: s.value })}
                title={s.label}
              >{s.icon}</button>
            ))}
          </div>
        </div>

        <div className="band-icon-row">
          <span className="band-icon-row-label">FX</span>
          <div className="band-icon-group">
            {VISUAL_EFFECTS.map(fx => (
              <button
                key={fx.value}
                className={`band-icon-btn ${config.visual_effects.includes(fx.value) ? 'active' : ''}`}
                onClick={() => toggleEffect(fx.value)}
                title={fx.label}
              >{fx.icon}</button>
            ))}
          </div>
        </div>

        <div className="band-icon-row">
          <span className="band-icon-row-label">Band</span>
          <div className="band-icon-group">
            {FOCUS_TARGETS.map(m => {
              const on = isMemberVisible(m.value);
              return (
                <button
                  key={m.value}
                  className={`band-icon-btn ${on ? 'active' : ''}`}
                  onClick={() => toggleMemberVisibility(m.value)}
                  title={`${on ? 'Hide' : 'Show'} ${m.label}`}
                >{m.icon}</button>
              );
            })}
          </div>
        </div>

        <div className="band-icon-row">
          <span className="band-icon-row-label">Cam</span>
          <div className="band-icon-group">
            {CAMERA_MODES.map(c => (
              <button
                key={c.value}
                className={`band-icon-btn ${config.camera_mode === c.value ? 'active' : ''}`}
                onClick={() => pushConfig({ ...config, camera_mode: c.value })}
                title={c.desc}
              >{c.icon}</button>
            ))}
          </div>
        </div>
        {config.camera_mode === 'zoom_character' && (
          <div className="band-control-row" style={{ marginTop: 4 }}>
            <span className="band-control-label">Focus</span>
            <select
              className="band-select"
              value={config.camera_focus ?? 'drummer'}
              onChange={(e) => pushConfig({ ...config, camera_focus: e.target.value })}
            >
              {FOCUS_TARGETS.map(f => (
                <option key={f.value} value={f.value}>{f.label}</option>
              ))}
            </select>
          </div>
        )}
      </div>

      {/* Live stats */}
      {snapshot && (
        <div className="band-control-section band-stats">
          <div className="band-control-row">
            <span className="band-control-label">Status</span>
            <span className={`band-status-dot ${snapshot.is_playing ? 'playing' : 'idle'}`} />
            <span className="band-control-value">
              {snapshot.is_playing ? 'Playing' : 'Idle'}
            </span>
          </div>

          <div className="band-control-row">
            <span className="band-control-label">Energy</span>
            <div className="band-energy-bar">
              <div
                className="band-energy-fill"
                style={{ width: `${Math.round(snapshot.energy * 100)}%` }}
              />
            </div>
            <span className="band-control-value">{Math.round(snapshot.energy * 100)}%</span>
          </div>

          <div className="band-control-row">
            <span className="band-control-label">BPM</span>
            <span className="band-control-value">{Math.round(snapshot.bpm)}</span>
          </div>

          <div className="band-control-row">
            <span className="band-control-label">Frame</span>
            <span className="band-control-value">#{snapshot.frame}</span>
          </div>

          {/* Band member status */}
          <div className="band-members-status">
            {snapshot.band.map((m) => (
              <div key={m.role} className="band-member-row">
                <span
                  className="band-member-dot"
                  style={{ background: memberColors[m.role] || '#666' }}
                />
                <span className="band-member-name">{memberLabels[m.role] || m.role}</span>
                <div className="band-member-energy-bar">
                  <div
                    className="band-member-energy-fill"
                    style={{
                      width: `${Math.round(m.energy * 100)}%`,
                      background: memberColors[m.role] || '#666',
                    }}
                  />
                </div>
                <span className="band-member-state">{m.animation_state.state}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};

export default BandControlPanel;
