import React, { useEffect, useRef, useCallback } from 'react';
import { useStore } from '../store';
import { FaTrash, FaArrowDown } from 'react-icons/fa';

/** Extract a line number from log messages like "Line 5: ..." or "Parse error: line 3" */
function extractLineNumber(message: string): number | null {
  // Match "Line N:" at the start of the message (from validation warnings)
  const linePrefix = message.match(/^Line (\d+):/);
  if (linePrefix) return parseInt(linePrefix[1], 10);

  // Match "line N" anywhere in the message (case-insensitive)
  const lineAnywhere = message.match(/\bline\s+(\d+)\b/i);
  if (lineAnywhere) return parseInt(lineAnywhere[1], 10);

  return null;
}

const LogPanel: React.FC = () => {
  const { logs, clearLogs, setErrorLine } = useStore();
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

  const handleLogClick = useCallback((message: string, level: string) => {
    if (level !== 'error' && level !== 'warning') return;
    const line = extractLineNumber(message);
    if (line !== null && line > 0) {
      setErrorLine(line);
    }
  }, [setErrorLine]);

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
        {logs.map((log, i) => {
          const lineNum = (log.level === 'error' || log.level === 'warning') ? extractLineNumber(log.message) : null;
          const isClickable = lineNum !== null && lineNum > 0;
          return (
            <div
              key={i}
              className={`log-entry ${getLogClass(log.level)}${isClickable ? ' log-clickable' : ''}`}
              onClick={isClickable ? () => handleLogClick(log.message, log.level) : undefined}
              title={isClickable ? `Click to go to line ${lineNum}` : undefined}
            >
              {log.timestamp > 0 && (
                <span className="log-time">{formatTime(log.timestamp)}</span>
              )}
              <span className="log-level">[{log.level}]</span>
              <span className="log-message">
                {log.message}
                {isClickable && <span className="log-line-link"> (line {lineNum})</span>}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
};

export default LogPanel;
