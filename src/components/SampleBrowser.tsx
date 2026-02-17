import React, { useEffect, useState } from 'react';
import { useStore } from '../store';
import { FaPlay, FaTimes, FaFolder, FaChevronRight, FaChevronDown } from 'react-icons/fa';

const SampleBrowser: React.FC = () => {
  const { samples, fetchSamples, playSampleFile, showSampleBrowser, toggleSampleBrowser, updateBufferCode, buffers, activeBufferId } = useStore();
  const [filter, setFilter] = useState('');
  const [collapsedCategories, setCollapsedCategories] = useState<Record<string, boolean>>({});

  // Filter and group samples by category
  const filtered = filter
    ? samples.filter(
        (s) =>
          s.name.toLowerCase().includes(filter.toLowerCase()) ||
          s.category.toLowerCase().includes(filter.toLowerCase())
      )
    : samples;

  const grouped = filtered.reduce((acc, s) => {
    if (!acc[s.category]) acc[s.category] = [];
    acc[s.category].push(s);
    return acc;
  }, {} as Record<string, typeof samples>);

  useEffect(() => {
    if (showSampleBrowser) {
      fetchSamples();
    }
  }, [showSampleBrowser, fetchSamples]);

  // Initialize all categories as collapsed on first render
  useEffect(() => {
    const categories = Object.keys(grouped);
    if (categories.length > 0 && Object.keys(collapsedCategories).length === 0) {
      const initialState = categories.reduce((acc, cat) => ({ ...acc, [cat]: true }), {});
      setCollapsedCategories(initialState);
    }
  }, [grouped, collapsedCategories]);

  if (!showSampleBrowser) return null;

  const toggleCategory = (category: string) => {
    setCollapsedCategories((prev) => ({ ...prev, [category]: !prev[category] }));
  };

  const insertSample = (name: string) => {
    const buffer = buffers.find(b => b.id === activeBufferId);
    if (buffer) {
      updateBufferCode(activeBufferId, buffer.code + `\nsample :${name}\n`);
    }
  };

  return (
    <div className="side-panel sample-browser">
      <div className="panel-header">
        <h3><FaFolder /> Samples</h3>
        <button className="close-btn" onClick={toggleSampleBrowser}>
          <FaTimes />
        </button>
      </div>
      <div className="panel-content">
        <div className="synth-filter">
          <input
            type="text"
            placeholder="Filter samples..."
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="synth-filter-input"
          />
        </div>
        {Object.keys(grouped).length === 0 && (
          <div className="empty-state">
            {samples.length === 0 ? (
              <>
                <p>No samples found.</p>
                <p className="hint">Samples will be generated on first run.</p>
              </>
            ) : (
              <p>No samples match your filter.</p>
            )}
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
                {items.map((sample) => (
                  <div key={sample.path} className="sample-item">
                    <span className="sample-name">{sample.name}</span>
                    <div className="sample-actions">
                      <button
                        className="sample-play-btn"
                        onClick={() => playSampleFile(sample.path)}
                        title="Preview"
                      >
                        <FaPlay />
                      </button>
                      <button
                        className="sample-insert-btn"
                        onClick={() => insertSample(sample.name)}
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

export default SampleBrowser;
