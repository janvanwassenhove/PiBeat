import React, { useRef, useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useStore } from '../store';

const FETCH_INTERVAL_MS = 33; // ~30fps for IPC data fetch

type ScopeMode = 'wave' | 'bars' | 'lissajous';
const SCOPE_MODES: { key: ScopeMode; label: string }[] = [
  { key: 'wave', label: 'Wave' },
  { key: 'bars', label: 'Bars' },
  { key: 'lissajous', label: 'Scope' },
];

const WaveformVisualizer: React.FC = () => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationRef = useRef<number>(0);
  const waveformRef = useRef<number[]>([]);
  const fetchingRef = useRef(false);
  const fetchTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const isPlaying = useStore((s) => s.isPlaying);
  const theme = useStore((s) => s.theme);
  const [scopeMode, setScopeMode] = useState<ScopeMode>('wave');
  const scopeModeRef = useRef<ScopeMode>(scopeMode);

  const cycleScopeMode = useCallback(() => {
    setScopeMode((prev) => {
      const idx = SCOPE_MODES.findIndex((m) => m.key === prev);
      const next = SCOPE_MODES[(idx + 1) % SCOPE_MODES.length].key;
      scopeModeRef.current = next;
      return next;
    });
  }, []);

  // Keep ref in sync for the draw loop
  useEffect(() => {
    scopeModeRef.current = scopeMode;
  }, [scopeMode]);

  // Fetch waveform data on a fixed interval, bypassing Zustand entirely.
  //
  // Only while something is playing: an idle scope shows a flat line, and
  // polling for it was costing 30 IPC round trips a second forever. On the
  // SuperCollider path each one also pumps the OSC socket, so this was the
  // app's single busiest idle activity. One final fetch after playback stops
  // flushes the tail of the sound out of the display.
  useEffect(() => {
    const fetchWaveform = async () => {
      if (fetchingRef.current) return; // skip if previous call still in-flight
      fetchingRef.current = true;
      try {
        const data = await invoke<number[]>('get_waveform');
        waveformRef.current = data;
      } catch {
        // Ignore waveform errors
      } finally {
        fetchingRef.current = false;
      }
    };

    if (!isPlaying) {
      // Settle the display on the last of the audio, then stop polling.
      const settle = setTimeout(() => {
        fetchWaveform().then(() => {
          waveformRef.current = [];
        });
      }, FETCH_INTERVAL_MS);
      return () => clearTimeout(settle);
    }

    fetchWaveform();
    fetchTimerRef.current = setInterval(fetchWaveform, FETCH_INTERVAL_MS);
    return () => {
      if (fetchTimerRef.current) clearInterval(fetchTimerRef.current);
      fetchTimerRef.current = null;
    };
  }, [isPlaying]);

  // Paint one frame from the current buffer. Held in a ref so the resize
  // observer can repaint while idle, when no animation loop is running.
  const paintRef = useRef<() => void>(() => {});
  paintRef.current = () => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const mode = scopeModeRef.current;
    if (mode === 'bars') {
      drawBars(ctx, canvas.width, canvas.height, waveformRef.current, theme);
    } else if (mode === 'lissajous') {
      drawLissajous(ctx, canvas.width, canvas.height, waveformRef.current, theme);
    } else {
      drawWaveform(ctx, canvas.width, canvas.height, waveformRef.current, theme);
    }
  };

  // Canvas resize handler
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const container = canvas.parentElement;
    const resize = () => {
      const w = canvas.offsetWidth;
      const h = canvas.offsetHeight;
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
        // Assigning width/height clears the canvas. While playing the next
        // animation frame covers that, but when idle there is no next frame —
        // repaint here or the scope goes blank on any layout change.
        paintRef.current();
      }
    };
    resize();

    let ro: ResizeObserver | undefined;
    if (container) {
      ro = new ResizeObserver(resize);
      ro.observe(container);
    }
    window.addEventListener('resize', resize);
    return () => {
      window.removeEventListener('resize', resize);
      ro?.disconnect();
    };
  }, []);

  // Draw loop at display refresh rate — reads from ref, no React state involved.
  //
  // The loop only runs during playback. When idle the canvas is painted once
  // (empty scope, grid and all) and then left alone, instead of redrawing an
  // unchanging picture — with shadow blur and three gradients per frame — for
  // as long as the app is open.
  useEffect(() => {
    let running = true;

    if (!isPlaying) {
      // Paint once now, then again after the post-stop fetch has settled, so
      // what's left on screen is the end of the sound rather than a stale frame.
      paintRef.current();
      const settled = setTimeout(() => paintRef.current(), FETCH_INTERVAL_MS * 3);
      return () => clearTimeout(settled);
    }

    const draw = () => {
      if (!running) return;
      paintRef.current();
      animationRef.current = requestAnimationFrame(draw);
    };

    animationRef.current = requestAnimationFrame(draw);
    return () => {
      running = false;
      cancelAnimationFrame(animationRef.current);
    };
  }, [theme, isPlaying, scopeMode]);

  return (
    <div className="waveform-container">
      <div className="waveform-label">
        <span className={`status-dot ${isPlaying ? 'active' : ''}`} />
        SCOPE
        <div className="scope-mode-switcher">
          {SCOPE_MODES.map((m) => (
            <button
              key={m.key}
              className={`scope-mode-btn ${scopeMode === m.key ? 'active' : ''}`}
              onClick={() => { scopeModeRef.current = m.key; setScopeMode(m.key); }}
              title={m.label}
            >
              {m.label}
            </button>
          ))}
        </div>
      </div>
      <canvas ref={canvasRef} className="waveform-canvas" onClick={cycleScopeMode} />
    </div>
  );
};

