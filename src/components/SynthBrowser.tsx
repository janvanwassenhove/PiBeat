import React, { useState } from 'react';
import { useStore } from '../store';
import { FaPlay, FaTimes, FaChevronRight, FaChevronDown } from 'react-icons/fa';

interface SynthEntry {
  name: string;       // display name
  sonicPiName: string; // use_synth :xxx
  category: string;
}

const SYNTH_CATALOG: SynthEntry[] = [
  // Basic waveforms
  { name: 'Sine / Beep', sonicPiName: 'sine', category: 'Basic' },
  { name: 'Saw', sonicPiName: 'saw', category: 'Basic' },
  { name: 'Square', sonicPiName: 'square', category: 'Basic' },
  { name: 'Triangle', sonicPiName: 'tri', category: 'Basic' },
  { name: 'Noise', sonicPiName: 'noise', category: 'Basic' },
  { name: 'Pulse', sonicPiName: 'pulse', category: 'Basic' },
  { name: 'Super Saw', sonicPiName: 'super_saw', category: 'Basic' },

  // Detuned
  { name: 'Detuned Saw', sonicPiName: 'dsaw', category: 'Detuned' },
  { name: 'Detuned Pulse', sonicPiName: 'dpulse', category: 'Detuned' },
  { name: 'Detuned Tri', sonicPiName: 'dtri', category: 'Detuned' },

  // FM synthesis
  { name: 'FM', sonicPiName: 'fm', category: 'FM' },
  { name: 'Mod FM', sonicPiName: 'mod_fm', category: 'FM' },

  // Modulated
  { name: 'Mod Sine', sonicPiName: 'mod_sine', category: 'Modulated' },
  { name: 'Mod Saw', sonicPiName: 'mod_saw', category: 'Modulated' },
  { name: 'Mod Detuned Saw', sonicPiName: 'mod_dsaw', category: 'Modulated' },
  { name: 'Mod Tri', sonicPiName: 'mod_tri', category: 'Modulated' },
  { name: 'Mod Pulse', sonicPiName: 'mod_pulse', category: 'Modulated' },

  // Classic
  { name: 'TB-303', sonicPiName: 'tb303', category: 'Classic' },
  { name: 'Prophet', sonicPiName: 'prophet', category: 'Classic' },
  { name: 'Zawa', sonicPiName: 'zawa', category: 'Classic' },

  // Filtered / Layered
  { name: 'Blade', sonicPiName: 'blade', category: 'Filtered' },
  { name: 'Tech Saws', sonicPiName: 'tech_saws', category: 'Filtered' },
  { name: 'Hoover', sonicPiName: 'hoover', category: 'Filtered' },

  // Plucked / Percussive
  { name: 'Pluck', sonicPiName: 'pluck', category: 'Plucked' },
  { name: 'Piano', sonicPiName: 'piano', category: 'Plucked' },
  { name: 'Pretty Bell', sonicPiName: 'pretty_bell', category: 'Plucked' },
  { name: 'Dull Bell', sonicPiName: 'dull_bell', category: 'Plucked' },

  // Pads / Ambient
  { name: 'Hollow', sonicPiName: 'hollow', category: 'Pads' },
  { name: 'Dark Ambience', sonicPiName: 'dark_ambience', category: 'Pads' },
  { name: 'Growl', sonicPiName: 'growl', category: 'Pads' },

  // Chiptune
  { name: 'Chip Lead', sonicPiName: 'chip_lead', category: 'Chiptune' },
  { name: 'Chip Bass', sonicPiName: 'chip_bass', category: 'Chiptune' },
  { name: 'Chip Noise', sonicPiName: 'chip_noise', category: 'Chiptune' },

  // Colored noise
  { name: 'Brown Noise', sonicPiName: 'bnoise', category: 'Noise' },
  { name: 'Pink Noise', sonicPiName: 'pnoise', category: 'Noise' },
  { name: 'Grey Noise', sonicPiName: 'gnoise', category: 'Noise' },
  { name: 'Clip Noise', sonicPiName: 'cnoise', category: 'Noise' },

  // Sub
  { name: 'Sub Pulse', sonicPiName: 'sub_pulse', category: 'Sub' },
];

