import React, { useMemo, useState, useRef, useCallback, useEffect } from 'react';
import { useStore } from '../store';
import { parseCodeToTimeline, extractCodeSampleNames, TimelineClip, TimelineTrack, ClipEffect, TimelineData, SectionMarker } from '../timelineParser';
import {
  applyClipAmpChange,
  applyTrackAmpChange,
  applyClipStartChange,
  applyClipDurationChange,
  applyAddEffect,
  applyRemoveEffect,
  applyUpdateEffectParams,
  applyClipMute,
} from '../timelineSync';

// ─── Clip Tooltip ────────────────────────────────────────────────
const ClipTooltip: React.FC<{
  clip: TimelineClip;
  x: number;
  y: number;
}> = ({ clip, x, y }) => {
  return (
    <div
      className="clip-tooltip"
      style={{
        left: Math.min(x, window.innerWidth - 420),
        top: y + 16,
      }}
    >
      <div className="clip-tooltip-header">
        <span className="clip-tooltip-name">{clip.name}</span>
        <span className="clip-tooltip-type">{clip.type}</span>
      </div>
      <div className="clip-tooltip-meta">
        <span>Start: beat {clip.startBeat.toFixed(1)}</span>
        <span>Duration: {clip.durationBeats.toFixed(1)} beats</span>
        {clip.audioTailBeats > 0.05 && (
          <span>Audio tail: +{clip.audioTailBeats.toFixed(1)} beats</span>
        )}
        <span>Amp: {clip.amp.toFixed(2)}</span>
        {clip.isLooping && <span className="clip-tooltip-loop">⟳ Looping</span>}
        {clip.needsPreload && <span className="clip-tooltip-preload">Needs preload</span>}
        {clip.samples.length > 0 && (
          <span>Samples: {clip.samples.join(', ')}</span>
        )}
        {Object.keys(clip.sampleDurationMap).length > 0 && (
          <span>Sample durations: {Object.entries(clip.sampleDurationMap).map(([n, d]) => `${n}: ${d.toFixed(2)}s`).join(', ')}</span>
        )}
        {clip.effects.length > 0 && (
          <span>FX: {clip.effects.map(e => e.type).join(', ')}</span>
        )}
      </div>
      <pre className="clip-tooltip-code">{clip.code}</pre>
    </div>
  );
};

// ─── Track Effects Control ───────────────────────────────────────
const TrackEffectsPopover: React.FC<{
  track: TimelineTrack;
  onUpdateTrack: (trackId: string, updates: Partial<TimelineTrack>) => void;
  onClose: () => void;
}> = ({ track, onUpdateTrack, onClose }) => {
  const [newFxType, setNewFxType] = useState('reverb');
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });

  const FX_OPTIONS = ['reverb', 'echo', 'delay', 'lpf', 'hpf', 'distortion', 'flanger', 'compressor', 'bitcrusher'];

  const handleDragStart = (e: React.MouseEvent) => {
    setIsDragging(true);
    setDragStart({ x: e.clientX - position.x, y: e.clientY - position.y });
  };

  useEffect(() => {
    if (!isDragging) return;

    const handleDrag = (e: MouseEvent) => {
      setPosition({ x: e.clientX - dragStart.x, y: e.clientY - dragStart.y });
    };

    const handleDragEnd = () => {
      setIsDragging(false);
    };

    document.addEventListener('mousemove', handleDrag);
    document.addEventListener('mouseup', handleDragEnd);

    return () => {
      document.removeEventListener('mousemove', handleDrag);
      document.removeEventListener('mouseup', handleDragEnd);
    };
  }, [isDragging, dragStart]);

  const addEffect = () => {
    const newEffect: ClipEffect = {
      type: newFxType,
      params: getDefaultParams(newFxType),
    };
    onUpdateTrack(track.id, {
      effects: [...track.effects, newEffect],
    });
  };

  const removeEffect = (idx: number) => {
    onUpdateTrack(track.id, {
      effects: track.effects.filter((_, i) => i !== idx),
    });
  };

  const updateEffectParam = (idx: number, param: string, value: number) => {
    const updated = [...track.effects];
    updated[idx] = { ...updated[idx], params: { ...updated[idx].params, [param]: value } };
    onUpdateTrack(track.id, { effects: updated });
  };

  return (
    <div 
      className="track-effects-popover" 
      onClick={(e) => e.stopPropagation()}
      style={{ transform: `translate(${position.x}px, ${position.y}px)` }}
    >
      <div 
        className="track-effects-header"
        onMouseDown={handleDragStart}
        style={{ cursor: isDragging ? 'grabbing' : 'grab' }}
      >
        <span>Track Effects: {track.name}</span>
        <button className="track-effects-close" onClick={onClose}>✕</button>
      </div>

      <div className="track-effects-amp">
        <label>Amplitude</label>
        <input
          type="range"
          min="0"
          max="2"
          step="0.05"
          value={track.amp}
          onChange={(e) => onUpdateTrack(track.id, { amp: parseFloat(e.target.value) })}
        />
        <span>{track.amp.toFixed(2)}</span>
      </div>

      <div className="track-effects-list">
        {track.effects.map((fx, i) => (
          <div key={i} className="track-effect-item">
            <div className="track-effect-header">
              <span className="track-effect-name">{fx.type}</span>
              <button className="track-effect-remove" onClick={() => removeEffect(i)}>✕</button>
            </div>
            {Object.entries(fx.params).map(([param, value]) => (
              <div key={param} className="track-effect-param">
                <label>{param}</label>
                <input
                  type="range"
                  min={getParamMin(param)}
                  max={getParamMax(param)}
                  step={getParamStep(param)}
                  value={value}
                  onChange={(e) => updateEffectParam(i, param, parseFloat(e.target.value))}
                />
                <span>{value.toFixed(2)}</span>
              </div>
            ))}
          </div>
        ))}
      </div>

      <div className="track-effects-add">
        <select value={newFxType} onChange={(e) => setNewFxType(e.target.value)}>
          {FX_OPTIONS.map(fx => (
            <option key={fx} value={fx}>{fx}</option>
          ))}
        </select>
        <button onClick={addEffect}>+ Add FX</button>
      </div>
    </div>
  );
};