/** Pure drawing function — no React state, no allocations beyond canvas ops */
function drawWaveform(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  waveform: number[],
  theme: string,
) {
  const midY = height / 2;

  const isSonicPi = theme === 'sonicpi';
  const isAmber = theme === 'amber';
  const waveColor = isSonicPi ? '#ff59b2' : isAmber ? '#ffaa00' : '#00ff88';
  const bgTop = isSonicPi ? '#0a0a0a' : isAmber ? '#0f0d08' : '#0d0d2b';
  const bgMid = isSonicPi ? '#0e0e0e' : isAmber ? '#12100a' : '#121233';
  const centerLineColor = isSonicPi ? '#1a1a1a' : isAmber ? '#1d1912' : '#222255';
  const gridColor = isSonicPi ? '#141414' : isAmber ? '#1a1710' : '#1a1a40';
  const fillR = isSonicPi ? '255, 89, 178' : isAmber ? '255, 170, 0' : '0, 255, 136';

  // Clear with gradient background
  const gradient = ctx.createLinearGradient(0, 0, 0, height);
  gradient.addColorStop(0, bgTop);
  gradient.addColorStop(0.5, bgMid);
  gradient.addColorStop(1, bgTop);
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, width, height);

  // Draw center line
  ctx.strokeStyle = centerLineColor;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(0, midY);
  ctx.lineTo(width, midY);
  ctx.stroke();

  // Draw grid lines
  ctx.strokeStyle = gridColor;
  ctx.lineWidth = 0.5;
  for (let i = 1; i < 4; i++) {
    const y = (height / 4) * i;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(width, y);
    ctx.stroke();
  }

  if (!waveform || waveform.length === 0) return;

  // Draw waveform
  const step = waveform.length / width;

  // Glow effect
  ctx.shadowColor = waveColor;
  ctx.shadowBlur = 8;

  // Main waveform line
  ctx.strokeStyle = waveColor;
  ctx.lineWidth = 2;
  ctx.beginPath();

  for (let x = 0; x < width; x++) {
    const idx = Math.floor(x * step);
    const sample = waveform[idx] || 0;
    const y = midY - sample * midY * 0.9;

    if (x === 0) {
      ctx.moveTo(x, y);
    } else {
      ctx.lineTo(x, y);
    }
  }
  ctx.stroke();

  // Draw filled area under waveform
  ctx.shadowBlur = 0;
  const fillGradient = ctx.createLinearGradient(0, 0, 0, height);
  fillGradient.addColorStop(0, `rgba(${fillR}, 0.15)`);
  fillGradient.addColorStop(0.5, `rgba(${fillR}, 0.05)`);
  fillGradient.addColorStop(1, `rgba(${fillR}, 0.15)`);
  ctx.fillStyle = fillGradient;
  ctx.beginPath();
  ctx.moveTo(0, midY);
  for (let x = 0; x < width; x++) {
    const idx = Math.floor(x * step);
    const sample = waveform[idx] || 0;
    const y = midY - sample * midY * 0.9;
    ctx.lineTo(x, y);
  }
  ctx.lineTo(width, midY);
  ctx.closePath();
  ctx.fill();

  // Draw a secondary lower-opacity waveform for depth
  ctx.strokeStyle = isSonicPi ? 'rgba(255, 221, 0, 0.25)' : isAmber ? 'rgba(255, 102, 0, 0.25)' : 'rgba(100, 200, 255, 0.3)';
  ctx.lineWidth = 1;
  ctx.beginPath();
  for (let x = 0; x < width; x++) {
    const idx = Math.floor(x * step);
    const sample = (waveform[idx] || 0) * 0.7;
    const y = midY - sample * midY * 0.9;
    if (x === 0) {
      ctx.moveTo(x, y);
    } else {
      ctx.lineTo(x, y);
    }
  }
  ctx.stroke();
}

/** Helper to extract theme colors */
function getThemeColors(theme: string) {
  const isSonicPi = theme === 'sonicpi';
  const isAmber = theme === 'amber';
  return {
    waveColor: isSonicPi ? '#ff59b2' : isAmber ? '#ffaa00' : '#00ff88',
    bgTop: isSonicPi ? '#0a0a0a' : isAmber ? '#0f0d08' : '#0d0d2b',
    bgMid: isSonicPi ? '#0e0e0e' : isAmber ? '#12100a' : '#121233',
    gridColor: isSonicPi ? '#141414' : isAmber ? '#1a1710' : '#1a1a40',
    fillR: isSonicPi ? '255, 89, 178' : isAmber ? '255, 170, 0' : '0, 255, 136',
    secondary: isSonicPi ? 'rgba(255, 221, 0, 0.5)' : isAmber ? 'rgba(255, 102, 0, 0.5)' : 'rgba(100, 200, 255, 0.5)',
  };
}

