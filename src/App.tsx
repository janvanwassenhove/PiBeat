import React, { useEffect, useState, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Toolbar from "./components/Toolbar";
import BufferTabs from "./components/BufferTabs";
import CodeEditor from "./components/CodeEditor";
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
import { useStore, AppTheme } from "./store";
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

const App: React.FC = () => {
  const { fetchSamples, fetchStatus, loadUserSamplesDir, showSampleBrowser, showSynthBrowser, showEffectsPanel, showHelp, showAgentChat, showCuePanel, showUserSamplePanel, viewMode, theme, setTheme } = useStore();

  useEffect(() => {
    fetchSamples();
    loadUserSamplesDir();
    const interval = setInterval(() => {
      fetchStatus();
    }, 1000);
    return () => clearInterval(interval);
  }, [fetchSamples, fetchStatus, loadUserSamplesDir]);

  // Apply theme data attribute to root element
  useEffect(() => {
    if (theme === 'pibeat') {
      document.documentElement.removeAttribute('data-theme');
    } else {
      document.documentElement.setAttribute('data-theme', theme);
    }
  }, [theme]);

  const hasSidePanel = showSampleBrowser || showSynthBrowser || showEffectsPanel || showHelp || showAgentChat || showCuePanel || showUserSamplePanel;

  const appWindow = getCurrentWindow();

  const handleMinimize = () => appWindow.minimize();
  const handleMaximize = () => appWindow.toggleMaximize();
  const handleClose = () => appWindow.close();

  return (
    <div className="app">
      <div className="app-header">
        <div className="titlebar-left" data-tauri-drag-region>
          <div className="app-logo">
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
        <div className={`main-area ${hasSidePanel ? "with-panel" : ""}`}>
          <div className="editor-section">
            <BufferTabs />
            {viewMode === 'code' ? <CodeEditor /> : <TimelineView />}
          </div>
          <div className="bottom-section">
            <WaveformVisualizer />
            <LogPanel />
          </div>
        </div>

        {hasSidePanel && (
          <div className="side-panel-area">
            <SampleBrowser />
            <SynthBrowser />
            <EffectsPanel />
            <HelpPanel />
            <AgentChat />
            <CuePanel />
            <UserSamplePanel />
          </div>
        )}
      </div>

      <div className="app-footer">
        <span className="footer-info">PiBeat v0.1.0</span>
        <span className="footer-keys">
          <kbd>Alt+R</kbd> Run | <kbd>Alt+S</kbd> Stop | <kbd>Alt+Shift+R</kbd> Record
        </span>
      </div>
    </div>
  );
};

export default App;
