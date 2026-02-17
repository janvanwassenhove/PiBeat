import React, { useEffect } from 'react';
import { useStore } from '../store';
import {
  FaPlay,
  FaStop,
  FaCircle,
  FaSquare,
  FaSave,
  FaVolumeUp,
  FaVolumeMute,
  FaMusic,
  FaQuestionCircle,
  FaRobot,
  FaBullseye,
} from 'react-icons/fa';

const FaWaveSquare = () => <span style={{fontSize: '14px'}}>~</span>;
const FaKeyboard = () => <span style={{fontSize: '14px'}}>🎹</span>;
const FaSuperCollider = () => <span style={{fontSize: '14px', fontWeight: 'bold'}}>SC</span>;

const Toolbar: React.FC = () => {
  const {
    isPlaying,
    isRecording,
    masterVolume,
    bpm,
    scStatus,
    runCode,
    stopAudio,
    setVolume,
    setBpm,
    startRecording,
    stopRecording,
    toggleSampleBrowser,
    toggleSynthBrowser,
    toggleEffectsPanel,
    toggleHelp,
    toggleAgentChat,
    toggleCuePanel,
    showSampleBrowser,
    showSynthBrowser,
    showEffectsPanel,
    showHelp,
    showAgentChat,
    showCuePanel,
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
          title="Run (Alt+R)"
        >
          <FaPlay /> Run
        </button>
        <button
          className="toolbar-btn stop-btn"
          onClick={stopAudio}
          title="Stop (Alt+S)"
        >
          <FaStop /> Stop
        </button>

        <div className="toolbar-separator" />

        <button
          className={`toolbar-btn rec-btn ${isRecording ? 'recording' : ''}`}
          onClick={isRecording ? () => stopRecording() : startRecording}
          title={isRecording ? 'Stop Recording (Alt+Shift+R)' : 'Start Recording (Alt+Shift+R)'}
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

        <div className="control-group">
          <label>BPM</label>
          <input
            type="number"
            min="20"
            max="300"
            value={bpm}
            onChange={(e) => setBpm(parseInt(e.target.value) || 120)}
            className="bpm-input"
          />
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
            color: scStatus.enabled ? '#00ff88' : scStatus.available ? '#ffa500' : undefined,
            borderColor: scStatus.enabled ? '#00ff88' : undefined,
          }}
        >
          <FaSuperCollider />
        </button>
      </div>
    </div>
  );
};

export default Toolbar;
