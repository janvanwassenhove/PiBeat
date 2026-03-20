import React, { useEffect, useRef } from 'react';
import { useStore } from '../store';
import { FaTrash, FaBullseye, FaPlay } from 'react-icons/fa';
import DetachablePanel from './DetachablePanel';

const CuePanel: React.FC = () => {
  const { cueEvents, clearCues, showCuePanel, toggleCuePanel } = useStore();
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [cueEvents]);

  if (!showCuePanel) return null;

  const formatTime = (ts: number) => {
    const d = new Date(ts);
    return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}.${(d.getMilliseconds() / 100).toFixed(0)}`;
  };

  const formatRelativeTime = (ts: number) => {
    const now = Date.now();
    const diff = now - ts;
    if (diff < 1000) return 'just now';
    if (diff < 60000) return `${Math.floor(diff / 1000)}s ago`;
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
    return `${Math.floor(diff / 3600000)}h ago`;
  };

  return (
    <DetachablePanel
      panelId="cuePanel"
      title="Cues"
      icon={<FaBullseye />}
      onClose={toggleCuePanel}
      className="cue-panel"
      defaultHeight={400}
      headerActions={
        <button 
          className="close-btn" 
          onClick={clearCues} 
          title="Clear All Cues"
          disabled={cueEvents.length === 0}
        >
          <FaTrash />
        </button>
      }
    >
      <div className="cue-content" ref={containerRef}>
        {cueEvents.length === 0 && (
          <div className="cue-empty">
            <div className="cue-empty-icon">
              <FaBullseye />
            </div>
            <div className="cue-empty-text">
              No cues yet
            </div>
            <div className="cue-empty-hint">
              Cues are sync points for coordinating live loops and threads
            </div>
          </div>
        )}
        {cueEvents.map((cue) => (
          <div key={cue.id} className="cue-entry">
            <div className="cue-entry-header">
              <span className="cue-icon">
                <FaPlay />
              </span>
              <span className="cue-name">:{cue.name}</span>
              <span className="cue-time" title={formatTime(cue.timestamp)}>
                {formatRelativeTime(cue.timestamp)}
              </span>
            </div>
            {cue.buffer && (
              <div className="cue-buffer">
                from {cue.buffer}
              </div>
            )}
          </div>
        ))}
      </div>
      <div className="cue-footer">
        <div className="cue-info">
          {cueEvents.length} {cueEvents.length === 1 ? 'cue' : 'cues'}
        </div>
      </div>
    </DetachablePanel>
  );
};

export default CuePanel;