// ─── Clip Effects Popover ────────────────────────────────────────
const ClipEffectsPopover: React.FC<{
  clip: TimelineClip;
  trackId: string;
  onUpdateClip: (trackId: string, clipId: string, updates: Partial<TimelineClip>) => void;
  onClose: () => void;
}> = ({ clip, trackId, onUpdateClip, onClose }) => {
  const [newFxType, setNewFxType] = useState('reverb');
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });

  const FX_OPTIONS = ['reverb', 'echo', 'delay', 'lpf', 'hpf', 'distortion', 'flanger', 'compressor'];

  const handleDragStart = (e: React.MouseEvent) => {
    setIsDragging(true);
    setDragStart({ x: e.clientX - position.x, y: e.clientY - position.y });
  };

  useEffect(() => {
    if (!isDragging) return;

    const handleDrag = (e: MouseEvent) => {
      setPosition({ x: e.clientX - dragStart.x, y: e.clientY - dragStart.y });
    };

    const handleDragEnd = () => {
      setIsDragging(false);
    };

    document.addEventListener('mousemove', handleDrag);
    document.addEventListener('mouseup', handleDragEnd);

    return () => {
      document.removeEventListener('mousemove', handleDrag);
      document.removeEventListener('mouseup', handleDragEnd);
    };
  }, [isDragging, dragStart]);

  const addEffect = () => {
    const newEffect: ClipEffect = {
      type: newFxType,
      params: getDefaultParams(newFxType),
    };
    onUpdateClip(trackId, clip.id, {
      effects: [...clip.effects, newEffect],
    });
  };

  const removeEffect = (idx: number) => {
    onUpdateClip(trackId, clip.id, {
      effects: clip.effects.filter((_, i) => i !== idx),
    });
  };

  const updateEffectParam = (idx: number, param: string, value: number) => {
    const updated = [...clip.effects];
    updated[idx] = { ...updated[idx], params: { ...updated[idx].params, [param]: value } };
    onUpdateClip(trackId, clip.id, { effects: updated });
  };

  return (
    <div 
      className="clip-effects-popover" 
      onClick={(e) => e.stopPropagation()}
      style={{ transform: `translate(${position.x}px, ${position.y}px)` }}
    >
      <div 
        className="track-effects-header"
        onMouseDown={handleDragStart}
        style={{ cursor: isDragging ? 'grabbing' : 'grab' }}
      >
        <span>Clip: {clip.name}</span>
        <button className="track-effects-close" onClick={onClose}>✕</button>
      </div>

      <div className="track-effects-amp">
        <label>Clip Amp</label>
        <input
          type="range"
          min="0"
          max="3"
          step="0.05"
          value={clip.amp}
          onChange={(e) => onUpdateClip(trackId, clip.id, { amp: parseFloat(e.target.value) })}
        />
        <span>{clip.amp.toFixed(2)}</span>
      </div>

      <div className="track-effects-list">
        {clip.effects.map((fx, i) => (
          <div key={i} className="track-effect-item">
            <div className="track-effect-header">
              <span className="track-effect-name">{fx.type}</span>
              <button className="track-effect-remove" onClick={() => removeEffect(i)}>✕</button>
            </div>
            {Object.entries(fx.params).map(([param, value]) => (
              <div key={param} className="track-effect-param">
                <label>{param}</label>
                <input
                  type="range"
                  min={getParamMin(param)}
                  max={getParamMax(param)}
                  step={getParamStep(param)}
                  value={value}
                  onChange={(e) => updateEffectParam(i, param, parseFloat(e.target.value))}
                />
                <span>{value.toFixed(2)}</span>
              </div>
            ))}
          </div>
        ))}
      </div>

      <div className="track-effects-add">
        <select value={newFxType} onChange={(e) => setNewFxType(e.target.value)}>
          {FX_OPTIONS.map(fx => (
            <option key={fx} value={fx}>{fx}</option>
          ))}
        </select>
        <button onClick={addEffect}>+ Add FX</button>
      </div>
    </div>
  );
};

// ─── Helpers for FX params ───────────────────────────────────────

function getDefaultParams(fxType: string): Record<string, number> {
  switch (fxType) {
    case 'reverb': return { mix: 0.4, room: 0.6 };
    case 'echo': return { phase: 0.25, decay: 2 };
    case 'delay': return { time: 0.5, feedback: 0.3 };
    case 'lpf': return { cutoff: 100 };
    case 'hpf': return { cutoff: 50 };
    case 'distortion': return { distort: 0.5 };
    case 'flanger': return { depth: 5, rate: 0.5 };
    case 'compressor': return { threshold: 0.5, ratio: 4 };
    case 'bitcrusher': return { bits: 8, rate: 1 };
    default: return { mix: 0.5 };
  }
}

function getParamMin(param: string): number {
  if (param === 'cutoff') return 20;
  if (param === 'bits') return 1;
  return 0;
}

function getParamMax(param: string): number {
  if (param === 'cutoff') return 130;
  if (param === 'room') return 1;
  if (param === 'mix') return 1;
  if (param === 'decay') return 10;
  if (param === 'feedback') return 1;
  if (param === 'ratio') return 20;
  if (param === 'bits') return 16;
  if (param === 'depth') return 20;
  if (param === 'distort') return 1;
  if (param === 'threshold') return 1;
  return 10;
}

function getParamStep(param: string): number {
  if (param === 'bits') return 1;
  if (param === 'cutoff') return 1;
  return 0.01;
}

// ─── Time Ruler ──────────────────────────────────────────────────

