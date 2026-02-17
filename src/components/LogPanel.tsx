import React, { useEffect, useRef } from 'react';
import { useStore } from '../store';
import { FaTrash, FaArrowDown } from 'react-icons/fa';

const LogPanel: React.FC = () => {
  const { logs, clearLogs } = useStore();
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [logs]);

  const getLogClass = (level: string) => {
    switch (level) {
      case 'error': return 'log-error';
      case 'warning': return 'log-warning';
      case 'comment': return 'log-comment';
      case 'info':
      default: return 'log-info';
    }
  };

  const formatTime = (ts: number) => {
    if (ts === 0) return '';
    const d = new Date(ts);
    return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}`;
  };

  return (
    <div className="log-panel">
      <div className="log-header">
        <span className="log-title">
          <FaArrowDown /> Log
        </span>
        <button className="log-clear-btn" onClick={clearLogs} title="Clear Logs">
          <FaTrash />
        </button>
      </div>
      <div className="log-content" ref={containerRef}>
        {logs.length === 0 && (
          <div className="log-empty">
            Ready. Press <kbd>Run</kbd> or <kbd>Alt+R</kbd> to execute code.
          </div>
        )}
        {logs.map((log, i) => (
          <div key={i} className={`log-entry ${getLogClass(log.level)}`}>
            {log.timestamp > 0 && (
              <span className="log-time">{formatTime(log.timestamp)}</span>
            )}
            <span className="log-level">[{log.level}]</span>
            <span className="log-message">{log.message}</span>
          </div>
        ))}
      </div>
    </div>
  );
};

export default LogPanel;