const FaKeyboard = () => <span style={{ fontSize: '14px' }}>🎹</span>;

const SynthBrowser: React.FC = () => {
  const {
    showSynthBrowser,
    toggleSynthBrowser,
    previewSynth,
    updateBufferCode,
    buffers,
    activeBufferId,
  } = useStore();
  const [filter, setFilter] = useState('');
  const [collapsedCategories, setCollapsedCategories] = useState<Record<string, boolean>>({});

  // Group and filter synths
  const filtered = filter
    ? SYNTH_CATALOG.filter(
        (s) =>
          s.name.toLowerCase().includes(filter.toLowerCase()) ||
          s.sonicPiName.toLowerCase().includes(filter.toLowerCase()) ||
          s.category.toLowerCase().includes(filter.toLowerCase())
      )
    : SYNTH_CATALOG;

  const grouped = filtered.reduce((acc, s) => {
    if (!acc[s.category]) acc[s.category] = [];
    acc[s.category].push(s);
    return acc;
  }, {} as Record<string, SynthEntry[]>);

  // Initialize all categories as collapsed on first render
  React.useEffect(() => {
    const categories = Object.keys(grouped);
    if (categories.length > 0 && Object.keys(collapsedCategories).length === 0) {
      const initialState = categories.reduce((acc, cat) => ({ ...acc, [cat]: true }), {});
      setCollapsedCategories(initialState);
    }
  }, [grouped, collapsedCategories]);

  const toggleCategory = (category: string) => {
    setCollapsedCategories((prev) => ({ ...prev, [category]: !prev[category] }));
  };

  if (!showSynthBrowser) return null;

  const insertSynth = (sonicPiName: string) => {
    const buffer = buffers.find((b) => b.id === activeBufferId);
    if (buffer) {
      updateBufferCode(
        activeBufferId,
        buffer.code + `\nuse_synth :${sonicPiName}\nplay :c4, amp: 0.5, sustain: 0.5\n`
      );
    }
  };

  return (
    <div className="side-panel sample-browser">
      <div className="panel-header">
        <h3><FaKeyboard /> Synths</h3>
        <button className="close-btn" onClick={toggleSynthBrowser}>
          <FaTimes />
        </button>
      </div>
      <div className="panel-content">
        <div className="synth-filter">
          <input
            type="text"
            placeholder="Filter synths..."
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="synth-filter-input"
          />
        </div>
        {Object.keys(grouped).length === 0 && (
          <div className="empty-state">
            <p>No synths match your filter.</p>
          </div>
        )}
        {Object.entries(grouped).map(([category, items]) => (
          <div key={category} className="sample-category">
            <h4 className="category-title" onClick={() => toggleCategory(category)}>
              <span className="category-chevron">
                {collapsedCategories[category] ? <FaChevronRight /> : <FaChevronDown />}
              </span>
              {category}
              <span className="category-count">{items.length}</span>
            </h4>
            {!collapsedCategories[category] && (
              <div className="sample-list">
                {items.map((synth) => (
                  <div key={synth.sonicPiName} className="sample-item">
                    <div className="synth-info">
                      <span className="sample-name">{synth.name}</span>
                      <span className="synth-code">:{synth.sonicPiName}</span>
                    </div>
                    <div className="sample-actions">
                      <button
                        className="sample-play-btn"
                        onClick={() => previewSynth(synth.sonicPiName)}
                        title="Preview synth"
                      >
                        <FaPlay />
                      </button>
                      <button
                        className="sample-insert-btn"
                        onClick={() => insertSynth(synth.sonicPiName)}
                        title="Insert into code"
                      >
                        +
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
};

export default SynthBrowser;
