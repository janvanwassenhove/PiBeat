import React, { useEffect, useState, useMemo, useRef, useCallback } from 'react';
import { useStore, UserSampleInfo } from '../store';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import {
  FaPlay,
  FaStop,
  FaFolderOpen,
  FaSync,
  FaChevronRight,
  FaChevronDown,
  FaSearch,
  FaClock,
  FaMusic,
  FaTags,
  FaPlus,
  FaDrum,
  FaMicrophone,
  FaGuitar,
  FaWater,
  FaMagic,
  FaRedoAlt,
  FaBolt,
  FaQuestionCircle,
  FaFolder,
} from 'react-icons/fa';
import DetachablePanel from './DetachablePanel';

// ── Mini Waveform Component ──
const SampleWaveform: React.FC<{
  peaks: number[];
  progress: number; // 0..1
  isPlaying: boolean;
  onClick?: (ratio: number) => void;
}> = React.memo(({ peaks, progress, isPlaying, onClick }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || peaks.length === 0) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    ctx.scale(dpr, dpr);

    ctx.clearRect(0, 0, w, h);
    const barWidth = w / peaks.length;
    const mid = h / 2;

    for (let i = 0; i < peaks.length; i++) {
      const x = i * barWidth;
      const amp = peaks[i] * mid * 0.9;
      const isPast = isPlaying && (i / peaks.length) < progress;
      ctx.fillStyle = isPast ? '#4488ff' : 'rgba(255,255,255,0.25)';
      ctx.fillRect(x, mid - amp, Math.max(barWidth - 0.5, 1), amp * 2 || 1);
    }

    // Draw playhead
    if (isPlaying && progress > 0 && progress < 1) {
      const px = progress * w;
      ctx.strokeStyle = '#4488ff';
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(px, 0);
      ctx.lineTo(px, h);
      ctx.stroke();
    }
  }, [peaks, progress, isPlaying]);

  const handleClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!onClick) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = (e.clientX - rect.left) / rect.width;
    onClick(Math.max(0, Math.min(1, ratio)));
  };

  return (
    <canvas
      ref={canvasRef}
      className="user-sample-waveform-canvas"
      onClick={handleClick}
    />
  );
});

type GroupBy = 'folder' | 'type' | 'feeling' | 'tag';
type SortBy = 'name' | 'duration' | 'bpm' | 'type';