const TimeRuler: React.FC<{
  totalBeats: number;
  pixelsPerBeat: number;
  bpm: number;
  scrollLeft: number;
  sections: SectionMarker[];
}> = ({ totalBeats, pixelsPerBeat, bpm, sections }) => {
  const marks: React.ReactElement[] = [];
  const beatsPerBar = 4;
  const secondsPerBeat = 60 / bpm;

  for (let beat = 0; beat <= totalBeats; beat++) {
    const x = beat * pixelsPerBeat;
    const isBar = beat % beatsPerBar === 0;
    const barNum = Math.floor(beat / beatsPerBar) + 1;
    const timeSeconds = beat * secondsPerBeat;
    const mins = Math.floor(timeSeconds / 60);
    const secs = Math.floor(timeSeconds % 60);

    if (isBar) {
      marks.push(
        <div key={`bar-${beat}`} className="ruler-mark ruler-bar" style={{ left: x }}>
          <span className="ruler-bar-num">{barNum}</span>
          <span className="ruler-time">{mins}:{secs.toString().padStart(2, '0')}</span>
        </div>
      );
    } else if (pixelsPerBeat > 20) {
      marks.push(
        <div key={`beat-${beat}`} className="ruler-mark ruler-beat" style={{ left: x }} />
      );
    }
  }

  // Section markers
  const sectionMarks = sections.map((sec, i) => (
    <div
      key={`sec-${i}`}
      className="ruler-section-marker"
      style={{ left: sec.beatStart * pixelsPerBeat }}
    >
      <span className="ruler-section-label">{sec.label}</span>
      <div className="ruler-section-line" />
    </div>
  ));

  return (
    <div className="time-ruler" style={{ width: totalBeats * pixelsPerBeat }}>
      {marks}
      {sectionMarks}
    </div>
  );
};

// ─── Single Clip Component ───────────────────────────────────────

const ClipComponent: React.FC<{
  clip: TimelineClip;
  trackId: string;
  pixelsPerBeat: number;
  totalBeats: number;
  onUpdateClip: (trackId: string, clipId: string, updates: Partial<TimelineClip>) => void;
  onDragEnd: (trackId: string, clipId: string, newStart: number, oldStart: number) => void;
  onResizeEnd: (trackId: string, clipId: string, newDur: number, oldDur: number) => void;
}> = ({ clip, trackId, pixelsPerBeat, totalBeats, onUpdateClip, onDragEnd, onResizeEnd }) => {
  const [hovering, setHovering] = useState(false);
  const [tooltipPos, setTooltipPos] = useState({ x: 0, y: 0 });
  const [showEffects, setShowEffects] = useState(false);
  const clipRef = useRef<HTMLDivElement>(null);
  const dragState = useRef<{ type: 'move' | 'resize'; startX: number; origStart: number; origDuration: number } | null>(null);

  const handleMouseEnter = (e: React.MouseEvent) => {
    if (!dragState.current) { setHovering(true); setTooltipPos({ x: e.clientX, y: e.clientY }); }
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    setTooltipPos({ x: e.clientX, y: e.clientY });
  };

  const handleMouseLeave = () => {
    setHovering(false);
  };

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    setShowEffects(!showEffects);
  };

  // Drag to move
  const handleDragStart = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const origStart = clip.startBeat;
    dragState.current = { type: 'move', startX: e.clientX, origStart, origDuration: clip.durationBeats };
    setHovering(false);

    let lastNewStart = origStart;
    const onMove = (ev: MouseEvent) => {
      if (!dragState.current) return;
      const dx = ev.clientX - dragState.current.startX;
      const dBeats = dx / pixelsPerBeat;
      lastNewStart = Math.max(0, Math.round((dragState.current.origStart + dBeats) * 4) / 4);
      onUpdateClip(trackId, clip.id, { startBeat: lastNewStart });
    };
    const onUp = () => {
      if (lastNewStart !== origStart) {
        onDragEnd(trackId, clip.id, lastNewStart, origStart);
      }
      dragState.current = null;
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  };

  // Resize handle
  const handleResizeStart = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const origDuration = clip.durationBeats;
    dragState.current = { type: 'resize', startX: e.clientX, origStart: clip.startBeat, origDuration };
    setHovering(false);

    let lastNewDur = origDuration;
    const onMove = (ev: MouseEvent) => {
      if (!dragState.current) return;
      const dx = ev.clientX - dragState.current.startX;
      const dBeats = dx / pixelsPerBeat;
      lastNewDur = Math.max(0.25, Math.round((dragState.current!.origDuration + dBeats) * 4) / 4);
      // Only update visual state during drag, don't sync to code yet
      onUpdateClip(trackId, clip.id, { durationBeats: lastNewDur });
    };
    const onUp = () => {
      if (lastNewDur !== origDuration) {
        // Pass the original duration from dragState to ensure correct old value
        onResizeEnd(trackId, clip.id, lastNewDur, dragState.current!.origDuration);
      }
      dragState.current = null;
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  };

  const left = clip.startBeat * pixelsPerBeat;
  const width = clip.durationBeats * pixelsPerBeat;

  // For looping clips, render ghost repeats
  const repeats: React.ReactElement[] = [];
  if (clip.isLooping && clip.durationBeats > 0) {
    let repeatStart = clip.startBeat + clip.durationBeats;
    let repeatIdx = 0;
    while (repeatStart < totalBeats && repeatIdx < 20) {
      repeats.push(
        <div
          key={`repeat-${repeatIdx}`}
          className="clip-repeat"
          style={{
            left: repeatStart * pixelsPerBeat,
            width: Math.min(clip.durationBeats, totalBeats - repeatStart) * pixelsPerBeat,
            backgroundColor: clip.color + '30',
            borderColor: clip.color + '50',
          }}
        >
          <span className="clip-repeat-label">⟳</span>
        </div>
      );
      repeatStart += clip.durationBeats;
      repeatIdx++;
    }
  }

  const clipStyle: React.CSSProperties = {
    left,
    width: Math.max(width, 4),
    backgroundColor: clip.color + '40',
    borderColor: clip.color + '80',
  };

  // Audio tail: faded extension showing where audio rings out past code execution
  const audioTailWidth = clip.audioTailBeats * pixelsPerBeat;

  return (
    <>
      <div
        ref={clipRef}
        className={`timeline-clip ${clip.type} ${hovering ? 'clip-hover' : ''} ${clip.needsPreload ? 'clip-needs-preload' : ''}`}
        style={clipStyle}
        onMouseEnter={handleMouseEnter}
        onMouseMove={handleMouseMove}
        onMouseLeave={handleMouseLeave}
        onContextMenu={handleContextMenu}
        onMouseDown={handleDragStart}
      >
        <div className="clip-header" style={{ backgroundColor: clip.color + '60' }}>
          {clip.needsPreload && <span className="clip-preload-badge" title="File-path sample (needs preloading)">!</span>}
          <span className="clip-label">{clip.name}</span>
          {clip.isLooping && <span className="clip-loop-badge">⟳</span>}
          {clip.effects.length > 0 && (
            <span className="clip-fx-badge">FX:{clip.effects.length}</span>
          )}
        </div>
        <div className="clip-body">
          {clip.samples.length > 0 && (
            <div className="clip-samples">
              {clip.samples.slice(0, 3).map((s, i) => (
                <span key={i} className="clip-sample-tag">{s}</span>
              ))}
              {clip.samples.length > 3 && <span className="clip-sample-more">+{clip.samples.length - 3}</span>}
            </div>
          )}
          {clip.type === 'synth' && (
            <div className="clip-waveform-placeholder">
              <svg viewBox="0 0 100 20" preserveAspectRatio="none">
                <path
                  d="M0 10 Q5 2 10 10 Q15 18 20 10 Q25 2 30 10 Q35 18 40 10 Q45 2 50 10 Q55 18 60 10 Q65 2 70 10 Q75 18 80 10 Q85 2 90 10 Q95 18 100 10"
                  fill="none"
                  stroke={clip.color}
                  strokeWidth="1.5"
                  opacity="0.6"
                />
              </svg>
            </div>
          )}
        </div>
        <div className="clip-amp-bar" style={{ width: `${Math.min(clip.amp * 100, 100)}%`, backgroundColor: clip.color }} />
        {/* Resize handle */}
        <div className="clip-resize-handle" onMouseDown={handleResizeStart} />
      </div>

      {/* Audio tail: faded extension showing sample audio extending past code execution */}
      {audioTailWidth > 2 && (
        <div
          className="clip-audio-tail"
          style={{
            left: left + width,
            width: audioTailWidth,
            backgroundColor: clip.color + '18',
            borderColor: clip.color + '30',
          }}
        >
          <div className="clip-audio-tail-gradient" style={{ background: `linear-gradient(to right, ${clip.color}25, transparent)` }} />
          <svg viewBox="0 0 100 16" preserveAspectRatio="none" className="clip-audio-tail-wave">
            <path
              d="M0 8 Q10 2 20 8 Q30 14 40 8 Q50 2 60 8 Q70 14 80 8 Q90 2 100 8"
              fill="none"
              stroke={clip.color}
              strokeWidth="1"
              opacity="0.3"
            />
          </svg>
        </div>
      )}

      {repeats}

      {hovering && <ClipTooltip clip={clip} x={tooltipPos.x} y={tooltipPos.y} />}

      {showEffects && (
        <ClipEffectsPopover
          clip={clip}
          trackId={trackId}
          onUpdateClip={onUpdateClip}
          onClose={() => setShowEffects(false)}
        />
      )}
    </>
  );
};

