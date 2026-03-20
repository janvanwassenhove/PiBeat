import React, { useRef, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useStore } from '../store';

// ─── Types matching Rust PerformanceSnapshot ────────────────────────────────

interface BandMemberSnapshot {
  role: string;
  animation_state: { type: string; state: string };
  animation_progress: number;
  energy: number;
}

interface StageLighting {
  brightness: number;
  strobe_active: boolean;
  spotlight_color: [number, number, number];
  beat_flash: number;
}

interface CrowdState {
  excitement: number;
  jumping_count: number;
  wave_active: boolean;
}

interface PerformanceSnapshot {
  band: BandMemberSnapshot[];
  lighting: StageLighting;
  crowd: CrowdState;
  energy: number;
  bpm: number;
  beat_position: number;
  is_playing: boolean;
  frame: number;
}

// ─── Pixel Art Colors ───────────────────────────────────────────────────────

const PALETTE = {
  bg: '#0d0d1a',
  stage: '#1a1a2e',
  stageFloor: '#2a1a3a',
  stageEdge: '#3a2a4a',
  // Band member base colors
  drummer: '#ff6b6b',
  bassist: '#4ecdc4',
  guitarist: '#ffe66d',
  keyboard: '#a78bfa',
  vocalist: '#f472b6',
  // UI
  crowd: '#2a2a4a',
  crowdExcited: '#4a3a6a',
  text: '#888',
  textBright: '#ccc',
  idle: '#333344',
  grid: '#1a1a30',
};

// ─── Pixel Drawing Helpers ──────────────────────────────────────────────────

/** Draw a filled pixel rectangle (scaled by pixel size) */
function pxRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, color: string, px: number) {
  ctx.fillStyle = color;
  ctx.fillRect(Math.round(x * px), Math.round(y * px), Math.round(w * px), Math.round(h * px));
}

/** Draw a simple pixel-art character (stick figure style) */
function drawCharacter(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  color: string,
  energy: number,
  animState: string,
  px: number,
  frame: number,
) {
  const bounce = animState !== 'idle' ? Math.sin(frame * 0.3) * energy * 2 : 0;
  const y = cy + bounce;

  // Head
  pxRect(ctx, cx - 1, y - 6, 3, 3, color, px);

  // Body
  pxRect(ctx, cx, y - 3, 1, 4, color, px);

  // Arms
  const armBounce = animState === 'intense' || animState === 'solo' || animState === 'play_hard'
    ? Math.sin(frame * 0.5) * 2
    : animState === 'groove' || animState === 'accent' || animState === 'play_soft'
      ? Math.sin(frame * 0.2) * 1
      : 0;
  pxRect(ctx, cx - 3, y - 2 + armBounce, 2, 1, color, px);
  pxRect(ctx, cx + 2, y - 2 - armBounce, 2, 1, color, px);

  // Legs
  const legSpread = animState !== 'idle' ? Math.abs(Math.sin(frame * 0.15)) * 1 : 0;
  pxRect(ctx, cx - 1 - legSpread, y + 1, 1, 3, color, px);
  pxRect(ctx, cx + 1 + legSpread, y + 1, 1, 3, color, px);

  // Energy glow
  if (energy > 0.3) {
    ctx.globalAlpha = energy * 0.3;
    pxRect(ctx, cx - 3, y - 7, 7, 12, color, px);
    ctx.globalAlpha = 1.0;
  }
}