const UserSamplePanel: React.FC = () => {
  const {
    userSamples,
    userSamplesDir,
    userSamplesLoading,
    userSamplesScanProgress,
    showUserSamplePanel,
    toggleUserSamplePanel,
    setUserSamplesDir,
    scanUserSamples,
    fullRescanUserSamples,
    playSampleFile,
    stopAudio,
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

  // Playing state
  const [playingSample, setPlayingSample] = useState<string | null>(null);
  const [playProgress, setPlayProgress] = useState(0);
  const playTimerRef = useRef<number | null>(null);
  const playStartRef = useRef<number>(0);

  // Waveform peaks cache
  const [peaksCache, setPeaksCache] = useState<Record<string, number[]>>({});

  // Fetch peaks when sample is expanded or visible
  const fetchPeaks = useCallback(async (path: string) => {
    if (peaksCache[path]) return;
    try {
      const peaks = await invoke<number[]>('get_sample_peaks', { path, numPeaks: 100 });
      setPeaksCache((prev) => ({ ...prev, [path]: peaks }));
    } catch (e) {
      console.error('[UserSamplePanel] Failed to get peaks:', e);
    }
  }, [peaksCache]);

  // Auto-fetch peaks for expanded sample
  useEffect(() => {
    if (expandedSample) {
      fetchPeaks(expandedSample);
    }
  }, [expandedSample]);

  // Cleanup play timer on unmount
  useEffect(() => {
    return () => {
      if (playTimerRef.current) cancelAnimationFrame(playTimerRef.current);
    };
  }, []);

  // Play/pause toggle
  const handlePlayPause = useCallback(async (sample: UserSampleInfo) => {
    if (playingSample === sample.path) {
      // Stop playing
      await stopAudio();
      setPlayingSample(null);
      setPlayProgress(0);
      if (playTimerRef.current) {
        cancelAnimationFrame(playTimerRef.current);
        playTimerRef.current = null;
      }
    } else {
      // Stop previous if any
      if (playingSample) {
        await stopAudio();
        if (playTimerRef.current) {
          cancelAnimationFrame(playTimerRef.current);
          playTimerRef.current = null;
        }
      }
      // Start playing new
      await playSampleFile(sample.path);
      setPlayingSample(sample.path);
      setPlayProgress(0);
      playStartRef.current = performance.now();
      const durationMs = sample.duration_secs * 1000;

      // Fetch peaks if not cached
      fetchPeaks(sample.path);

      // Animate progress
      const animate = () => {
        const elapsed = performance.now() - playStartRef.current;
        const prog = Math.min(elapsed / durationMs, 1);
        setPlayProgress(prog);
        if (prog < 1) {
          playTimerRef.current = requestAnimationFrame(animate);
        } else {
          setPlayingSample(null);
          setPlayProgress(0);
          playTimerRef.current = null;
        }
      };
      playTimerRef.current = requestAnimationFrame(animate);
    }
  }, [playingSample, playSampleFile, stopAudio, fetchPeaks]);

  // On mount / panel open: scan only if we have no cached data yet
  const hasScannedRef = useRef(false);
  useEffect(() => {
    if (showUserSamplePanel && userSamplesDir && !hasScannedRef.current) {
      hasScannedRef.current = true;
      // Background scan to detect new/changed/removed files.
      // If everything is cached, this completes silently without loading UI.
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

  const typeIcon: Record<string, React.ReactNode> = {
    drums: <FaDrum />,
    vocal: <FaMicrophone />,
    instrumental: <FaGuitar />,
    bass: <FaMusic />,
    pad: <FaWater />,
    fx: <FaMagic />,
    loop: <FaRedoAlt />,
    'one-shot': <FaBolt />,
    unknown: <FaQuestionCircle />,
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
    <DetachablePanel
      panelId="userSamplePanel"
      title="My Samples"
      icon={<FaFolderOpen />}
      onClose={toggleUserSamplePanel}
      className="user-sample-panel"
      defaultWidth={350}
      defaultHeight={550}
    >
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
              title="Scan for changes"
            >
              <FaSync className={userSamplesLoading ? 'spinning' : ''} />
            </button>
          )}
          {userSamplesDir && (
            <button
              className="user-sample-rescan-btn user-sample-fullrescan-btn"
              onClick={fullRescanUserSamples}
              disabled={userSamplesLoading}
              title="Full rescan — clear cache and re-analyze all files"
            >
              <FaSync className={userSamplesLoading ? 'spinning' : ''} /> All
            </button>
          )}
        </div>

        {!userSamplesDir && (
          <div className="empty-state">
            <p>Select a folder to scan for audio samples.</p>
            <p className="hint">Supports WAV and MP3 files.</p>
          </div>
        )}

        {userSamplesLoading && userSamplesScanProgress && (
          <div className="user-sample-progress">
            <div className="user-sample-progress-header">
              <FaSync className="spinning" />
              <span>
                {userSamplesScanProgress.phase === 'discovering'
                  ? 'Discovering files...'
                  : `Analyzing ${userSamplesScanProgress.scanned} / ${userSamplesScanProgress.total} samples`}
              </span>
            </div>
            {userSamplesScanProgress.total > 0 && (
              <div className="user-sample-progress-bar">
                <div
                  className="user-sample-progress-fill"
                  style={{
                    width: `${Math.round((userSamplesScanProgress.scanned / userSamplesScanProgress.total) * 100)}%`,
                  }}
                />
              </div>
            )}
            {userSamplesScanProgress.phase === 'analyzing' && userSamplesScanProgress.total > 0 && (
              <div className="user-sample-progress-pct">
                {Math.round((userSamplesScanProgress.scanned / userSamplesScanProgress.total) * 100)}%
              </div>
            )}
          </div>
        )}

        {userSamplesLoading && !userSamplesScanProgress && (
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
                      {collapsedGroups[group] === false ? (
                        <FaChevronDown />
                      ) : (
                        <FaChevronRight />
                      )}
                    </span>
                    <span className="category-icon">{typeIcon[group] || <FaFolder />}</span>
                    <span>{group}</span>
                    <span className="category-count">{items.length}</span>
                  </h4>
                  {collapsedGroups[group] === false && (
                    <div className="sample-list">
                      {items.map((sample) => {
                        const isAnalyzed = sample.duration_secs > 0 || sample.audio_type !== 'unknown';
                        const isCurrPlaying = playingSample === sample.path;
                        const peaks = peaksCache[sample.path];
                        return (
                        <div
                          key={sample.path}
                          className={`user-sample-item${!isAnalyzed ? ' user-sample-pending' : ''}${isCurrPlaying ? ' user-sample-playing' : ''}`}
                        >
                          <div className="user-sample-item-main">
                            <button
                              className={`sample-play-btn user-sample-play-toggle${isCurrPlaying ? ' playing' : ''}`}
                              onClick={() => handlePlayPause(sample)}
                              title={isCurrPlaying ? 'Stop' : 'Play'}
                            >
                              {isCurrPlaying ? <FaStop /> : <FaPlay />}
                            </button>
                            <div className="user-sample-item-info">
                              <span className="sample-name">{sample.name}</span>
                              {/* Inline mini waveform */}
                              {isAnalyzed && peaks && (
                                <div className="user-sample-waveform-row">
                                  <SampleWaveform
                                    peaks={peaks}
                                    progress={isCurrPlaying ? playProgress : 0}
                                    isPlaying={isCurrPlaying}
                                  />
                                </div>
                              )}
                              <div className="user-sample-meta">
                                {isAnalyzed ? (
                                  <>
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
                                  </>
                                ) : (
                                  <span className="user-sample-meta-item user-sample-analyzing">
                                    analyzing…
                                  </span>
                                )}</div></div>
                            <div className="sample-actions user-sample-actions-visible">
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
                              {/* Large waveform */}
                              {peaks && (
                                <div className="user-sample-waveform-large">
                                  <SampleWaveform
                                    peaks={peaks}
                                    progress={isCurrPlaying ? playProgress : 0}
                                    isPlaying={isCurrPlaying}
                                  />
                                </div>
                              )}
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
                      );})}
                    </div>
                  )}
                </div>
              ))}
          </>
        )}
      </div>
    </DetachablePanel>
  );
};

export default UserSamplePanel;
