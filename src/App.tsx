import React, { useEffect, useState, useRef, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import Toolbar from "./components/Toolbar";
import BufferTabs from "./components/BufferTabs";
import CodeEditor from "./components/CodeEditorLazy";
import TimelineView from "./components/TimelineView";
import WaveformVisualizer from "./components/WaveformVisualizer";
import LogPanel from "./components/LogPanel";
import SampleBrowser from "./components/SampleBrowser";
import SynthBrowser from "./components/SynthBrowser";
import EffectsPanel from "./components/EffectsPanel";
import HelpPanel from "./components/HelpPanel";
import AgentChat from "./components/AgentChat";
import CuePanel from "./components/CuePanel";
import UserSamplePanel from "./components/UserSamplePanel";
import BandControlPanel from "./components/BandControlPanel";
import { useStore, AppTheme } from "./store";
import { useShallow } from "zustand/react/shallow";
import "./App.css";

const THEMES: { id: AppTheme; label: string; colors: [string, string, string] }[] = [
  { id: 'pibeat',  label: 'PiBeat',    colors: ['#0d0d1a', '#00ff88', '#4488ff'] },
  { id: 'sonicpi', label: 'Sonic Pi',  colors: ['#0a0a0a', '#ff59b2', '#ffdd00'] },
  { id: 'amber',   label: 'Amber',     colors: ['#0f0d08', '#ffaa00', '#ff6600'] },
];

const ThemeSwitcher: React.FC<{ theme: AppTheme; setTheme: (t: AppTheme) => void }> = ({ theme, setTheme }) => {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, []);

  const current = THEMES.find(t => t.id === theme) || THEMES[0];

  return (
    <div className="theme-switcher" ref={ref}>
      <button
        className="theme-trigger"
        onClick={() => setOpen(!open)}
        title={`Theme: ${current.label}`}
      >
        <span className="theme-swatch-row">
          {current.colors.map((c, i) => (
            <span key={i} className="theme-dot" style={{ background: c }} />
          ))}
        </span>
        <svg className="theme-chevron" width="8" height="5" viewBox="0 0 8 5">
          <path d="M0 0 L4 4 L8 0" fill="none" stroke="currentColor" strokeWidth="1.5" />
        </svg>
      </button>
      {open && (
        <div className="theme-dropdown">
          {THEMES.map(t => (
            <button
              key={t.id}
              className={`theme-dropdown-item ${theme === t.id ? 'active' : ''}`}
              onClick={() => { setTheme(t.id); setOpen(false); }}
            >
              <span className="theme-swatch-row">
                {t.colors.map((c, i) => (
                  <span key={i} className="theme-dot" style={{ background: c }} />
                ))}
              </span>
              <span className="theme-label">{t.label}</span>
              {theme === t.id && <span className="theme-check">✓</span>}
            </button>
          ))}
        </div>
      )}
    </div>
  );
};

const AboutModal: React.FC<{ open: boolean; onClose: () => void }> = ({ open, onClose }) => {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [open, onClose]);

  const handleLink = useCallback((url: string) => (e: React.MouseEvent) => {
    e.preventDefault();
    openUrl(url);
  }, []);

  if (!open) return null;

  return (
    <div className="about-overlay" onClick={onClose}>
      <div className="about-modal" ref={ref} onClick={(e) => e.stopPropagation()}>
        <div className="about-logo">
          <span className="about-logo-icon">&#9835;</span>
          <span className="about-logo-text">PiBeat</span>
        </div>
        <div className="about-version">Version {__APP_VERSION__}</div>
        <p className="about-description">
          A desktop music live-coding application inspired by Sonic Pi.
          Write code, make music, in real time.
        </p>
        <div className="about-links">
          <a href="https://mityjohn.com/" onClick={handleLink('https://mityjohn.com/')}>mityjohn.com</a>
          <span className="about-separator">·</span>
          <a href="https://github.com/janvanwassenhove/PiBeat" onClick={handleLink('https://github.com/janvanwassenhove/PiBeat')}>GitHub</a>
        </div>
        <div className="about-copyright">© 2025–2026 Jan Van Wassenhove</div>
        <button className="about-close-btn" onClick={onClose}>Close</button>
      </div>
    </div>
  );
};

