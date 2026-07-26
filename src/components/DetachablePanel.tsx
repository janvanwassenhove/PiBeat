import React, { useEffect, useRef } from 'react';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useStore } from '../store';
import { useShallow } from 'zustand/react/shallow';
import { FaTimes, FaExternalLinkAlt } from 'react-icons/fa';

interface DetachablePanelProps {
  panelId: string;
  title: React.ReactNode;
  icon?: React.ReactNode;
  onClose: () => void;
  className?: string;
  children: React.ReactNode;
  headerActions?: React.ReactNode;
  defaultWidth?: number;
  defaultHeight?: number;
}

const PANEL_OFFSETS: Record<string, { x: number; y: number }> = {
  sampleBrowser: { x: 100, y: 80 },
  synthBrowser: { x: 140, y: 100 },
  effectsPanel: { x: 180, y: 120 },
  helpPanel: { x: 220, y: 140 },
  agentChat: { x: 260, y: 160 },
  cuePanel: { x: 300, y: 180 },
  userSamplePanel: { x: 340, y: 200 },
};

// Detect if we're running inside a panel window (PanelHost)
const isPanelWindow = () => {
  try {
    return new URLSearchParams(window.location.search).has('panel');
  } catch {
    return false;
  }
};

const DetachablePanel: React.FC<DetachablePanelProps> = ({
  panelId,
  title,
  icon,
  onClose,
  className = '',
  children,
  headerActions,
  defaultWidth = 340,
  defaultHeight = 520,
}) => {
  const {
    detachedPanels,
    toggleDetachPanel,
  } = useStore(
    useShallow((s) => ({
      detachedPanels: s.detachedPanels,
      toggleDetachPanel: s.toggleDetachPanel,
    })),
  );
  const inPanelWindow = isPanelWindow();
  const isDetached = !inPanelWindow && !!detachedPanels[panelId];
  const windowRef = useRef<WebviewWindow | null>(null);
  const creatingRef = useRef(false);

  const handleDetach = () => toggleDetachPanel(panelId);

  // Manage the WebviewWindow lifecycle when detach state changes
  useEffect(() => {
    if (isDetached && !windowRef.current && !creatingRef.current) {
      creatingRef.current = true;

      (async () => {
        const label = `panel-${panelId}`;

        try {
          // Check if window already exists
          const existing = await WebviewWindow.getByLabel(label);
          if (existing) {
            await existing.setFocus();
            windowRef.current = existing;
            creatingRef.current = false;
            return;
          }
        } catch { /* doesn't exist */ }

        try {
          const win = new WebviewWindow(label, {
            url: `index.html?panel=${panelId}`,
            title: `PiBeat — ${typeof title === 'string' ? title : panelId}`,
            width: defaultWidth,
            height: defaultHeight,
            minWidth: 250,
            minHeight: 200,
            resizable: true,
            decorations: false,
            center: false,
            x: PANEL_OFFSETS[panelId]?.x ?? 200,
            y: PANEL_OFFSETS[panelId]?.y ?? 150,
          });

          win.once('tauri://error', (e) => {
            console.error(`[DetachablePanel] Window error for ${panelId}:`, e);
            windowRef.current = null;
            creatingRef.current = false;
            // Re-attach if window creation failed
            if (useStore.getState().detachedPanels[panelId]) {
              toggleDetachPanel(panelId);
            }
          });

          win.once('tauri://created', () => {
            windowRef.current = win;
            creatingRef.current = false;
          });

          win.once('tauri://destroyed', () => {
            windowRef.current = null;
            creatingRef.current = false;
            // Re-attach when window is closed (by user or OS)
            if (useStore.getState().detachedPanels[panelId]) {
              toggleDetachPanel(panelId);
            }
          });
        } catch (e) {
          console.error(`[DetachablePanel] Failed to create window for ${panelId}:`, e);
          creatingRef.current = false;
          if (useStore.getState().detachedPanels[panelId]) {
            toggleDetachPanel(panelId);
          }
        }
      })();
    }

    if (!isDetached && windowRef.current) {
      windowRef.current.close().catch(() => {});
      windowRef.current = null;
    }
  }, [isDetached, panelId, defaultWidth, defaultHeight, title, toggleDetachPanel]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (windowRef.current) {
        windowRef.current.close().catch(() => {});
        windowRef.current = null;
      }
    };
  }, []);

  // When detached, don't render anything in the main window
  if (isDetached) {
    return null;
  }

  return (
    <div className={`side-panel ${className}`}>
      <div className="panel-header">
        <h3>{icon && <>{icon} </>}{title}</h3>
        <div className="panel-header-actions">
          {headerActions}
          {!inPanelWindow && (
            <button
              className="close-btn"
              onClick={handleDetach}
              title="Detach as separate window"
            >
              <FaExternalLinkAlt />
            </button>
          )}
          <button className="close-btn" onClick={onClose}>
            <FaTimes />
          </button>
        </div>
      </div>
      {children}
    </div>
  );
};

export default DetachablePanel;