// ─── Track Header Component ──────────────────────────────────────

const TrackHeader: React.FC<{
  track: TimelineTrack;
  onUpdateTrack: (trackId: string, updates: Partial<TimelineTrack>) => void;
}> = ({ track, onUpdateTrack }) => {
  const [showTrackFx, setShowTrackFx] = useState(false);

  return (
    <div className={`track-header-wrapper ${track.muted ? 'track-muted' : ''} ${track.solo ? 'track-solo' : ''}`}>
      <div className="track-header">
        <div className="track-name" style={{ color: track.color }}>
          {track.name}
        </div>
        <div className="track-controls">
          <button
            className={`track-ctrl-btn ${track.muted ? 'active' : ''}`}
            onClick={() => onUpdateTrack(track.id, { muted: !track.muted })}
            title="Mute"
          >
            M
          </button>
          <button
            className={`track-ctrl-btn ${track.solo ? 'active' : ''}`}
            onClick={() => onUpdateTrack(track.id, { solo: !track.solo })}
            title="Solo"
          >
            S
          </button>
          <button
            className="track-ctrl-btn"
            onClick={() => setShowTrackFx(!showTrackFx)}
            title="Track Effects"
          >
            FX
          </button>
        </div>
        <div className="track-amp-slider">
          <input
            type="range"
            min="0"
            max="2"
            step="0.05"
            value={track.amp}
            onChange={(e) => onUpdateTrack(track.id, { amp: parseFloat(e.target.value) })}
            title={`Amp: ${track.amp.toFixed(2)}`}
          />
        </div>
      </div>
      {showTrackFx && (
        <TrackEffectsPopover
          track={track}
          onUpdateTrack={onUpdateTrack}
          onClose={() => setShowTrackFx(false)}
        />
      )}
    </div>
  );
};

// ─── Track Lane ──────────────────────────────────────────────────