/** Clear canvas with theme gradient */
function clearCanvas(ctx: CanvasRenderingContext2D, width: number, height: number, theme: string) {
  const { bgTop, bgMid } = getThemeColors(theme);
  const gradient = ctx.createLinearGradient(0, 0, 0, height);
  gradient.addColorStop(0, bgTop);
  gradient.addColorStop(0.5, bgMid);
  gradient.addColorStop(1, bgTop);
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, width, height);
}

/** Bars visualization — spectrum-analyzer style like Sonic Pi */
function drawBars(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  waveform: number[],
  theme: string,
) {
  clearCanvas(ctx, width, height, theme);
  const { waveColor, fillR, gridColor } = getThemeColors(theme);

  // Draw horizontal grid lines
  ctx.strokeStyle = gridColor;
  ctx.lineWidth = 0.5;
  for (let i = 1; i < 4; i++) {
    const y = (height / 4) * i;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(width, y);
    ctx.stroke();
  }

  if (!waveform || waveform.length === 0) return;

  // Compute RMS energy in bands (simple spectral approximation from time-domain data)
  const barCount = Math.min(48, Math.floor(width / 8));
  const bandSize = Math.floor(waveform.length / barCount);
  const barWidth = width / barCount;
  const gap = Math.max(1, barWidth * 0.15);
  const topPad = 24; // avoid overlapping the label
  const drawH = height - topPad;

  for (let i = 0; i < barCount; i++) {
    let sum = 0;
    const start = i * bandSize;
    for (let j = start; j < start + bandSize && j < waveform.length; j++) {
      sum += waveform[j] * waveform[j];
    }
    const rms = Math.sqrt(sum / bandSize);
    const barH = Math.min(rms * drawH * 1.8, drawH - 2);

    const x = i * barWidth + gap / 2;
    const w = barWidth - gap;
    const y = height - barH;

    // Bar gradient
    const barGrad = ctx.createLinearGradient(x, y, x, height);
    barGrad.addColorStop(0, waveColor);
    barGrad.addColorStop(1, `rgba(${fillR}, 0.3)`);

    ctx.fillStyle = barGrad;
    ctx.shadowColor = waveColor;
    ctx.shadowBlur = 4;
    ctx.fillRect(x, y, w, barH);
    ctx.shadowBlur = 0;

    // Peak cap
    ctx.fillStyle = waveColor;
    ctx.fillRect(x, y - 2, w, 2);
  }
}

/** Lissajous / stereo scope visualization */
function drawLissajous(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  waveform: number[],
  theme: string,
) {
  clearCanvas(ctx, width, height, theme);
  const { waveColor, gridColor, secondary } = getThemeColors(theme);
  const cx = width / 2;
  const cy = height / 2;
  const radius = Math.min(cx, cy) * 0.85;

  // Draw crosshairs
  ctx.strokeStyle = gridColor;
  ctx.lineWidth = 0.5;
  ctx.beginPath();
  ctx.moveTo(cx, 0);
  ctx.lineTo(cx, height);
  ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(0, cy);
  ctx.lineTo(width, cy);
  ctx.stroke();

  // Draw circle guide
  ctx.beginPath();
  ctx.arc(cx, cy, radius, 0, Math.PI * 2);
  ctx.stroke();

  if (!waveform || waveform.length < 2) return;

  // Treat alternating samples as L/R (or create pseudo-stereo via offset)
  const half = Math.floor(waveform.length / 2);

  ctx.shadowColor = waveColor;
  ctx.shadowBlur = 6;
  ctx.strokeStyle = waveColor;
  ctx.lineWidth = 1.5;
  ctx.beginPath();

  for (let i = 0; i < half; i++) {
    const l = waveform[i] || 0;
    const r = waveform[i + half] || 0;
    const x = cx + l * radius;
    const y = cy - r * radius;
    if (i === 0) {
      ctx.moveTo(x, y);
    } else {
      ctx.lineTo(x, y);
    }
  }
  ctx.stroke();
  ctx.shadowBlur = 0;

  // Draw dots at sparse intervals for sparkle
  ctx.fillStyle = secondary;
  const dotStep = Math.max(1, Math.floor(half / 80));
  for (let i = 0; i < half; i += dotStep) {
    const l = waveform[i] || 0;
    const r = waveform[i + half] || 0;
    const x = cx + l * radius;
    const y = cy - r * radius;
    ctx.beginPath();
    ctx.arc(x, y, 1.5, 0, Math.PI * 2);
    ctx.fill();
  }
}

export default WaveformVisualizer;
