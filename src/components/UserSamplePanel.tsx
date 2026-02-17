import React, { useEffect, useState, useMemo } from 'react';
import { useStore, UserSampleInfo } from '../store';
import { open } from '@tauri-apps/plugin-dialog';
import {
  FaPlay,
  FaTimes,
  FaFolderOpen,
  FaSync,
  FaChevronRight,
  FaChevronDown,
  FaSearch,
  FaClock,
  FaMusic,
  FaTags,
  FaPlus,
} from 'react-icons/fa';

type GroupBy = 'folder' | 'type' | 'feeling' | 'tag';
type SortBy = 'name' | 'duration' | 'bpm' | 'type';

const UserSamplePanel: React.FC = () => {
  const {
    userSamples,
    userSamplesDir,
    userSamplesLoading,
    showUserSamplePanel,
    toggleUserSamplePanel,
    setUserSamplesDir,
    scanUserSamples,
    playSampleFile,
    updateBufferCode,
    buffers,
    activeBufferId,
  } = useStore();

  const [filter, setFilter] = useState('');
  const [groupBy, setGroupBy] = useState<GroupBy>('type');
  const [sortBy, setSortBy] = useState<SortBy>('name');
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [expandedSample, setExpandedSample] = useState<string | null>(null);

  // Load saved dir on mount
  useEffect(() => {
    if (showUserSamplePanel && userSamplesDir && userSamples.length === 0) {
      scanUserSamples();
    }
  }, [showUserSamplePanel]);

  // Get all unique tags
  const allTags = useMemo(() => {
    const tagSet = new Set<string>();
    userSamples.forEach((s) => s.tags.forEach((t) => tagSet.add(t)));
    return Array.from(tagSet).sort();
  }, [userSamples]);

  // Filter samples
  const filtered = useMemo(() => {
    let result = userSamples;

    if (filter) {
      const q = filter.toLowerCase();
      result = result.filter(
        (s) =>
          s.name.toLowerCase().includes(q) ||
          s.audio_type.toLowerCase().includes(q) ||
          s.feeling.toLowerCase().includes(q) ||
          s.folder.toLowerCase().includes(q) ||
          s.tags.some((t) => t.toLowerCase().includes(q))
      );
    }

    if (selectedTags.length > 0) {
      result = result.filter((s) => selectedTags.every((t) => s.tags.includes(t)));
    }

    // Sort
    result = [...result].sort((a, b) => {
      switch (sortBy) {
        case 'name':
          return a.name.localeCompare(b.name);
        case 'duration':
          return a.duration_secs - b.duration_secs;
        case 'bpm':
          return (a.bpm_estimate ?? 0) - (b.bpm_estimate ?? 0);
        case 'type':
          return a.audio_type.localeCompare(b.audio_type);
        default:
          return 0;
      }
    });

    return result;
  }, [userSamples, filter, selectedTags, sortBy]);

  // Group samples
  const grouped = useMemo(() => {
    const groups: Record<string, UserSampleInfo[]> = {};
    for (const s of filtered) {
      let key: string;
      switch (groupBy) {
        case 'folder':
          key = s.folder || '(root)';
          break;
        case 'type':
          key = s.audio_type;
          break;
        case 'feeling':
          key = s.feeling;
          break;
        case 'tag':
          if (s.tags.length === 0) {
            key = '(untagged)';
            if (!groups[key]) groups[key] = [];
            groups[key].push(s);
          } else {
            for (const t of s.tags) {
              if (!groups[t]) groups[t] = [];
              groups[t].push(s);
            }
          }
          continue;
        default:
          key = s.audio_type;
      }
      if (!groups[key]) groups[key] = [];
      groups[key].push(s);
    }
    return groups;
  }, [filtered, groupBy]);

  if (!showUserSamplePanel) return null;

  const handleSelectFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Sample Folder',
      });
      if (selected && typeof selected === 'string') {
        await setUserSamplesDir(selected);
      }
    } catch (e) {
      console.error('[UserSamplePanel] Folder selection error:', e);
    }
  };

  const toggleGroup = (group: string) => {
    setCollapsedGroups((prev) => ({ ...prev, [group]: !prev[group] }));
  };

  const toggleTag = (tag: string) => {
    setSelectedTags((prev) =>
      prev.includes(tag) ? prev.filter((t) => t !== tag) : [...prev, tag]
    );
  };

  const insertSample = (sample: UserSampleInfo) => {
    const buffer = buffers.find((b) => b.id === activeBufferId);
    if (buffer) {
      // Use the full path for user samples since they're external files
      const escapedPath = sample.path.replace(/\\/g, '/');
      updateBufferCode(
        activeBufferId,
        buffer.code + `\nsample "${escapedPath}"\n`
      );
    }
  };

  const formatDuration = (secs: number): string => {
    if (secs < 1) return `${Math.round(secs * 1000)}ms`;
    if (secs < 60) return `${secs.toFixed(1)}s`;
    const m = Math.floor(secs / 60);
    const s = Math.round(secs % 60);
    return `${m}:${s.toString().padStart(2, '0')}`;
  };

  const typeEmoji: Record<string, string> = {
    drums: '🥁',
    vocal: '🎤',
    instrumental: '🎸',
    bass: '🎵',
    pad: '🌊',
    fx: '✨',
    loop: '🔁',
    'one-shot': '💥',
    unknown: '❓',
  };

  const feelingColor: Record<string, string> = {
    energetic: '#ff5555',
    calm: '#55aaff',
    dark: '#8855cc',
    bright: '#ffcc00',
    aggressive: '#ff3300',
    mellow: '#88cc66',
    neutral: '#888888',
  };

  return (
    <div className="side-panel user-sample-panel">
      <div className="panel-header">
        <h3>
          <FaFolderOpen /> My Samples
        </h3>
        <button className="close-btn" onClick={toggleUserSamplePanel}>
          <FaTimes />
        </button>
      </div>

      <div className="panel-content">
        {/* Folder selection */}
        <div className="user-sample-folder-section">
          <button className="user-sample-folder-btn" onClick={handleSelectFolder}>
            <FaFolderOpen /> {userSamplesDir ? 'Change Folder' : 'Select Folder'}
          </button>
          {userSamplesDir && (
            <div className="user-sample-folder-path" title={userSamplesDir}>
              {userSamplesDir.split(/[/\\]/).pop() || userSamplesDir}
            </div>
          )}
          {userSamplesDir && (
            <button
              className="user-sample-rescan-btn"
              onClick={scanUserSamples}
              disabled={userSamplesLoading}
              title="Rescan folder"
            >
              <FaSync className={userSamplesLoading ? 'spinning' : ''} />
            </button>
          )}
        </div>

        {!userSamplesDir && (
          <div className="empty-state">
            <p>Select a folder to scan for audio samples.</p>
            <p className="hint">Supports WAV and MP3 files.</p>
          </div>
        )}

        {userSamplesLoading && (
          <div className="user-sample-loading">
            <FaSync className="spinning" /> Scanning and analyzing samples...
          </div>
        )}

        {userSamplesDir && !userSamplesLoading && userSamples.length === 0 && (
          <div className="empty-state">
            <p>No audio files found in this folder.</p>
            <p className="hint">Add WAV or MP3 files and rescan.</p>
          </div>
        )}

        {userSamples.length > 0 && (
          <>
            {/* Search and controls */}
            <div className="user-sample-controls">
              <div className="user-sample-search">
                <FaSearch className="search-icon" />
                <input
                  type="text"
                  placeholder="Search samples..."
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                  className="synth-filter-input"
                />
              </div>
              <div className="user-sample-toolbar">
                <div className="user-sample-control-group">
                  <label>Group:</label>
                  <select
                    value={groupBy}
                    onChange={(e) => setGroupBy(e.target.value as GroupBy)}
                  >
                    <option value="type">Type</option>
                    <option value="folder">Folder</option>
                    <option value="feeling">Mood</option>
                    <option value="tag">Tag</option>
                  </select>
                </div>
                <div className="user-sample-control-group">
                  <label>Sort:</label>
                  <select
                    value={sortBy}
                    onChange={(e) => setSortBy(e.target.value as SortBy)}
                  >
                    <option value="name">Name</option>
                    <option value="duration">Duration</option>
                    <option value="bpm">BPM</option>
                    <option value="type">Type</option>
                  </select>
                </div>
              </div>

              {/* Tag filters */}
              {allTags.length > 0 && (
                <div className="user-sample-tag-filters">
                  {allTags.slice(0, 20).map((tag) => (
                    <button
                      key={tag}
                      className={`user-sample-tag-btn ${selectedTags.includes(tag) ? 'active' : ''}`}
                      onClick={() => toggleTag(tag)}
                    >
                      {tag}
                    </button>
                  ))}
                  {selectedTags.length > 0 && (
                    <button
                      className="user-sample-tag-btn clear-tags"
                      onClick={() => setSelectedTags([])}
                    >
                      ✕ Clear
                    </button>
                  )}
                </div>
              )}

              <div className="user-sample-count">
                {filtered.length} / {userSamples.length} samples
              </div>
            </div>

            {/* Sample list */}
            {Object.entries(grouped)
              .sort(([a], [b]) => a.localeCompare(b))
              .map(([group, items]) => (
                <div key={group} className="sample-category">
                  <h4
                    className="category-title"
                    onClick={() => toggleGroup(group)}
                  >
                    <span className="category-chevron">
                      {collapsedGroups[group] ? (
                        <FaChevronRight />
                      ) : (
                        <FaChevronDown />
                      )}
                    </span>
                    <span>{typeEmoji[group] || '📁'} {group}</span>
                    <span className="category-count">{items.length}</span>
                  </h4>
                  {!collapsedGroups[group] && (
                    <div className="sample-list">
                      {items.map((sample) => (
                        <div
                          key={sample.path}
                          className="user-sample-item"
                        >
                          <div className="user-sample-item-main">
                            <div className="user-sample-item-info">
                              <span className="sample-name">{sample.name}</span>
                              <div className="user-sample-meta">
                                <span className="user-sample-meta-item" title="Duration">
                                  <FaClock /> {formatDuration(sample.duration_secs)}
                                </span>
                                {sample.bpm_estimate && (
                                  <span className="user-sample-meta-item" title="Estimated BPM">
                                    <FaMusic /> {Math.round(sample.bpm_estimate)} BPM
                                  </span>
                                )}
                                <span
                                  className="user-sample-meta-item user-sample-feeling"
                                  style={{ color: feelingColor[sample.feeling] || '#888' }}
                                  title={`Mood: ${sample.feeling}`}
                                >
                                  {sample.feeling}
                                </span>
                              </div>
                            </div>
                            <div className="sample-actions user-sample-actions-visible">
                              <button
                                className="sample-play-btn"
                                onClick={() => playSampleFile(sample.path)}
                                title="Preview"
                              >
                                <FaPlay />
                              </button>
                              <button
                                className="sample-insert-btn"
                                onClick={() => insertSample(sample)}
                                title="Insert into code"
                              >
                                <FaPlus />
                              </button>
                              <button
                                className="sample-insert-btn"
                                onClick={() =>
                                  setExpandedSample(
                                    expandedSample === sample.path ? null : sample.path
                                  )
                                }
                                title="Details"
                              >
                                <FaTags />
                              </button>
                            </div>
                          </div>
                          {expandedSample === sample.path && (
                            <div className="user-sample-details">
                              <div className="user-sample-detail-row">
                                <span className="detail-label">Type:</span>
                                <span>{sample.audio_type}</span>
                              </div>
                              <div className="user-sample-detail-row">
                                <span className="detail-label">Feeling:</span>
                                <span style={{ color: feelingColor[sample.feeling] }}>
                                  {sample.feeling}
                                </span>
                              </div>
                              <div className="user-sample-detail-row">
                                <span className="detail-label">Duration:</span>
                                <span>{formatDuration(sample.duration_secs)}</span>
                              </div>
                              {sample.bpm_estimate && (
                                <div className="user-sample-detail-row">
                                  <span className="detail-label">BPM:</span>
                                  <span>{Math.round(sample.bpm_estimate)}</span>
                                </div>
                              )}
                              <div className="user-sample-detail-row">
                                <span className="detail-label">Format:</span>
                                <span>{sample.file_type.toUpperCase()} · {sample.sample_rate}Hz</span>
                              </div>
                              <div className="user-sample-detail-row">
                                <span className="detail-label">Path:</span>
                                <span className="detail-path" title={sample.path}>
                                  {sample.folder ? `${sample.folder}/` : ''}{sample.name}.{sample.file_type}
                                </span>
                              </div>
                              <div className="user-sample-tags-detail">
                                {sample.tags.map((tag) => (
                                  <span key={tag} className="user-sample-tag">
                                    {tag}
                                  </span>
                                ))}
                              </div>
                            </div>
                          )}
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              ))}
          </>
        )}
      </div>
    </div>
  );
};

export default UserSamplePanel;
