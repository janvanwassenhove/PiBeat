import React from 'react';
import { useStore } from '../store';
import { FaTimes } from 'react-icons/fa';

const HelpPanel: React.FC = () => {
  const { showHelp, toggleHelp } = useStore();

  if (!showHelp) return null;

  return (
    <div className="side-panel help-panel">
      <div className="panel-header">
        <h3>Help & Reference</h3>
        <button className="close-btn" onClick={toggleHelp}>
          <FaTimes />
        </button>
      </div>
      <div className="panel-content help-content">
        <section>
          <h4>🎹 Keyboard Shortcuts</h4>
          <div className="shortcut-list">
            <div className="shortcut"><kbd>Alt + R</kbd> <span>Run code</span></div>
            <div className="shortcut"><kbd>Alt + S</kbd> <span>Stop all sound</span></div>
            <div className="shortcut"><kbd>Alt + Shift + R</kbd> <span>Toggle recording</span></div>
          </div>
        </section>

        <section>
          <h4>🎵 Playing Notes</h4>
          <pre>{`# Play a note
play :c4
play :e4, amp: 0.5
play 60    # MIDI note number
play :c4, sustain: 1, attack: 0.1, release: 0.5

# Rest
sleep 0.5  # Wait for half a beat`}</pre>
        </section>

        <section>
          <h4>🎸 Synths</h4>
          <pre>{`use_synth :sine      # Smooth sine wave
use_synth :saw       # Sawtooth (bright)
use_synth :square    # Square wave (hollow)
use_synth :triangle  # Triangle (soft)
use_synth :noise     # White noise
use_synth :pulse     # Pulse wave
use_synth :super_saw # Detuned supersaw`}</pre>
        </section>

        <section>
          <h4>🥁 Samples</h4>
          <pre>{`sample :kick
sample :snare
sample :hihat
sample :clap
sample :kick, amp: 0.8, rate: 1.5`}</pre>
        </section>

        <section>
          <h4>🔁 Loops</h4>
          <pre>{`live_loop :beat do
  sample :kick
  sleep 0.5
  sample :hihat
  sleep 0.5
end`}</pre>
        </section>

        <section>
          <h4>✨ Effects</h4>
          <pre>{`with_fx :reverb, mix: 0.5 do
  play :c4
  sleep 0.5
  play :e4
end

with_fx :echo, time: 0.25, feedback: 0.5 do
  play :g4
end

with_fx :distortion, mix: 0.3 do
  play :c3
end

with_fx :lpf, cutoff: 1000 do
  play :c4
end`}</pre>
        </section>

        <section>
          <h4>🎼 Note Names</h4>
          <div className="note-reference">
            <p>Notes: C, D, E, F, G, A, B</p>
            <p>Sharps: Cs, Ds, Fs, Gs, As (or C#, D#...)</p>
            <p>Flats: Db, Eb, Gb, Ab, Bb</p>
            <p>Octaves: :c3 (low) to :c7 (high)</p>
            <p>MIDI: 60 = C4 (middle C)</p>
          </div>
        </section>

        <section>
          <h4>🎚️ Parameters</h4>
          <div className="param-list">
            <div><code>amp:</code> Volume (0.0 - 1.0+)</div>
            <div><code>pan:</code> Stereo position (-1.0 to 1.0)</div>
            <div><code>sustain:</code> Note duration in beats</div>
            <div><code>attack:</code> Fade-in time</div>
            <div><code>decay:</code> Decay time after attack</div>
            <div><code>release:</code> Fade-out time</div>
            <div><code>rate:</code> Sample playback speed</div>
          </div>
        </section>

        <section>
          <h4>⏱️ Tempo</h4>
          <pre>{`use_bpm 140  # Set tempo to 140 BPM
sleep 1      # Now sleeps for 60/140 seconds`}</pre>
        </section>
      </div>
    </div>
  );
};

export default HelpPanel;
