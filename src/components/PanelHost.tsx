import React, { useEffect, useCallback } from 'react';
import { emit, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { FaCompress } from 'react-icons/fa';
import { useStore } from '../store';
import SampleBrowser from './SampleBrowser';
import SynthBrowser from './SynthBrowser';
import EffectsPanel from './EffectsPanel';
import HelpPanel from './HelpPanel';
import AgentChat from './AgentChat';
import CuePanel from './CuePanel';
import UserSamplePanel from './UserSamplePanel';
import '../App.css';

const PANEL_SHOW_FLAGS: Record<string, string> = {
  sampleBrowser: 'showSampleBrowser',
  synthBrowser: 'showSynthBrowser',
  effectsPanel: 'showEffectsPanel',
  helpPanel: 'showHelp',
  agentChat: 'showAgentChat',
  cuePanel: 'showCuePanel',
  userSamplePanel: 'showUserSamplePanel',
};

const PANEL_COMPONENTS: Record<string, React.FC> = {
  sampleBrowser: SampleBrowser,
  synthBrowser: SynthBrowser,
  effectsPanel: EffectsPanel,
  helpPanel: HelpPanel,
  agentChat: AgentChat,
  cuePanel: CuePanel,
  userSamplePanel: UserSamplePanel,
};

const PANEL_TITLES: Record<string, string> = {
  sampleBrowser: 'Samples',
  synthBrowser: 'Synths',
  effectsPanel: 'Effects',
  helpPanel: 'Help & Reference',
  agentChat: 'Agent Chat',
  cuePanel: 'Cues',
  userSamplePanel: 'My Samples',
};

interface PanelHostProps {
  panelId: string;
}

/**
 * Standalone host for rendering a single panel in its own Tauri window.
 * This component runs in a separate WebviewWindow context.
 */
const PanelHost: React.FC<PanelHostProps> = ({ panelId }) => {
  const { fetchSamples, loadUserSamplesDir, theme } = useStore();

  // Force the panel's show flag to true and clear detached state
  // so DetachablePanel renders inline instead of trying to open another window
  useEffect(() => {
    const flag = PANEL_SHOW_FLAGS[panelId];
    const updates: Record<string, any> = {};
    if (flag) {
      updates[flag] = true;
    }
    // Clear detached state for this panel in our local store (don't persist to localStorage)
    const currentDetached = useStore.getState().detachedPanels;
    if (currentDetached[panelId]) {
      const newDetached = { ...currentDetached };
      delete newDetached[panelId];
      updates.detachedPanels = newDetached;
    }
    useStore.setState(updates as any);
  }, [panelId]);

  // Initialize data that panels need
  useEffect(() => {
    fetchSamples();
    loadUserSamplesDir();
  }, [fetchSamples, loadUserSamplesDir]);

  // Override updateBufferCode to emit event to main window
  useEffect(() => {
    const originalUpdate = useStore.getState().updateBufferCode;
    useStore.setState({
      updateBufferCode: (id: number, code: string) => {
        // Update local store (for UI feedback)
        originalUpdate(id, code);
        // Emit event to main window for the actual editor update
        emit('panel-insert-code', { id, code });
      },
    });
  }, []);

  // Override toggle/close to emit attach event and close window
  useEffect(() => {
    const flag = PANEL_SHOW_FLAGS[panelId];
    if (!flag) return;

    const toggleFns: Record<string, string> = {
      showSampleBrowser: 'toggleSampleBrowser',
      showSynthBrowser: 'toggleSynthBrowser',
      showEffectsPanel: 'toggleEffectsPanel',
      showHelp: 'toggleHelp',
      showAgentChat: 'toggleAgentChat',
      showCuePanel: 'toggleCuePanel',
      showUserSamplePanel: 'toggleUserSamplePanel',
    };

    const fnName = toggleFns[flag] as keyof ReturnType<typeof useStore.getState>;
    if (fnName) {
      useStore.setState({
        [fnName]: () => {
          // Emit attach event to main window, then close this window
          emit('panel-attach', { panelId });
          getCurrentWindow().close();
        },
      } as any);
    }
  }, [panelId]);

  // Apply theme
  useEffect(() => {
    if (theme === 'pibeat') {
      document.documentElement.removeAttribute('data-theme');
    } else {
      document.documentElement.setAttribute('data-theme', theme);
    }
  }, [theme]);

  // Listen for theme changes from main window
  useEffect(() => {
    const unlisten = listen<{ theme: string }>('theme-changed', (event) => {
      const newTheme = event.payload.theme as import('../store').AppTheme;
      useStore.setState({ theme: newTheme });
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // Set window title
  useEffect(() => {
    const title = PANEL_TITLES[panelId] || 'Panel';
    getCurrentWindow().setTitle(`PiBeat — ${title}`);
  }, [panelId]);

  const appWindow = getCurrentWindow();
  const handleMinimize = useCallback(() => appWindow.minimize(), [appWindow]);
  const handleMaximize = useCallback(() => appWindow.toggleMaximize(), [appWindow]);
  const handleClose = useCallback(() => appWindow.close(), [appWindow]);
  const handleReattach = useCallback(() => {
    emit('panel-attach', { panelId });
    appWindow.close();
  }, [appWindow, panelId]);

  const PanelComponent = PANEL_COMPONENTS[panelId];
  if (!PanelComponent) {
    return <div className="panel-host-error">Unknown panel: {panelId}</div>;
  }

  const panelTitle = PANEL_TITLES[panelId] || 'Panel';

  return (
    <div className="panel-host" data-panel-window="true">
      <div className="panel-host-titlebar" data-tauri-drag-region>
        <div className="panel-host-titlebar-left" data-tauri-drag-region>
          <span className="panel-host-logo">&#9835;</span>
          <span className="panel-host-title">{panelTitle}</span>
        </div>
        <div className="panel-host-titlebar-controls">
          <button className="titlebar-button" onClick={handleReattach} title="Re-attach to main window">
            <FaCompress size={10} />
          </button>
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
      <div className="panel-host-body">
        <PanelComponent />
      </div>
    </div>
  );
};

export default PanelHost;
