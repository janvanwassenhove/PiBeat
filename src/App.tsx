import React, { useEffect } from "react";
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
import { useStore } from "./store";
import "./App.css";

const App: React.FC = () => {
  const { fetchSamples, fetchStatus, showSampleBrowser, showSynthBrowser, showEffectsPanel, showHelp, showAgentChat, showCuePanel, viewMode } = useStore();

  useEffect(() => {
    fetchSamples();
    const interval = setInterval(() => {
      fetchStatus();
    }, 1000);
    return () => clearInterval(interval);
  }, [fetchSamples, fetchStatus]);

  const hasSidePanel = showSampleBrowser || showSynthBrowser || showEffectsPanel || showHelp || showAgentChat || showCuePanel;

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