/** Draw the drummer with kit */
function drawDrummer(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  energy: number,
  animState: string,
  px: number,
  frame: number,
) {
  // Drum kit (static)
  pxRect(ctx, cx - 5, cy + 2, 3, 2, '#444', px); // left drum
  pxRect(ctx, cx + 3, cy + 2, 3, 2, '#444', px); // right drum
  pxRect(ctx, cx - 1, cy + 1, 3, 3, '#555', px); // center drum (snare)
  pxRect(ctx, cx - 3, cy - 1, 1, 1, '#666', px); // hi-hat
  pxRect(ctx, cx + 4, cy - 2, 2, 1, '#777', px); // cymbal

  // Cymbal flash on crash hit
  if (animState === 'crash_hit') {
    ctx.globalAlpha = 0.8;
    pxRect(ctx, cx + 3, cy - 3, 4, 2, '#ffff00', px);
    ctx.globalAlpha = 1.0;
  }

  // Sticks animation
  const stickAngle = animState === 'play_hard' || animState === 'fill'
    ? Math.sin(frame * 0.6) * 3
    : animState === 'play_soft'
      ? Math.sin(frame * 0.3) * 1.5
      : 0;
  
  // Character
  const bounce = animState !== 'idle' ? Math.sin(frame * 0.25) * energy * 1.5 : 0;
  const headY = cy - 6 + bounce;

  // Head
  pxRect(ctx, cx - 1, headY, 3, 3, PALETTE.drummer, px);
  // Body
  pxRect(ctx, cx, headY + 3, 1, 3, PALETTE.drummer, px);
  // Arms with sticks
  pxRect(ctx, cx - 3, headY + 4 + stickAngle, 3, 1, PALETTE.drummer, px);
  pxRect(ctx, cx + 1, headY + 4 - stickAngle, 3, 1, PALETTE.drummer, px);

  // Fill flash
  if (animState === 'fill') {
    ctx.globalAlpha = 0.4;
    pxRect(ctx, cx - 6, cy - 2, 13, 6, '#ff6b6b', px);
    ctx.globalAlpha = 1.0;
  }
}

/** Draw crowd members */
function drawCrowd(
  ctx: CanvasRenderingContext2D,
  startX: number,
  y: number,
  width: number,
  crowd: CrowdState,
  px: number,
  frame: number,
) {
  const count = 20;
  const spacing = width / count;
  
  for (let i = 0; i < count; i++) {
    const x = startX + i * spacing;
    const isJumping = i < crowd.jumping_count;
    const jumpOffset = isJumping ? Math.abs(Math.sin((frame + i * 0.5) * 0.2)) * 3 : 0;
    const waveOffset = crowd.wave_active ? Math.sin((frame * 0.1) + i * 0.5) * 2 : 0;
    const finalY = y - jumpOffset - waveOffset;

    const brightness = Math.floor(40 + crowd.excitement * 60);
    const color = `rgb(${brightness}, ${brightness}, ${brightness + 20})`;

    // Simple pixel head
    pxRect(ctx, x, finalY, 2, 2, color, px);
    // Body
    pxRect(ctx, x, finalY + 2, 2, 2, color, px);
    
    // Arms up when excited
    if (isJumping && crowd.excitement > 0.5) {
      const armUp = Math.sin((frame + i) * 0.3) > 0;
      if (armUp) {
        pxRect(ctx, x - 1, finalY - 1, 1, 1, color, px);
        pxRect(ctx, x + 2, finalY - 1, 1, 1, color, px);
      }
    }
  }
}

/** Draw stage lights */
function drawStageLights(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  lighting: StageLighting,
  px: number,
  _frame: number,
) {
  const [r, g, b] = lighting.spotlight_color;

  // Spotlight cones from top
  const coneCount = 3;
  for (let i = 0; i < coneCount; i++) {
    const x = (width / (coneCount + 1)) * (i + 1);
    ctx.globalAlpha = lighting.brightness * 0.15;
    
    ctx.beginPath();
    ctx.moveTo(x * px - 2 * px, 0);
    ctx.lineTo((x - 8) * px, height * px * 0.6);
    ctx.lineTo((x + 8) * px, height * px * 0.6);
    ctx.closePath();
    ctx.fillStyle = `rgb(${r}, ${g}, ${b})`;
    ctx.fill();
    ctx.globalAlpha = 1.0;
  }

  // Beat flash overlay
  if (lighting.beat_flash > 0.1) {
    ctx.globalAlpha = lighting.beat_flash * 0.2;
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, width * px, height * px);
    ctx.globalAlpha = 1.0;
  }

  // Strobe
  if (lighting.strobe_active) {
    ctx.globalAlpha = 0.3;
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, width * px, height * px);
    ctx.globalAlpha = 1.0;
  }
}

