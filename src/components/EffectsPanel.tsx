import React from 'react';
import { useStore } from '../store';
import { FaTimes } from 'react-icons/fa';

const EffectsPanel: React.FC = () => {
  const { effects, setEffects, showEffectsPanel, toggleEffectsPanel } = useStore();

  if (!showEffectsPanel) return null;

  return (
    <div className="side-panel effects-panel">
      <div className="panel-header">
        <h3>Effects</h3>
        <button className="close-btn" onClick={toggleEffectsPanel}>
          <FaTimes />
        </button>
      </div>
      <div className="panel-content">
        <div className="effect-group">
          <label>Reverb Mix</label>
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={effects.reverb_mix}
            onChange={(e) => setEffects({ reverb_mix: parseFloat(e.target.value) })}
          />
          <span className="effect-value">{(effects.reverb_mix * 100).toFixed(0)}%</span>
        </div>

        <div className="effect-group">
          <label>Delay Time</label>
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={effects.delay_time}
            onChange={(e) => setEffects({ delay_time: parseFloat(e.target.value) })}
          />
          <span className="effect-value">{(effects.delay_time * 1000).toFixed(0)}ms</span>
        </div>

        <div className="effect-group">
          <label>Delay Feedback</label>
          <input
            type="range"
            min="0"
            max="0.95"
            step="0.01"
            value={effects.delay_feedback}
            onChange={(e) => setEffects({ delay_feedback: parseFloat(e.target.value) })}
          />
          <span className="effect-value">{(effects.delay_feedback * 100).toFixed(0)}%</span>
        </div>

        <div className="effect-group">
          <label>Distortion</label>
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={effects.distortion}
            onChange={(e) => setEffects({ distortion: parseFloat(e.target.value) })}
          />
          <span className="effect-value">{(effects.distortion * 100).toFixed(0)}%</span>
        </div>

        <div className="effect-group">
          <label>Low-Pass Filter</label>
          <input
            type="range"
            min="100"
            max="20000"
            step="10"
            value={effects.lpf_cutoff}
            onChange={(e) => setEffects({ lpf_cutoff: parseFloat(e.target.value) })}
          />
          <span className="effect-value">{effects.lpf_cutoff.toFixed(0)} Hz</span>
        </div>

        <div className="effect-group">
          <label>High-Pass Filter</label>
          <input
            type="range"
            min="20"
            max="5000"
            step="10"
            value={effects.hpf_cutoff}
            onChange={(e) => setEffects({ hpf_cutoff: parseFloat(e.target.value) })}
          />
          <span className="effect-value">{effects.hpf_cutoff.toFixed(0)} Hz</span>
        </div>

        <div className="effect-presets">
          <h4>Presets</h4>
          <div className="preset-buttons">
            <button onClick={() => setEffects({
              reverb_mix: 0, delay_time: 0, delay_feedback: 0,
              distortion: 0, lpf_cutoff: 20000, hpf_cutoff: 20,
            })}>
              Dry
            </button>
            <button onClick={() => setEffects({
              reverb_mix: 0.5, delay_time: 0, delay_feedback: 0,
              distortion: 0, lpf_cutoff: 20000, hpf_cutoff: 20,
            })}>
              Hall
            </button>
            <button onClick={() => setEffects({
              reverb_mix: 0.2, delay_time: 0.25, delay_feedback: 0.4,
              distortion: 0, lpf_cutoff: 20000, hpf_cutoff: 20,
            })}>
              Echo
            </button>
            <button onClick={() => setEffects({
              reverb_mix: 0.3, delay_time: 0.1, delay_feedback: 0.3,
              distortion: 0.3, lpf_cutoff: 8000, hpf_cutoff: 200,
            })}>
              Lo-Fi
            </button>
            <button onClick={() => setEffects({
              reverb_mix: 0.7, delay_time: 0.5, delay_feedback: 0.6,
              distortion: 0, lpf_cutoff: 4000, hpf_cutoff: 20,
            })}>
              Ambient
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default EffectsPanel;