// @ts-ignore: Reserved for future use
const TrackLane: React.FC<{
  track: TimelineTrack;
  pixelsPerBeat: number;
  totalBeats: number;
  onUpdateTrack: (trackId: string, updates: Partial<TimelineTrack>) => void;
  onUpdateClip: (trackId: string, clipId: string, updates: Partial<TimelineClip>) => void;
  onDragEnd: (trackId: string, clipId: string, newStart: number, oldStart: number) => void;
  onResizeEnd: (trackId: string, clipId: string, newDur: number, oldDur: number) => void;
}> = ({ track, pixelsPerBeat, totalBeats, onUpdateTrack, onUpdateClip, onDragEnd, onResizeEnd }) => {
  const [showTrackFx, setShowTrackFx] = useState(false);

  return (
    <div className={`track-lane ${track.muted ? 'track-muted' : ''} ${track.solo ? 'track-solo' : ''}`}>
      {/* Track header (left side) */}
      <div className="track-header">
        <div className="track-name" style={{ color: track.color }}>
          {track.name}
        </div>
        <div className="track-controls">
          <button
            className={`track-ctrl-btn ${track.muted ? 'active' : ''}`}
            onClick={() => onUpdateTrack(track.id, { muted: !track.muted })}
            title="Mute"
          >
            M
          </button>
          <button
            className={`track-ctrl-btn ${track.solo ? 'active' : ''}`}
            onClick={() => onUpdateTrack(track.id, { solo: !track.solo })}
            title="Solo"
          >
            S
          </button>
          <button
            className="track-ctrl-btn"
            onClick={() => setShowTrackFx(!showTrackFx)}
            title="Track Effects"
          >
            FX
          </button>
        </div>
        <div className="track-amp-slider">
          <input
            type="range"
            min="0"
            max="2"
            step="0.05"
            value={track.amp}
            onChange={(e) => onUpdateTrack(track.id, { amp: parseFloat(e.target.value) })}
            title={`Amp: ${track.amp.toFixed(2)}`}
          />
        </div>
      </div>

      {/* Track content (scrollable area with clips) */}
      <div className="track-content">
        <div className="track-clips-area" style={{ width: totalBeats * pixelsPerBeat }}>
          {/* Grid lines */}
          {Array.from({ length: Math.ceil(totalBeats / 4) }, (_, i) => (
            <div
              key={`grid-${i}`}
              className="track-grid-bar"
              style={{ left: i * 4 * pixelsPerBeat }}
            />
          ))}

          {/* Clips */}
          {track.clips.map((clip) => (
            <ClipComponent
              key={clip.id}
              clip={clip}
              trackId={track.id}
              pixelsPerBeat={pixelsPerBeat}
              totalBeats={totalBeats}
              onUpdateClip={onUpdateClip}
              onDragEnd={onDragEnd}
              onResizeEnd={onResizeEnd}
            />
          ))}
        </div>
      </div>

      {/* Track FX popover */}
      {showTrackFx && (
        <TrackEffectsPopover
          track={track}
          onUpdateTrack={onUpdateTrack}
          onClose={() => setShowTrackFx(false)}
        />
      )}
    </div>
  );
};

// ─── Playhead (unused standalone – now rendered inline) ─────────

// ─── Main TimelineView ──────────────────────────────────────────