// ─── Component ──────────────────────────────────────────────────────────────

const BandVisualizer: React.FC = () => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const snapshotRef = useRef<PerformanceSnapshot | null>(null);
  const localFrameRef = useRef<number>(0);
  const { isPlaying, showBandVisualizer, theme } = useStore();

  // Poll the Rust visual engine for snapshots and render
  const render = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Virtual pixel grid dimensions (we'll scale up)
    const GRID_W = 128;
    const GRID_H = 80;
    
    // Calculate pixel size to fit the canvas
    const px = Math.max(1, Math.min(
      Math.floor(canvas.width / GRID_W),
      Math.floor(canvas.height / GRID_H)
    ));

    const actualW = GRID_W * px;
    const actualH = GRID_H * px;
    const offsetX = Math.floor((canvas.width - actualW) / 2);
    const offsetY = Math.floor((canvas.height - actualH) / 2);

    ctx.save();
    ctx.translate(offsetX, offsetY);

    const snap = snapshotRef.current;
    const frame = localFrameRef.current;
    localFrameRef.current++;

    // ── Background ──────────────────────────────────────────
    const isSonicPi = theme === 'sonicpi';
    const isAmber = theme === 'amber';
    const bgColor = isSonicPi ? '#0a0a0a' : isAmber ? '#0f0d08' : PALETTE.bg;
    
    ctx.fillStyle = bgColor;
    ctx.fillRect(0, 0, actualW, actualH);

    // Stage floor
    const stageY = 50;
    const floorColor = isSonicPi ? '#1a0a15' : isAmber ? '#1a1508' : PALETTE.stageFloor;
    pxRect(ctx, 2, stageY, GRID_W - 4, 2, 
      isSonicPi ? '#2a1525' : isAmber ? '#2a2010' : PALETTE.stageEdge, px);
    pxRect(ctx, 0, stageY + 2, GRID_W, GRID_H - stageY - 2, floorColor, px);

    // Stage grid lines (retro)
    const gridColor = isSonicPi ? '#120a10' : isAmber ? '#15120a' : PALETTE.grid;
    for (let i = 0; i < GRID_W; i += 8) {
      pxRect(ctx, i, stageY + 2, 1, GRID_H - stageY - 2, gridColor, px);
    }

    if (!snap || !snap.is_playing) {
      // Idle state — draw dim band
      const positions = [
        { x: 20, label: 'DRUMS' },
        { x: 40, label: 'BASS' },
        { x: 64, label: 'GUITAR' },
        { x: 84, label: 'KEYS' },
        { x: 104, label: 'VOX' },
      ];
      for (const p of positions) {
        drawCharacter(ctx, p.x, stageY - 2, PALETTE.idle, 0, 'idle', px, 0);
        // Label
        ctx.fillStyle = PALETTE.text;
        ctx.font = `${Math.max(8, px * 3)}px monospace`;
        ctx.textAlign = 'center';
        ctx.fillText(p.label, p.x * px, (stageY + 8) * px);
      }

      // "Press Run to start" text
      ctx.fillStyle = PALETTE.textBright;
      ctx.font = `${Math.max(10, px * 4)}px monospace`;
      ctx.textAlign = 'center';
      ctx.fillText(
        isPlaying ? 'Waiting for events...' : '♪ Press Run to start ♪',
        (GRID_W / 2) * px,
        25 * px
      );

      ctx.restore();
      return;
    }

    // ── Active Performance ──────────────────────────────────

    // Stage lights
    drawStageLights(ctx, GRID_W, GRID_H, snap.lighting, px, frame);

    // Band members
    const memberPositions: Record<string, { x: number; color: string; label: string }> = {
      drummer: { x: 20, color: PALETTE.drummer, label: 'DRUMS' },
      bassist: { x: 40, color: PALETTE.bassist, label: 'BASS' },
      guitarist: { x: 64, color: PALETTE.guitarist, label: 'GUITAR' },
      keyboard: { x: 84, color: PALETTE.keyboard, label: 'KEYS' },
      vocalist: { x: 104, color: PALETTE.vocalist, label: 'VOX' },
    };

    for (const member of snap.band) {
      const pos = memberPositions[member.role];
      if (!pos) continue;

      const state = member.animation_state.state;

      if (member.role === 'drummer') {
        drawDrummer(ctx, pos.x, stageY - 2, member.energy, state, px, frame);
      } else {
        drawCharacter(ctx, pos.x, stageY - 2, pos.color, member.energy, state, px, frame);
      }

      // Member label
      ctx.fillStyle = member.energy > 0.3 ? pos.color : PALETTE.text;
      ctx.font = `${Math.max(7, px * 3)}px monospace`;
      ctx.textAlign = 'center';
      ctx.fillText(pos.label, pos.x * px, (stageY + 8) * px);

      // Energy bar
      const barW = 8;
      const barH = 1;
      const barX = pos.x - barW / 2;
      const barY = stageY + 10;
      pxRect(ctx, barX, barY, barW, barH, '#222', px);
      pxRect(ctx, barX, barY, Math.round(barW * member.energy), barH, pos.color, px);
    }

    // ── Crowd ───────────────────────────────────────────────
    drawCrowd(ctx, 4, GRID_H - 8, GRID_W - 8, snap.crowd, px, frame);

    // ── HUD overlay ─────────────────────────────────────────
    const accentColor = isSonicPi ? '#ff59b2' : isAmber ? '#ffaa00' : '#00ff88';
    
    // Energy bar (top)
    const energyBarW = GRID_W - 8;
    pxRect(ctx, 4, 2, energyBarW, 2, '#111', px);
    pxRect(ctx, 4, 2, Math.round(energyBarW * snap.energy), 2, accentColor, px);

    // BPM / Beat indicator
    ctx.fillStyle = PALETTE.textBright;
    ctx.font = `${Math.max(8, px * 3)}px monospace`;
    ctx.textAlign = 'left';
    ctx.fillText(`${Math.round(snap.bpm)} BPM`, 4 * px, 9 * px);

    // Beat dots (4/4 time)
    const beatInBar = Math.floor(snap.beat_position);
    for (let i = 0; i < 4; i++) {
      const dotX = GRID_W - 14 + i * 3;
      pxRect(ctx, dotX, 3, 2, 2, i <= beatInBar ? accentColor : '#333', px);
    }

    ctx.restore();
  }, [isPlaying, theme]);

  // Polling loop: fetch snapshot + render
  useEffect(() => {
    if (!showBandVisualizer) return;

    let running = true;
    let pollTimer: ReturnType<typeof setTimeout>;

    const poll = async () => {
      if (!running) return;
      try {
        const snap = await invoke<PerformanceSnapshot>('get_visual_snapshot');
        snapshotRef.current = snap;
      } catch {
        // Visual engine not available — silently ignore
      }
      render();
      // Poll at ~30fps (33ms) — visual system is best-effort
      if (running) {
        pollTimer = setTimeout(poll, 33);
      }
    };

    poll();

    return () => {
      running = false;
      clearTimeout(pollTimer);
    };
  }, [showBandVisualizer, render]);

  // Canvas resize observer
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const resizeObserver = new ResizeObserver(() => {
      const rect = canvas.getBoundingClientRect();
      canvas.width = rect.width * window.devicePixelRatio;
      canvas.height = rect.height * window.devicePixelRatio;
    });

    resizeObserver.observe(canvas);
    // Initial size
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * window.devicePixelRatio;
    canvas.height = rect.height * window.devicePixelRatio;

    return () => resizeObserver.disconnect();
  }, []);

  if (!showBandVisualizer) return null;

  return (
    <div className="band-visualizer">
      <canvas
        ref={canvasRef}
        className="band-visualizer-canvas"
        style={{ width: '100%', height: '100%', imageRendering: 'pixelated' }}
      />
    </div>
  );
};

export default BandVisualizer;