const App: React.FC = () => {
  const {
    fetchSamples,
    fetchStatus,
    loadUserSamplesDir,
    showSampleBrowser,
    showSynthBrowser,
    showEffectsPanel,
    showHelp,
    showAgentChat,
    showCuePanel,
    showUserSamplePanel,
    showBandVisualizer,
    detachedPanels,
    viewMode,
    theme,
    setTheme,
  } = useStore(
    useShallow((s) => ({
      fetchSamples: s.fetchSamples,
      fetchStatus: s.fetchStatus,
      loadUserSamplesDir: s.loadUserSamplesDir,
      showSampleBrowser: s.showSampleBrowser,
      showSynthBrowser: s.showSynthBrowser,
      showEffectsPanel: s.showEffectsPanel,
      showHelp: s.showHelp,
      showAgentChat: s.showAgentChat,
      showCuePanel: s.showCuePanel,
      showUserSamplePanel: s.showUserSamplePanel,
      showBandVisualizer: s.showBandVisualizer,
      detachedPanels: s.detachedPanels,
      viewMode: s.viewMode,
      theme: s.theme,
      setTheme: s.setTheme,
    })),
  );
  const [showAbout, setShowAbout] = useState(false);

  useEffect(() => {
    fetchSamples();
    loadUserSamplesDir();
    const interval = setInterval(() => {
      fetchStatus();
    }, 1000);
    return () => clearInterval(interval);
  }, [fetchSamples, fetchStatus, loadUserSamplesDir]);

  // Global keyboard shortcuts (capture phase so they fire before Monaco Editor)
  // Uses useStore.getState() to avoid stale closures and e.code for reliable key detection on Windows
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Ctrl+S (no shift): Save file
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.code === 'KeyS') {
        e.preventDefault();
        useStore.getState().saveBufferToFile();
        return;
      }
      // Ctrl+O: Open file
      if ((e.ctrlKey || e.metaKey) && e.code === 'KeyO') {
        e.preventDefault();
        useStore.getState().loadBufferFromFile();
        return;
      }
      // Ctrl+Shift+R or Alt+Shift+R: Toggle recording
      if (e.shiftKey && e.code === 'KeyR' && (e.ctrlKey || e.metaKey || e.altKey)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        const state = useStore.getState();
        if (state.isRecording) {
          state.stopRecording();
        } else {
          state.startRecording();
        }
        return;
      }
      // Ctrl+Enter or Alt+R: Run code
      if (((e.ctrlKey || e.metaKey) && e.code === 'Enter') || (e.altKey && !e.shiftKey && e.code === 'KeyR')) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        useStore.getState().runCode();
        return;
      }
      // Ctrl+. or Alt+S: Stop audio
      if (((e.ctrlKey || e.metaKey) && e.code === 'Period') || (e.altKey && e.code === 'KeyS')) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        useStore.getState().stopAudio();
        return;
      }
    };
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, []);

  // Native OS-level global shortcuts (Alt+R, Alt+S, Alt+Shift+R)
  // Registered from Rust at plugin init to avoid React StrictMode double-mount race conditions
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<string>('global-shortcut', (event) => {
      const shortcut = event.payload.toLowerCase();
      if (shortcut.includes('shift') && shortcut.includes('r')) {
        // Alt+Shift+R: Toggle recording
        const state = useStore.getState();
        if (state.isRecording) {
          state.stopRecording();
        } else {
          state.startRecording();
        }
      } else if (shortcut.includes('r')) {
        // Alt+R: Run code
        useStore.getState().runCode();
      } else if (shortcut.includes('s')) {
        // Alt+S: Stop audio
        useStore.getState().stopAudio();
      }
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  // Apply theme data attribute to root element
  useEffect(() => {
    if (theme === 'pibeat') {
      document.documentElement.removeAttribute('data-theme');
    } else {
      document.documentElement.setAttribute('data-theme', theme);
    }
  }, [theme]);

  // Listen for cross-window events from detached panel windows
  useEffect(() => {
    let unlistenInsert: (() => void) | undefined;
    let unlistenAttach: (() => void) | undefined;

    // Panel window wants to insert code into the active buffer
    listen<{ id: number; code: string }>('panel-insert-code', (event) => {
      const { id, code } = event.payload;
      useStore.getState().updateBufferCode(id, code);
    }).then((fn) => { unlistenInsert = fn; });

    // Panel window wants to re-attach to the side panel
    listen<{ panelId: string }>('panel-attach', (event) => {
      const { panelId: pid } = event.payload;
      const state = useStore.getState();
      if (state.detachedPanels[pid]) {
        state.toggleDetachPanel(pid);
      }
    }).then((fn) => { unlistenAttach = fn; });

    return () => {
      unlistenInsert?.();
      unlistenAttach?.();
    };
  }, []);

  const dp = detachedPanels || {};
  const hasAttachedPanel =
    (showSampleBrowser && !dp.sampleBrowser) ||
    (showSynthBrowser && !dp.synthBrowser) ||
    (showEffectsPanel && !dp.effectsPanel) ||
    (showHelp && !dp.helpPanel) ||
    (showAgentChat && !dp.agentChat) ||
    (showCuePanel && !dp.cuePanel) ||
    (showUserSamplePanel && !dp.userSamplePanel) ||
    (showBandVisualizer && !dp.bandVisualizer);

  const appWindow = getCurrentWindow();

  const handleMinimize = () => appWindow.minimize();
  const handleMaximize = () => appWindow.toggleMaximize();
  const handleClose = () => appWindow.close();

  return (
    <div className="app">
      <div className="app-header">
        <div className="titlebar-left" data-tauri-drag-region>
          <div className="app-logo" onClick={() => setShowAbout(true)} title="About PiBeat">
            <span className="logo-icon">&#9835;</span>
            <span className="logo-text">PiBeat</span>
          </div>
        </div>
        <ThemeSwitcher theme={theme} setTheme={setTheme} />
        <Toolbar />
        <div className="titlebar-spacer" data-tauri-drag-region></div>
        <div className="titlebar-controls">
          <button className="titlebar-button" onClick={handleMinimize} title="Minimize">
            <svg width="10" height="1" viewBox="0 0 10 1">
              <rect width="10" height="1" fill="currentColor" />
            </svg>
          </button>
          <button className="titlebar-button" onClick={handleMaximize} title="Maximize">
            <svg width="10" height="10" viewBox="0 0 10 10">
              <rect x="0" y="0" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="1" />
            </svg>
          </button>
          <button className="titlebar-button titlebar-close" onClick={handleClose} title="Close">
            <svg width="10" height="10" viewBox="0 0 10 10">
              <path d="M 0,0 L 10,10 M 10,0 L 0,10" stroke="currentColor" strokeWidth="1" />
            </svg>
          </button>
        </div>
      </div>

      <div className="app-body">
        <div className={`main-area ${hasAttachedPanel ? "with-panel" : ""}`}>
          <div className="editor-section">
            <BufferTabs />
            {viewMode === 'code' ? <CodeEditor /> : <TimelineView />}
          </div>
          <div className="bottom-section">
            <WaveformVisualizer />
            <LogPanel />
          </div>
        </div>

        <div className={`side-panel-area${!hasAttachedPanel ? ' side-panel-area--hidden' : ''}`}>
            <SampleBrowser />
            <SynthBrowser />
            <EffectsPanel />
            <HelpPanel />
            <AgentChat />
            <CuePanel />
            <UserSamplePanel />
            <BandControlPanel visible={showBandVisualizer} />
        </div>
      </div>

      <div className="app-footer">
        <span className="footer-info">PiBeat v{__APP_VERSION__}</span>
        <span className="footer-keys">
          <kbd>Ctrl+Enter</kbd> Run | <kbd>Alt+S</kbd> Stop | <kbd>Ctrl+Shift+R</kbd> Record | <kbd>Ctrl+S</kbd> Save | <kbd>Ctrl+O</kbd> Open
        </span>
      </div>

      <AboutModal open={showAbout} onClose={() => setShowAbout(false)} />
    </div>
  );
};

export default App;
