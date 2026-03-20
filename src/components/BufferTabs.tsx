import React, { useState, useRef, useEffect } from 'react';
import { useStore } from '../store';
import {
  FaPlus,
  FaTimes,
  FaSave,
  FaFolderOpen,
} from 'react-icons/fa';

const BufferTabs: React.FC = () => {
  const {
    buffers,
    activeBufferId,
    setActiveBuffer,
    addBuffer,
    removeBuffer,
    renameBuffer,
    saveBufferToFile,
    loadBufferFromFile,
    viewMode,
    toggleViewMode,
  } = useStore();

  const [editingId, setEditingId] = useState<number | null>(null);
  const [editName, setEditName] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editingId !== null && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [editingId]);

  const handleDoubleClick = (id: number, name: string) => {
    setEditingId(id);
    setEditName(name);
  };

  const commitRename = () => {
    if (editingId !== null && editName.trim()) {
      renameBuffer(editingId, editName.trim());
    }
    setEditingId(null);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') commitRename();
    if (e.key === 'Escape') setEditingId(null);
  };

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

      {/* Buffer tabs */}
      {buffers.map((buffer) => (
        <div
          key={buffer.id}
          className={`buffer-tab ${buffer.id === activeBufferId ? 'active' : ''}`}
          onClick={() => setActiveBuffer(buffer.id)}
          onDoubleClick={() => handleDoubleClick(buffer.id, buffer.name)}
          title={`${buffer.name} (double-click to rename)`}
        >
          {editingId === buffer.id ? (
            <input
              ref={inputRef}
              className="buffer-tab-rename-input"
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              onBlur={commitRename}
              onKeyDown={handleKeyDown}
              onClick={(e) => e.stopPropagation()}
              maxLength={24}
            />
          ) : (
            <span className="buffer-tab-name">{buffer.name}</span>
          )}
          {buffers.length > 1 && (
            <button
              className="buffer-tab-close"
              onClick={(e) => {
                e.stopPropagation();
                removeBuffer(buffer.id);
              }}
              title={`Close ${buffer.name}`}
            >
              <FaTimes />
            </button>
          )}
        </div>
      ))}

      {/* Add buffer button */}
      <button
        className="buffer-tab buffer-tab-add"
        onClick={addBuffer}
        title="Add new buffer"
      >
        <FaPlus />
      </button>

      {/* Spacer pushes file actions to the right */}
      <div className="buffer-tabs-spacer" />

      {/* File actions */}
      <div className="buffer-tabs-file-actions">
        <button
          className="buffer-file-btn"
          onClick={() => loadBufferFromFile()}
          title="Open .sonicpi file into current buffer"
        >
          <FaFolderOpen /> <span className="buffer-file-label">Open</span>
        </button>
        <button
          className="buffer-file-btn"
          onClick={() => saveBufferToFile()}
          title="Save current buffer as .sonicpi file"
        >
          <FaSave /> <span className="buffer-file-label">Save</span>
        </button>
      </div>
    </div>
  );
};

export default BufferTabs;
