import React, { useEffect } from 'react';
import { useStore } from '../store';
import {
  FaPlay,
  FaStop,
  FaPause,
  FaCircle,
  FaSquare,
  FaSave,
  FaVolumeUp,
  FaVolumeMute,
  FaMusic,
  FaQuestionCircle,
  FaRobot,
  FaBullseye,
  FaFolderOpen,
} from 'react-icons/fa';

const FaWaveSquare = () => <span style={{fontSize: '14px'}}>~</span>;
const FaKeyboard = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round">
    <rect x="1" y="4" width="14" height="9" rx="1.5" />
    <line x1="4" y1="7" x2="5" y2="7" /><line x1="7" y1="7" x2="8" y2="7" /><line x1="10" y1="7" x2="11" y2="7" />
    <line x1="4" y1="10" x2="11" y2="10" />
  </svg>
);
const FaSuperCollider = () => <span style={{fontSize: '14px', fontWeight: 'bold'}}>SC</span>;
const FaBandIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="5" cy="5" r="2" /><line x1="5" y1="7" x2="5" y2="12" />
    <circle cx="11" cy="5" r="2" /><line x1="11" y1="7" x2="11" y2="12" />
    <line x1="3" y1="10" x2="7" y2="10" /><line x1="9" y1="10" x2="13" y2="10" />
  </svg>
);

const Toolbar: React.FC = () => {
  const {
    isPlaying,
    isPaused,
    isRecording,
    masterVolume,
    scStatus,
    theme,
    runCode,
    stopAudio,
    pauseAudio,
    resumeAudio,
    setVolume,
    startRecording,
    stopRecording,
    toggleSampleBrowser,
    toggleSynthBrowser,
    toggleEffectsPanel,
    toggleHelp,
    toggleAgentChat,
    toggleCuePanel,
    toggleUserSamplePanel,
    toggleBandVisualizer,
    showSampleBrowser,
    showSynthBrowser,
    showEffectsPanel,
    showHelp,
    showAgentChat,
    showCuePanel,
    showUserSamplePanel,
    showBandVisualizer,
    initSuperCollider,
    toggleScEngine,
    fetchScStatus,
  } = useStore();

  // Check SC status on mount
  useEffect(() => {
    fetchScStatus();
  }, []);

  const handleScToggle = async () => {
    if (!scStatus.available || !scStatus.booted) {
      // Try to initialize
      await initSuperCollider();
    } else {
      // Toggle on/off
      await toggleScEngine(!scStatus.enabled);
    }
  };

  return (
    <div className="toolbar">
      <div className="toolbar-group toolbar-main">
        <button
          className={`toolbar-btn run-btn ${isPlaying ? 'playing' : ''}`}
          onClick={runCode}
          title="Run (Ctrl+Enter / Alt+R)"
        >
          <FaPlay /> Run
        </button>
        <button
          className="toolbar-btn stop-btn"
          onClick={stopAudio}
          title="Stop (Ctrl+. / Alt+S)"
        >
          <FaStop /> Stop
        </button>
        <button
          className={`toolbar-btn pause-btn ${isPaused ? 'paused' : ''}`}
          onClick={isPaused ? resumeAudio : pauseAudio}
          disabled={!isPlaying}
          title={isPaused ? 'Resume' : 'Pause'}
        >
          {isPaused ? <FaPlay /> : <FaPause />}
          {isPaused ? 'Resume' : 'Pause'}
        </button>

        <div className="toolbar-separator" />

        <button
          className={`toolbar-btn rec-btn ${isRecording ? 'recording' : ''}`}
          onClick={isRecording ? () => stopRecording() : startRecording}
          title={isRecording ? 'Stop Recording (Ctrl+Shift+R)' : 'Start Recording (Ctrl+Shift+R)'}
        >
          {isRecording ? <FaSquare /> : <FaCircle />}
          {isRecording ? 'Stop Rec' : 'Rec'}
        </button>

        {isRecording && (
          <button
            className="toolbar-btn save-btn"
            onClick={() => stopRecording()}
            title="Save Recording"
          >
            <FaSave /> Save
          </button>
        )}
      </div>

      <div className="toolbar-group toolbar-controls">
        <div className="control-group">
          <label>
            {masterVolume > 0 ? <FaVolumeUp /> : <FaVolumeMute />}
          </label>
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={masterVolume}
            onChange={(e) => setVolume(parseFloat(e.target.value))}
            className="volume-slider"
            title={`Volume: ${Math.round(masterVolume * 100)}%`}
          />
          <span className="control-value">{Math.round(masterVolume * 100)}%</span>
        </div>

      </div>

      <div className="toolbar-group toolbar-panels">
        <button
          className={`toolbar-btn panel-btn ${showSampleBrowser ? 'panel-btn-active' : ''}`}
          onClick={toggleSampleBrowser}
          title="Sample Browser"
        >
          <FaMusic />
        </button>
        <button
          className={`toolbar-btn panel-btn ${showSynthBrowser ? 'panel-btn-active' : ''}`}
          onClick={toggleSynthBrowser}
          title="Synth Browser"
        >
          <FaKeyboard />
        </button>
        <button
          className={`toolbar-btn panel-btn ${showEffectsPanel ? 'panel-btn-active' : ''}`}
          onClick={toggleEffectsPanel}
          title="Effects"
        >
          <FaWaveSquare />
        </button>
        <button
          className={`toolbar-btn panel-btn ${showHelp ? 'panel-btn-active' : ''}`}
          onClick={toggleHelp}
          title="Help"
        >
          <FaQuestionCircle />
        </button>
        <button
          className={`toolbar-btn panel-btn ${showAgentChat ? 'panel-btn-active' : ''}`}
          onClick={toggleAgentChat}
          title="AI Agent Chat"
        >
          <FaRobot />
        </button>
        <button
          className={`toolbar-btn panel-btn ${showUserSamplePanel ? 'panel-btn-active' : ''}`}
          onClick={toggleUserSamplePanel}
          title="My Samples"
        >
          <FaFolderOpen />
        </button>
        <button
          className={`toolbar-btn panel-btn ${showBandVisualizer ? 'panel-btn-active' : ''}`}
          onClick={toggleBandVisualizer}
          title="Band Visualizer"
        >
          <FaBandIcon />
        </button>
        <button
          className={`toolbar-btn panel-btn ${showCuePanel ? 'panel-btn-active' : ''}`}
          onClick={toggleCuePanel}
          title="Cue Panel"
        >
          <FaBullseye />
        </button>

        <div className="toolbar-separator" />

        <button
          className={`toolbar-btn panel-btn ${scStatus.enabled ? 'sc-active' : ''}`}
          onClick={handleScToggle}
          title={scStatus.enabled
            ? 'SuperCollider engine active — click to switch to built-in engine'
            : scStatus.available
              ? 'Click to enable SuperCollider engine'
              : 'Click to initialize SuperCollider (requires SC installed)'
          }
          style={{
            color: scStatus.enabled ? (theme === 'sonicpi' ? '#ff59b2' : theme === 'amber' ? '#ffaa00' : '#00ff88') : scStatus.available ? '#ffa500' : undefined,
            borderColor: scStatus.enabled ? (theme === 'sonicpi' ? '#ff59b2' : theme === 'amber' ? '#ffaa00' : '#00ff88') : undefined,
          }}
        >
          <FaSuperCollider />
        </button>
      </div>
    </div>
  );
};

export default Toolbar;
