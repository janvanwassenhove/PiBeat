import React from 'react';
import { useStore } from '../store';

const BufferTabs: React.FC = () => {
  const { buffers, activeBufferId, setActiveBuffer, viewMode, toggleViewMode } = useStore();

  return (
    <div className="buffer-tabs">
      {/* View mode toggle */}
      <div className="view-mode-toggle">
        <button
          className={`view-toggle-btn ${viewMode === 'code' ? 'active' : ''}`}
          onClick={() => viewMode !== 'code' && toggleViewMode()}
          title="Code Editor"
        >
          <span className="view-toggle-icon">{'{ }'}</span>
          <span className="view-toggle-label">Code</span>
        </button>
        <button
          className={`view-toggle-btn ${viewMode === 'timeline' ? 'active' : ''}`}
          onClick={() => viewMode !== 'timeline' && toggleViewMode()}
          title="Timeline / Track View"
        >
          <span className="view-toggle-icon">≡≡</span>
          <span className="view-toggle-label">Timeline</span>
        </button>
      </div>

      <div className="buffer-tabs-divider" />

      {/* Buffer tabs (shown in both modes for context) */}
      {buffers.map((buffer) => (
        <button
          key={buffer.id}
          className={`buffer-tab ${buffer.id === activeBufferId ? 'active' : ''}`}
          onClick={() => setActiveBuffer(buffer.id)}
          title={buffer.name}
        >
          {buffer.name}
        </button>
      ))}
    </div>
  );
};

export default BufferTabs;