const TimelineView: React.FC = () => {
  const { bpm, isPlaying, isPaused, updateBufferCode, activeBufferId, setupTimeMs, sampleDurations, fetchSampleDurations, runCode } = useStore();
  // Subscribe only to the active buffer's code to avoid re-renders when other buffers change
  const activeBuffer = useStore((s) => s.buffers.find(b => b.id === s.activeBufferId));
  const [pixelsPerBeat, setPixelsPerBeat] = useState(24);
  const [playheadBeat, setPlayheadBeat] = useState(0);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [scrollLeft, setScrollLeft] = useState(0);
  const playStartTimeRef = useRef<number | null>(null);
  const pauseStartRef = useRef<number | null>(null);
  const isDraggingPlayhead = useRef(false);
  const [isDraggingPlayheadVisual, setIsDraggingPlayheadVisual] = useState(false);
  const rulerScrollRef = useRef<HTMLDivElement>(null);

  // Extract sample names from the active buffer and fetch their durations
  const activeCode = activeBuffer?.code || '';

  useEffect(() => {
    if (!activeCode || activeCode.trim().length <= 20) return;
    const { builtins, filePaths } = extractCodeSampleNames(activeCode);
    const allNames = [...builtins, ...filePaths];
    if (allNames.length > 0) {
      fetchSampleDurations(allNames);
    }
  }, [activeCode, fetchSampleDurations]);

  // Parse only the active buffer into timeline data (with sample durations)
  const timelineData: TimelineData = useMemo(() => {
    if (!activeCode || activeCode.trim().length <= 20) {
      return { tracks: [], bpm, totalBeats: 0, sections: [] };
    }
    return parseCodeToTimeline(activeCode, activeBufferId, sampleDurations);
  }, [activeCode, activeBufferId, bpm, sampleDurations]);

  // Local mutable track state (so we can modify mute/solo/amp/effects without changing buffers)
  const [tracks, setTracks] = useState<TimelineTrack[]>([]);

  // Merge parsed timeline data with locally-preserved track state
  // (amp, muted, solo) so slider positions survive re-parses.
  useEffect(() => {
    setTracks(prev => {
      return timelineData.tracks.map(newTrack => {
        const existing = prev.find(t => t.name === newTrack.name);
        if (existing) {
          return {
            ...newTrack,
            amp: existing.amp,
            muted: existing.muted,
            solo: existing.solo,
          };
        }
        return newTrack;
      });
    });
  }, [timelineData]);

  // Compute totalBeats from both parsed data AND local edited tracks
  const totalBeats = useMemo(() => {
    let maxBeat = Math.max(timelineData.totalBeats, 32);
    for (const track of tracks) {
      for (const clip of track.clips) {
        maxBeat = Math.max(maxBeat, clip.startBeat + clip.durationBeats);
      }
    }
    return Math.ceil(maxBeat);
  }, [timelineData.totalBeats, tracks]);

  // Pending code map: deduplicates by buffer ID so rapid changes
  // (e.g. slider drags) always start from the latest in-flight code.
  const pendingCodeRef = useRef<Map<number, string>>(new Map());
  // Debounced re-run timer: re-runs the code after timeline edits while playing
  const rerunTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isPlayingRef = useRef(isPlaying);
  isPlayingRef.current = isPlaying;

  // Schedule a debounced re-run of the code (only while playing)
  const scheduleRerun = useCallback(() => {
    if (!isPlayingRef.current) return;
    if (rerunTimerRef.current) clearTimeout(rerunTimerRef.current);
    rerunTimerRef.current = setTimeout(() => {
      rerunTimerRef.current = null;
      if (isPlayingRef.current) {
        runCode();
      }
    }, 400);
  }, [runCode]);

  // Flush pending code changes to the store after each render
  useEffect(() => {
    if (pendingCodeRef.current.size === 0) return;
    const pending = new Map(pendingCodeRef.current);
    pendingCodeRef.current.clear();
    
    for (const [bufferId, code] of pending) {
      updateBufferCode(bufferId, code);
    }

    // Re-run code so the audio engine picks up the changes
    scheduleRerun();
  });

  // Use parsed BPM from timeline data for accurate playhead.
  // The parser extracts use_bpm from the code, which is the authoritative BPM for timeline visualization.
  // The store's bpm is updated after runCode() resolves, but may lag behind.
  const effectiveBpm = timelineData.bpm || bpm;

  // Keep a ref so the animation callback always reads the latest BPM & totalBeats
  // without needing the useEffect to re-run (which would reset the playhead).
  const effectiveBpmRef = useRef(effectiveBpm);
  effectiveBpmRef.current = effectiveBpm;
  const totalBeatsRef = useRef(totalBeats);
  totalBeatsRef.current = totalBeats;
  const pixelsPerBeatRef = useRef(pixelsPerBeat);
  pixelsPerBeatRef.current = pixelsPerBeat;

  // Animate playhead — only re-run when isPlaying/isPaused changes, NOT when bpm/totalBeats change.
  // This prevents the playhead from jumping back to zero on re-parses during playback.
  useEffect(() => {
    if (isPlaying && !isPaused) {
      // Resuming from pause: adjust start time to account for paused duration
      if (pauseStartRef.current !== null && playStartTimeRef.current !== null) {
        const pausedDuration = Date.now() - pauseStartRef.current;
        playStartTimeRef.current += pausedDuration;
        pauseStartRef.current = null;
      }
      // Only set the start time once per playback session.
      // setupTimeMs is the time the backend already spent playing before we got the response.
      if (playStartTimeRef.current === null) {
        playStartTimeRef.current = Date.now() - setupTimeMs;
      }
      let animFrameId: number;
      const animate = () => {
        if (!playStartTimeRef.current || isDraggingPlayhead.current) {
          animFrameId = requestAnimationFrame(animate);
          return;
        }
        const elapsed = (Date.now() - playStartTimeRef.current) / 1000;
        const beat = elapsed * (effectiveBpmRef.current / 60);
        setPlayheadBeat(beat);

        // Auto-scroll to keep playhead visible
        const container = scrollContainerRef.current;
        if (container) {
          const headerWidth = 180; // track header column width
          const playheadX = beat * pixelsPerBeatRef.current;
          const visibleLeft = container.scrollLeft;
          const visibleRight = visibleLeft + container.clientWidth - headerWidth;
          // Scroll when playhead nears the right edge (within 15% margin)
          const margin = (container.clientWidth - headerWidth) * 0.15;
          if (playheadX > visibleRight - margin) {
            container.scrollLeft = playheadX - (container.clientWidth - headerWidth) * 0.25;
          } else if (playheadX < visibleLeft + margin && visibleLeft > 0) {
            container.scrollLeft = Math.max(0, playheadX - (container.clientWidth - headerWidth) * 0.75);
          }
        }

        // Keep animating even if past totalBeats — live_loops keep playing indefinitely
        animFrameId = requestAnimationFrame(animate);
      };
      animFrameId = requestAnimationFrame(animate);
      return () => cancelAnimationFrame(animFrameId);
    } else if (isPlaying && isPaused) {
      // Record when pause started so we can adjust on resume
      if (pauseStartRef.current === null) {
        pauseStartRef.current = Date.now();
      }
    } else {
      // Stopped: reset playhead to beginning
      playStartTimeRef.current = null;
      pauseStartRef.current = null;
      setPlayheadBeat(0);
      if (scrollContainerRef.current) {
        scrollContainerRef.current.scrollLeft = 0;
      }
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isPlaying, isPaused, setupTimeMs]);

  // Seek the playhead to a specific beat
  const seekToBeat = useCallback((beat: number) => {
    const clampedBeat = Math.max(0, Math.min(beat, totalBeatsRef.current));
    setPlayheadBeat(clampedBeat);
    if (isPlaying) {
      // Adjust playStartTimeRef so animation continues from this beat
      const secondsForBeat = clampedBeat * (60 / effectiveBpmRef.current);
      playStartTimeRef.current = Date.now() - secondsForBeat * 1000;
    }
  }, [isPlaying]);

  // Ruler click to seek
  const handleRulerClick = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    // Don't seek if click originated from playhead drag
    if (isDraggingPlayhead.current) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left + e.currentTarget.scrollLeft;
    const beat = x / pixelsPerBeat;
    seekToBeat(beat);
  }, [pixelsPerBeat, seekToBeat]);

  // Playhead drag handlers
  const handlePlayheadDragStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    isDraggingPlayhead.current = true;
    setIsDraggingPlayheadVisual(true);

    const onMove = (ev: MouseEvent) => {
      if (!rulerScrollRef.current) return;
      const rect = rulerScrollRef.current.getBoundingClientRect();
      const x = ev.clientX - rect.left + rulerScrollRef.current.scrollLeft;
      const beat = Math.max(0, Math.min(x / pixelsPerBeat, totalBeats));
      setPlayheadBeat(beat);
    };

    const onUp = (ev: MouseEvent) => {
      isDraggingPlayhead.current = false;
      setIsDraggingPlayheadVisual(false);
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      // Calculate final beat and update play start ref
      if (rulerScrollRef.current) {
        const rect = rulerScrollRef.current.getBoundingClientRect();
        const x = ev.clientX - rect.left + rulerScrollRef.current.scrollLeft;
        const beat = Math.max(0, Math.min(x / pixelsPerBeat, totalBeats));
        seekToBeat(beat);
      }
    };

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }, [pixelsPerBeat, totalBeats, seekToBeat]);

  const handleScroll = useCallback((e: React.UIEvent<HTMLDivElement>) => {
    setScrollLeft(e.currentTarget.scrollLeft);
  }, []);

  // Viewport culling: only render clips within the visible beat range (+ margin)
  const visibleBeatRange = useMemo(() => {
    const headerWidth = 180;
    const containerWidth = scrollContainerRef.current?.clientWidth ?? 1200;
    const viewportWidth = containerWidth - headerWidth;
    const margin = viewportWidth * 0.5; // render extra clips outside viewport for smooth scrolling
    const startBeat = Math.max(0, (scrollLeft - margin) / pixelsPerBeat);
    const endBeat = (scrollLeft + viewportWidth + margin) / pixelsPerBeat;
    return { startBeat, endBeat };
  }, [scrollLeft, pixelsPerBeat]);

  const handleZoomIn = () => setPixelsPerBeat(prev => Math.min(prev * 1.5, 120));
  const handleZoomOut = () => setPixelsPerBeat(prev => Math.max(prev / 1.5, 6));
  const handleZoomFit = () => {
    if (scrollContainerRef.current) {
      const containerWidth = scrollContainerRef.current.clientWidth - 180; // subtract track header
      setPixelsPerBeat(Math.max(containerWidth / totalBeats, 6));
    }
  };

  // ── Sync: push updated code to the store ──
  const syncToBuffer = useCallback((bufferId: number, newCode: string) => {
    // Store in pending map — deduplicates rapid changes per buffer
    pendingCodeRef.current.set(bufferId, newCode);
  }, []);

  /** Read the latest code for a buffer, including un-flushed pending changes. */
  const getLatestCode = useCallback((bufferId: number): string | null => {
    const pending = pendingCodeRef.current.get(bufferId);
    if (pending !== undefined) return pending;
    const buf = useStore.getState().buffers.find(b => b.id === bufferId);
    return buf?.code ?? null;
  }, []);

  // ── Track update with auto-sync ──
  const updateTrack = useCallback((trackId: string, updates: Partial<TimelineTrack>) => {
    setTracks(prev => {
      const newTracks = prev.map(t =>
        t.id === trackId ? { ...t, ...updates } : t
      );

      // Sync amp and mute changes to code
      const track = prev.find(t => t.id === trackId);
      if (!track) return newTracks;

      if (updates.amp !== undefined && updates.amp !== track.amp) {
        // Compute ratio so clip amps scale proportionally (avoids compounding)
        const ampRatio = (track.amp > 0) ? updates.amp / track.amp : updates.amp;
        // Group clips by buffer and apply amp change
        const byBuffer = new Map<number, { bufferId: number; clips: TimelineClip[] }>();
        for (const clip of track.clips) {
          if (!byBuffer.has(clip.bufferId)) byBuffer.set(clip.bufferId, { bufferId: clip.bufferId, clips: [] });
          byBuffer.get(clip.bufferId)!.clips.push(clip);
        }
        for (const { bufferId, clips } of byBuffer.values()) {
          let code = getLatestCode(bufferId);
          if (!code) continue;
          code = applyTrackAmpChange(code, { ...track, clips }, ampRatio);
          syncToBuffer(bufferId, code);
        }
      }

      if (updates.muted !== undefined && updates.muted !== track.muted) {
        for (const clip of track.clips) {
          let code = getLatestCode(clip.bufferId);
          if (!code) continue;
          code = applyClipMute(code, clip, updates.muted);
          syncToBuffer(clip.bufferId, code);
        }
      }

      // Sync track-level effects changes to code via the first clip
      if (updates.effects !== undefined && track.clips.length > 0) {
        const refClip = track.clips[0];
        let code = getLatestCode(refClip.bufferId);
        if (code) {
          const oldFx = track.effects;
          const newFx = updates.effects;
          let changed = false;
          // Added effects
          for (const fx of newFx) {
            if (!oldFx.find(o => o.type === fx.type)) {
              code = applyAddEffect(code, refClip, fx);
              changed = true;
            }
          }
          // Removed effects
          for (const fx of oldFx) {
            if (!newFx.find(n => n.type === fx.type)) {
              code = applyRemoveEffect(code, refClip, fx.type);
              changed = true;
            }
          }
          // Updated params
          for (const fx of newFx) {
            const old = oldFx.find(o => o.type === fx.type);
            if (old && JSON.stringify(old.params) !== JSON.stringify(fx.params)) {
              code = applyUpdateEffectParams(code, refClip, fx.type, fx.params);
              changed = true;
            }
          }
          if (changed) {
            syncToBuffer(refClip.bufferId, code);
          }
        }
      }

      return newTracks;
    });
  }, [syncToBuffer, getLatestCode]);

  // ── Clip update with auto-sync ──
  const updateClip = useCallback((trackId: string, clipId: string, updates: Partial<TimelineClip>) => {
    setTracks(prev => {
      const newTracks = prev.map(t =>
        t.id === trackId
          ? { ...t, clips: t.clips.map(c => c.id === clipId ? { ...c, ...updates } : c) }
          : t
      );

      // Find the original clip to sync changes
      let origClip: TimelineClip | undefined;
      for (const t of prev) {
        if (t.id !== trackId) continue;
        origClip = t.clips.find(c => c.id === clipId);
      }
      if (!origClip) return newTracks;
      let code = getLatestCode(origClip.bufferId);
      if (!code) return newTracks;

      let changed = false;

      // Amp change
      if (updates.amp !== undefined && updates.amp !== origClip.amp) {
        code = applyClipAmpChange(code, origClip, updates.amp);
        changed = true;
      }

      // Start beat and duration changes are not synced here
      // They are handled via onDragEnd/onResizeEnd callbacks

      // Effects changes
      if (updates.effects !== undefined) {
        const oldFx = origClip.effects;
        const newFx = updates.effects;
        // Added effects
        for (const fx of newFx) {
          if (!oldFx.find(o => o.type === fx.type)) {
            code = applyAddEffect(code, origClip, fx);
            changed = true;
          }
        }
        // Removed effects
        for (const fx of oldFx) {
          if (!newFx.find(n => n.type === fx.type)) {
            code = applyRemoveEffect(code, origClip, fx.type);
            changed = true;
          }
        }
        // Updated params
        for (const fx of newFx) {
          const old = oldFx.find(o => o.type === fx.type);
          if (old && JSON.stringify(old.params) !== JSON.stringify(fx.params)) {
            code = applyUpdateEffectParams(code, origClip, fx.type, fx.params);
            changed = true;
          }
        }
      }

      if (changed) {
        syncToBuffer(origClip.bufferId, code);
      }

      return newTracks;
    });
  }, [syncToBuffer, getLatestCode]);

  // ── Sync move/resize on mouse-up (called from ClipComponent) ──
  const syncClipMove = useCallback((trackId: string, clipId: string, newStartBeat: number, oldStartBeat: number) => {
    // Find clip from parsed timeline data (before local state updates)
    const parsedTrack = timelineData.tracks.find(t => t.id === trackId);
    if (!parsedTrack) return;
    const parsedClip = parsedTrack.clips.find(c => c.id === clipId);
    if (!parsedClip) return;
    
    const code = getLatestCode(parsedClip.bufferId);
    if (!code) return;
    
    const newCode = applyClipStartChange(code, parsedClip, newStartBeat, oldStartBeat);
    syncToBuffer(parsedClip.bufferId, newCode);
  }, [timelineData, syncToBuffer, getLatestCode]);

  const syncClipResize = useCallback((trackId: string, clipId: string, newDuration: number, oldDuration: number) => {
    // Find clip from parsed timeline data (before local state updates)
    const parsedTrack = timelineData.tracks.find(t => t.id === trackId);
    if (!parsedTrack) return;
    const parsedClip = parsedTrack.clips.find(c => c.id === clipId);
    if (!parsedClip) return;
    
    const code = getLatestCode(parsedClip.bufferId);
    if (!code) return;
    
    const newCode = applyClipDurationChange(code, parsedClip, newDuration, oldDuration);
    syncToBuffer(parsedClip.bufferId, newCode);
  }, [timelineData, syncToBuffer, getLatestCode]);

  const secondsPerBeat = 60 / effectiveBpm;
  const totalSeconds = totalBeats * secondsPerBeat;
  const totalMins = Math.floor(totalSeconds / 60);
  const totalSecs = Math.floor(totalSeconds % 60);

  return (
    <div className="timeline-view">
      {/* Timeline toolbar */}
      <div className="timeline-toolbar">
        <div className="timeline-toolbar-left">
          <span className="timeline-info">
            {tracks.length} tracks · {totalBeats.toFixed(0)} beats · {totalMins}:{totalSecs.toString().padStart(2, '0')} @ {effectiveBpm} BPM
          </span>
        </div>
        <div className="timeline-toolbar-right">
          <button className="timeline-zoom-btn" onClick={handleZoomOut} title="Zoom out">−</button>
          <button className="timeline-zoom-btn" onClick={handleZoomFit} title="Zoom to fit">⊞</button>
          <button className="timeline-zoom-btn" onClick={handleZoomIn} title="Zoom in">+</button>
          <span className="timeline-zoom-label">{Math.round(pixelsPerBeat)}px/beat</span>
        </div>
      </div>

      {/* Timeline content */}
      <div className="timeline-body" ref={scrollContainerRef} onScroll={handleScroll}>
        {/* Ruler row */}
        <div className="timeline-ruler-row">
          <div className="timeline-ruler-spacer" />
          <div className="timeline-ruler-scroll" ref={rulerScrollRef} onClick={handleRulerClick}>
            <TimeRuler
              totalBeats={totalBeats}
              pixelsPerBeat={pixelsPerBeat}
              bpm={effectiveBpm}
              scrollLeft={scrollLeft}
              sections={timelineData.sections}
            />
            <div
              className={`timeline-playhead ${isDraggingPlayheadVisual ? 'dragging' : ''}`}
              style={{ left: playheadBeat * pixelsPerBeat }}
            >
              <div
                className="playhead-head"
                onMouseDown={handlePlayheadDragStart}
              />
              <div className="playhead-line" />
            </div>
          </div>
        </div>

        {tracks.length === 0 ? (
          <div className="timeline-empty">
            <p>No tracks to display</p>
            <p className="timeline-empty-hint">Write Sonic Pi code with <code>live_loop</code>, <code>sample</code>, <code>play</code>, or <code>play_pattern_timed</code> — all structures appear as clips. Drag to move, resize edges, right-click for effects.</p>
          </div>
        ) : (
          <div className="timeline-tracks-container">
            {/* Fixed left column with track headers */}
            <div className="timeline-track-headers-column">
              {tracks.map((track) => (
                <TrackHeader
                  key={track.id}
                  track={track}
                  onUpdateTrack={updateTrack}
                />
              ))}
            </div>
            {/* Scrollable content area with clips */}
            <div className="timeline-track-contents-column" style={{ position: 'relative' }}>
              {tracks.map((track) => (
                <div key={track.id} className="track-content-wrapper">
                  <div className="track-content">
                    <div className="track-clips-area" style={{ width: totalBeats * pixelsPerBeat }}>
                      {/* Grid lines */}
                      {Array.from({ length: Math.ceil(totalBeats / 4) }, (_, i) => (
                        <div
                          key={`grid-${track.id}-${i}`}
                          className="track-grid-bar"
                          style={{ left: i * 4 * pixelsPerBeat }}
                        />
                      ))}

                      {/* Clips (viewport-culled) */}
                      {track.clips
                        .filter(clip => clip.startBeat + clip.durationBeats >= visibleBeatRange.startBeat && clip.startBeat <= visibleBeatRange.endBeat)
                        .map((clip) => (
                        <ClipComponent
                          key={clip.id}
                          clip={clip}
                          trackId={track.id}
                          pixelsPerBeat={pixelsPerBeat}
                          totalBeats={totalBeats}
                          onUpdateClip={updateClip}
                          onDragEnd={syncClipMove}
                          onResizeEnd={syncClipResize}
                        />
                      ))}
                    </div>
                  </div>
                </div>
              ))}
              {/* Playhead line spanning all tracks */}
              <div
                className="timeline-playhead-track-line"
                style={{ left: playheadBeat * pixelsPerBeat }}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default TimelineView;
