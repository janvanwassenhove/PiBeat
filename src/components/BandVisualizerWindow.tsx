import React, { useRef, useEffect, useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

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

type DanceStyle = 'bounce' | 'headbang' | 'sway' | 'robot' | 'funk' | 'rave';
type VisualEffect = 'scanlines' | 'pixel_rain' | 'star_field' | 'fire_trails' | 'mirror_ball' | 'neon_glow';
type StageDecor = 'retro_stage' | 'oscilloscope' | 'space_scene' | 'city_night' | 'matrix' | 'underwater';
type CameraMode = 'full_stage' | 'stage_view' | 'close_up' | 'zoom_character' | 'auto';

interface PerformanceSnapshot {
  band: BandMemberSnapshot[];
  lighting: StageLighting;
  crowd: CrowdState;
  energy: number;
  bpm: number;
  beat_position: number;
  is_playing: boolean;
  frame: number;
  dance_style: DanceStyle;
  active_effects: VisualEffect[];
  decor: StageDecor;
  camera_mode: CameraMode;
  camera_focus: string | null;
  visible_members: Record<string, boolean>;
}

// ─── Pixel Art Colors ───────────────────────────────────────────────────────

const PALETTE = {
  bg: '#0d0d1a',
  stage: '#1a1a2e',
  stageFloor: '#2a1a3a',
  stageEdge: '#3a2a4a',
  drummer: '#ff6b6b',
  bassist: '#4ecdc4',
  guitarist: '#ffe66d',
  keyboard: '#a78bfa',
  vocalist: '#f472b6',
  dj: '#00d4ff',
  percussionist: '#ff9f43',
  crowd: '#2a2a4a',
  crowdExcited: '#4a3a6a',
  text: '#888',
  textBright: '#ccc',
  idle: '#333344',
  grid: '#1a1a30',
};

// ─── Member layout (7 roles spread across stage) ────────────────────────────

const MEMBER_POSITIONS: Record<string, { x: number; color: string; label: string }> = {
  drummer:       { x: 12,  color: PALETTE.drummer,       label: 'DRUMS' },
  percussionist: { x: 28,  color: PALETTE.percussionist, label: 'PERC' },
  bassist:       { x: 44,  color: PALETTE.bassist,       label: 'BASS' },
  guitarist:     { x: 60,  color: PALETTE.guitarist,     label: 'GUITAR' },
  keyboard:      { x: 76,  color: PALETTE.keyboard,      label: 'KEYS' },
  vocalist:      { x: 92,  color: PALETTE.vocalist,      label: 'VOX' },
  dj:            { x: 110, color: PALETTE.dj,            label: 'DJ' },
};

// ─── Pixel Drawing ──────────────────────────────────────────────────────────

function pxRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, color: string, px: number) {
  ctx.fillStyle = color;
  ctx.fillRect(Math.round(x * px), Math.round(y * px), Math.round(w * px), Math.round(h * px));
}

// ─── Dance Style Animations ─────────────────────────────────────────────────

function getDanceOffset(
  style: DanceStyle,
  energy: number,
  animState: string,
  frame: number,
): { dx: number; dy: number; armL: number; armR: number; legSpread: number; headTilt: number } {
  const isIdle = animState === 'idle';
  const isIntense = animState === 'intense' || animState === 'solo' || animState === 'play_hard' || animState === 'fill';
  const e = isIdle ? 0 : energy;

  switch (style) {
    case 'bounce': {
      const dy = isIdle ? 0 : Math.sin(frame * 0.3) * e * 2;
      const armL = isIntense ? Math.sin(frame * 0.5) * 2 : e > 0 ? Math.sin(frame * 0.2) * 1 : 0;
      return { dx: 0, dy, armL, armR: -armL, legSpread: isIdle ? 0 : Math.abs(Math.sin(frame * 0.15)) * 1, headTilt: 0 };
    }
    case 'headbang': {
      const dy = isIdle ? 0 : Math.abs(Math.sin(frame * 0.4)) * e * 3;
      const headTilt = isIdle ? 0 : Math.sin(frame * 0.4) * e * 2;
      const armL = isIntense ? -2 + Math.sin(frame * 0.4) * 1 : 0;
      return { dx: 0, dy, armL, armR: armL, legSpread: 0.5, headTilt };
    }
    case 'sway': {
      const dx = isIdle ? 0 : Math.sin(frame * 0.12) * e * 3;
      const armL = Math.sin(frame * 0.12) * e * 1.5;
      return { dx, dy: 0, armL, armR: armL, legSpread: 0, headTilt: dx * 0.3 };
    }
    case 'robot': {
      const step = Math.floor(frame * 0.15) % 4;
      const dy = isIdle ? 0 : (step % 2 === 0 ? -1 : 1) * e;
      const armL = isIdle ? 0 : (step < 2 ? 2 : -2) * e;
      return { dx: 0, dy, armL, armR: -armL, legSpread: step % 2, headTilt: 0 };
    }
    case 'funk': {
      const dx = isIdle ? 0 : Math.sin(frame * 0.18 + 0.3) * e * 2;
      const dy = isIdle ? 0 : Math.abs(Math.sin(frame * 0.36)) * e * 1.5;
      const armL = Math.sin(frame * 0.36) * e * 2;
      return { dx, dy, armL, armR: Math.cos(frame * 0.36) * e * 2, legSpread: Math.abs(Math.sin(frame * 0.18)) * 1.5, headTilt: dx * 0.2 };
    }
    case 'rave': {
      const jump = isIdle ? 0 : Math.max(0, Math.sin(frame * 0.35)) * e * 4;
      const armUp = isIntense ? -3 - Math.abs(Math.sin(frame * 0.5)) * 2 : -Math.abs(Math.sin(frame * 0.35)) * e * 2;
      return { dx: 0, dy: -jump, armL: armUp, armR: armUp, legSpread: 0.5, headTilt: 0 };
    }
    default:
      return { dx: 0, dy: 0, armL: 0, armR: 0, legSpread: 0, headTilt: 0 };
  }
}

function drawCharacter(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  color: string,
  energy: number,
  animState: string,
  px: number,
  frame: number,
  danceStyle: DanceStyle,
) {
  const d = getDanceOffset(danceStyle, energy, animState, frame);
  const x = cx + d.dx;
  const y = cy + d.dy;

  // Head
  pxRect(ctx, x - 1 + d.headTilt * 0.3, y - 6, 3, 3, color, px);
  // Body
  pxRect(ctx, x, y - 3, 1, 4, color, px);
  // Arms
  pxRect(ctx, x - 3, y - 2 + d.armL, 2, 1, color, px);
  pxRect(ctx, x + 2, y - 2 + d.armR, 2, 1, color, px);
  // Legs
  pxRect(ctx, x - 1 - d.legSpread, y + 1, 1, 3, color, px);
  pxRect(ctx, x + 1 + d.legSpread, y + 1, 1, 3, color, px);

  // Energy glow
  if (energy > 0.3) {
    ctx.globalAlpha = energy * 0.3;
    pxRect(ctx, x - 3, y - 7, 7, 12, color, px);
    ctx.globalAlpha = 1.0;
  }
}

function drawDrummer(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  energy: number,
  animState: string,
  px: number,
  frame: number,
  danceStyle: DanceStyle,
  drumColor: string,
) {
  // Kit
  pxRect(ctx, cx - 5, cy + 2, 3, 2, '#444', px);
  pxRect(ctx, cx + 3, cy + 2, 3, 2, '#444', px);
  pxRect(ctx, cx - 1, cy + 1, 3, 3, '#555', px);
  pxRect(ctx, cx - 3, cy - 1, 1, 1, '#666', px);
  pxRect(ctx, cx + 4, cy - 2, 2, 1, '#777', px);

  if (animState === 'crash_hit') {
    ctx.globalAlpha = 0.8;
    pxRect(ctx, cx + 3, cy - 3, 4, 2, '#ffff00', px);
    ctx.globalAlpha = 1.0;
  }

  const d = getDanceOffset(danceStyle, energy, animState, frame);

  const stickAngle = animState === 'play_hard' || animState === 'fill'
    ? Math.sin(frame * 0.6) * 3
    : animState === 'play_soft'
      ? Math.sin(frame * 0.3) * 1.5
      : 0;

  const headY = cy - 6 + d.dy;
  pxRect(ctx, cx - 1 + d.headTilt * 0.3, headY, 3, 3, drumColor, px);
  pxRect(ctx, cx, headY + 3, 1, 3, drumColor, px);
  pxRect(ctx, cx - 3, headY + 4 + stickAngle, 3, 1, drumColor, px);
  pxRect(ctx, cx + 1, headY + 4 - stickAngle, 3, 1, drumColor, px);

  if (animState === 'fill') {
    ctx.globalAlpha = 0.4;
    pxRect(ctx, cx - 6, cy - 2, 13, 6, drumColor, px);
    ctx.globalAlpha = 1.0;
  }
}

function drawDJ(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  energy: number,
  animState: string,
  px: number,
  frame: number,
  danceStyle: DanceStyle,
) {
  // Turntable / deck
  pxRect(ctx, cx - 4, cy + 1, 9, 3, '#333', px);
  pxRect(ctx, cx - 3, cy + 1, 3, 2, '#555', px);
  pxRect(ctx, cx + 1, cy + 1, 3, 2, '#555', px);
  // Spinning record indicator
  if (animState !== 'idle') {
    const spin = Math.floor(frame * 0.3) % 2;
    pxRect(ctx, cx - 2 + spin, cy + 2, 1, 1, PALETTE.dj, px);
    pxRect(ctx, cx + 2 - spin, cy + 2, 1, 1, PALETTE.dj, px);
  }

  const d = getDanceOffset(danceStyle, energy, animState, frame);
  const y = cy + d.dy;

  // Head with headphones
  pxRect(ctx, cx - 1 + d.headTilt * 0.3, y - 6, 3, 3, PALETTE.dj, px);
  pxRect(ctx, cx - 2, y - 5, 1, 1, '#888', px); // headphone L
  pxRect(ctx, cx + 2, y - 5, 1, 1, '#888', px); // headphone R
  // Body
  pxRect(ctx, cx, y - 3, 1, 4, PALETTE.dj, px);
  // Arms reaching to deck
  const scratchAnim = animState !== 'idle' ? Math.sin(frame * 0.4) * 1 : 0;
  pxRect(ctx, cx - 3, y - 1 + scratchAnim, 2, 1, PALETTE.dj, px);
  pxRect(ctx, cx + 2, y - 1 - scratchAnim, 2, 1, PALETTE.dj, px);

  if (energy > 0.3) {
    ctx.globalAlpha = energy * 0.25;
    pxRect(ctx, cx - 4, y - 7, 9, 11, PALETTE.dj, px);
    ctx.globalAlpha = 1.0;
  }
}

// ─── Decor / Backdrop Renderers ─────────────────────────────────────────────

function drawDecorRetroStage(
  ctx: CanvasRenderingContext2D,
  GRID_W: number,
  GRID_H: number,
  stageY: number,
  px: number,
) {
  pxRect(ctx, 2, stageY, GRID_W - 4, 2, PALETTE.stageEdge, px);
  pxRect(ctx, 0, stageY + 2, GRID_W, GRID_H - stageY - 2, PALETTE.stageFloor, px);
  for (let i = 0; i < GRID_W; i += 8) {
    pxRect(ctx, i, stageY + 2, 1, GRID_H - stageY - 2, PALETTE.grid, px);
  }
}

function drawDecorOscilloscope(
  ctx: CanvasRenderingContext2D,
  GRID_W: number,
  GRID_H: number,
  stageY: number,
  px: number,
  frame: number,
  energy: number,
) {
  // Dark bg with waveform
  pxRect(ctx, 0, 0, GRID_W, stageY, '#050510', px);
  const midY = stageY / 2;
  ctx.strokeStyle = '#00ff88';
  ctx.lineWidth = px;
  ctx.globalAlpha = 0.5 + energy * 0.5;
  ctx.beginPath();
  for (let i = 0; i < GRID_W; i++) {
    const y = midY + Math.sin((i + frame) * 0.15) * energy * 12 + Math.sin((i + frame * 0.7) * 0.3) * energy * 5;
    const screenX = i * px;
    const screenY = y * px;
    if (i === 0) ctx.moveTo(screenX, screenY);
    else ctx.lineTo(screenX, screenY);
  }
  ctx.stroke();
  ctx.globalAlpha = 1.0;

  // Stage floor
  pxRect(ctx, 0, stageY, GRID_W, GRID_H - stageY, '#0a0a18', px);
  for (let i = 0; i < GRID_W; i += 12) {
    pxRect(ctx, i, stageY, 1, GRID_H - stageY, '#0f0f25', px);
  }
}

function drawDecorSpaceScene(
  ctx: CanvasRenderingContext2D,
  GRID_W: number,
  GRID_H: number,
  stageY: number,
  px: number,
  frame: number,
  energy: number,
) {
  pxRect(ctx, 0, 0, GRID_W, GRID_H, '#020208', px);

  // Stars
  for (let i = 0; i < 60; i++) {
    const sx = ((i * 137 + 31) % GRID_W);
    const sy = ((i * 97 + 13) % (stageY - 4));
    const twinkle = Math.sin(frame * 0.1 + i) * 0.5 + 0.5;
    const bright = Math.floor(80 + twinkle * 175);
    pxRect(ctx, sx, sy, 1, 1, `rgb(${bright},${bright},${bright + 20})`, px);
  }

  // Nebula glow
  if (energy > 0.2) {
    ctx.globalAlpha = energy * 0.15;
    const nebX = 30 + Math.sin(frame * 0.02) * 15;
    const nebY = 15 + Math.cos(frame * 0.03) * 5;
    ctx.beginPath();
    ctx.arc(nebX * px, nebY * px, 20 * px, 0, Math.PI * 2);
    ctx.fillStyle = '#6a1b9a';
    ctx.fill();
    ctx.globalAlpha = 1.0;
  }

  // Floor platform
  ctx.globalAlpha = 0.4;
  pxRect(ctx, 4, stageY, GRID_W - 8, 2, '#334', px);
  ctx.globalAlpha = 1.0;
  pxRect(ctx, 0, stageY + 2, GRID_W, GRID_H - stageY - 2, '#0a0a14', px);
}

function drawDecorCityNight(
  ctx: CanvasRenderingContext2D,
  GRID_W: number,
  GRID_H: number,
  stageY: number,
  px: number,
  frame: number,
  energy: number,
) {
  pxRect(ctx, 0, 0, GRID_W, stageY, '#0a0a1a', px);

  const buildings = [
    { x: 2, w: 8, h: 18 }, { x: 12, w: 6, h: 24 }, { x: 20, w: 10, h: 20 },
    { x: 32, w: 7, h: 28 }, { x: 42, w: 12, h: 16 }, { x: 56, w: 8, h: 22 },
    { x: 66, w: 6, h: 30 }, { x: 74, w: 10, h: 19 }, { x: 86, w: 8, h: 25 },
    { x: 96, w: 12, h: 17 }, { x: 110, w: 8, h: 21 }, { x: 120, w: 6, h: 26 },
  ];

  for (const b of buildings) {
    const by = stageY - b.h;
    pxRect(ctx, b.x, by, b.w, b.h, '#151525', px);
    for (let wy = by + 2; wy < stageY - 2; wy += 4) {
      for (let wx = b.x + 1; wx < b.x + b.w - 1; wx += 3) {
        const on = ((wx * 7 + wy * 13 + Math.floor(frame * 0.02)) % 5) > 1;
        if (on) {
          const hue = (wx * 31 + wy * 17) % 360;
          pxRect(ctx, wx, wy, 1, 1, `hsl(${hue}, 80%, ${40 + energy * 30}%)`, px);
        }
      }
    }
  }

  pxRect(ctx, 0, stageY, GRID_W, GRID_H - stageY, '#111118', px);
  for (let i = 0; i < GRID_W; i += 10) {
    pxRect(ctx, i + ((frame * 0.5) % 10), stageY + 1, 4, 1, '#333', px);
  }
}

function drawDecorMatrix(
  ctx: CanvasRenderingContext2D,
  GRID_W: number,
  GRID_H: number,
  stageY: number,
  px: number,
  frame: number,
  energy: number,
) {
  pxRect(ctx, 0, 0, GRID_W, GRID_H, '#000800', px);

  const cols = 20;
  const colW = Math.floor(GRID_W / cols);
  for (let c = 0; c < cols; c++) {
    const speed = 0.1 + (c % 3) * 0.05 + energy * 0.1;
    const headY = ((frame * speed + c * 7) % (stageY + 10)) - 5;
    const len = 6 + (c % 4) * 2;
    for (let j = 0; j < len; j++) {
      const cy = headY - j;
      if (cy < 0 || cy >= stageY) continue;
      const alpha = (1 - j / len) * (0.4 + energy * 0.6);
      const g = j === 0 ? 255 : Math.floor(100 + (1 - j / len) * 100);
      ctx.globalAlpha = alpha;
      pxRect(ctx, c * colW + 1, cy, 1, 1, `rgb(0,${g},0)`, px);
    }
  }
  ctx.globalAlpha = 1.0;

  pxRect(ctx, 0, stageY, GRID_W, GRID_H - stageY, '#000a00', px);
}

function drawDecorUnderwater(
  ctx: CanvasRenderingContext2D,
  GRID_W: number,
  GRID_H: number,
  stageY: number,
  px: number,
  frame: number,
  energy: number,
) {
  const topB = 15, botB = 40;
  for (let y = 0; y < GRID_H; y += 2) {
    const t = y / GRID_H;
    const b = Math.floor(topB + t * (botB - topB));
    pxRect(ctx, 0, y, GRID_W, 2, `rgb(0,${Math.floor(b * 0.4)},${b})`, px);
  }

  // Bubbles
  for (let i = 0; i < 12; i++) {
    const bx = (i * 11 + 3) % GRID_W;
    const speed = 0.15 + (i % 3) * 0.05 + energy * 0.1;
    const by = GRID_H - ((frame * speed + i * 8) % (GRID_H + 5));
    if (by < 0) continue;
    const size = 1 + (i % 2);
    ctx.globalAlpha = 0.3 + energy * 0.3;
    pxRect(ctx, bx, by, size, size, '#6af', px);
  }
  ctx.globalAlpha = 1.0;

  pxRect(ctx, 0, stageY + 2, GRID_W, GRID_H - stageY - 2, '#1a1a10', px);
  // Seaweed
  for (let i = 0; i < 6; i++) {
    const sx = 10 + i * 20;
    const sway = Math.sin(frame * 0.08 + i) * 2;
    for (let j = 0; j < 5; j++) {
      pxRect(ctx, sx + sway * (j / 5), stageY + 2 - j, 1, 1, '#2a6a2a', px);
    }
  }
}

function drawDecor(
  ctx: CanvasRenderingContext2D,
  decor: StageDecor,
  GRID_W: number,
  GRID_H: number,
  stageY: number,
  px: number,
  frame: number,
  energy: number,
) {
  switch (decor) {
    case 'retro_stage':  drawDecorRetroStage(ctx, GRID_W, GRID_H, stageY, px); break;
    case 'oscilloscope': drawDecorOscilloscope(ctx, GRID_W, GRID_H, stageY, px, frame, energy); break;
    case 'space_scene':  drawDecorSpaceScene(ctx, GRID_W, GRID_H, stageY, px, frame, energy); break;
    case 'city_night':   drawDecorCityNight(ctx, GRID_W, GRID_H, stageY, px, frame, energy); break;
    case 'matrix':       drawDecorMatrix(ctx, GRID_W, GRID_H, stageY, px, frame, energy); break;
    case 'underwater':   drawDecorUnderwater(ctx, GRID_W, GRID_H, stageY, px, frame, energy); break;
    default:             drawDecorRetroStage(ctx, GRID_W, GRID_H, stageY, px);
  }
}

// ─── Visual Effects (Post-processing) ───────────────────────────────────────

function fxScanlines(ctx: CanvasRenderingContext2D, w: number, h: number, energy: number) {
  ctx.globalAlpha = 0.08 + energy * 0.07;
  for (let y = 0; y < h; y += 3) {
    ctx.fillStyle = '#000';
    ctx.fillRect(0, y, w, 1);
  }
  ctx.globalAlpha = 1.0;
}

function fxPixelRain(ctx: CanvasRenderingContext2D, w: number, h: number, frame: number, energy: number, px: number) {
  const cols = 30;
  const colW = w / cols;
  for (let c = 0; c < cols; c++) {
    const speed = 2 + (c % 4) * 0.8 + energy * 3;
    const headY = ((frame * speed + c * 37) % (h + 100)) - 50;
    for (let j = 0; j < 5; j++) {
      const ry = headY - j * px * 2;
      if (ry < 0 || ry >= h) continue;
      const alpha = (1 - j / 5);
      ctx.globalAlpha = alpha * (0.1 + energy * 0.15);
      ctx.fillStyle = '#0f0';
      ctx.fillRect(c * colW, ry, px, px);
    }
  }
  ctx.globalAlpha = 1.0;
}

function fxStarField(ctx: CanvasRenderingContext2D, w: number, h: number, frame: number, energy: number) {
  ctx.globalAlpha = 0.4 + energy * 0.4;
  for (let i = 0; i < 30; i++) {
    const speed = 0.5 + (i % 3) * 0.5 + energy * 1.5;
    const sx = ((i * 137 + frame * speed * (1 + i % 3)) % w);
    const sy = ((i * 97 + 13) % h);
    const size = 1 + (i % 2);
    const twinkle = Math.sin(frame * 0.15 + i * 2) * 0.5 + 0.5;
    ctx.fillStyle = `rgba(255,255,255,${twinkle})`;
    ctx.fillRect(sx, sy, size, size);
  }
  ctx.globalAlpha = 1.0;
}

function fxFireTrails(ctx: CanvasRenderingContext2D, w: number, h: number, frame: number, energy: number, px: number) {
  if (energy < 0.1) return;
  const stageBottom = h * 0.7;
  ctx.globalAlpha = energy * 0.5;
  for (let i = 0; i < 15; i++) {
    const fx = ((i * 41 + 7) % Math.floor(w / px)) * px;
    const rise = (frame * (1 + i % 3) * 0.5 + i * 11) % (h * 0.4);
    const fy = stageBottom - rise;
    if (fy < 0) continue;
    const life = 1 - rise / (h * 0.4);
    const r = Math.floor(255 * life);
    const g = Math.floor(100 * life + 50 * (1 - life));
    ctx.fillStyle = `rgb(${r},${g},0)`;
    ctx.fillRect(fx, fy, px, px * (1 + Math.floor(life * 2)));
  }
  ctx.globalAlpha = 1.0;
}

function fxMirrorBall(ctx: CanvasRenderingContext2D, w: number, h: number, frame: number, energy: number) {
  if (energy < 0.1) return;
  ctx.globalAlpha = energy * 0.35;
  const count = Math.floor(8 + energy * 12);
  for (let i = 0; i < count; i++) {
    const angle = (frame * 0.05 + i * (Math.PI * 2 / count));
    const dist = 30 + Math.sin(frame * 0.03 + i) * 20;
    const rx = w / 2 + Math.cos(angle) * dist * (w / 200);
    const ry = h * 0.3 + Math.sin(angle) * dist * (h / 200);
    const hue = (i * 40 + frame * 2) % 360;
    ctx.fillStyle = `hsl(${hue}, 80%, 70%)`;
    ctx.beginPath();
    ctx.arc(rx, ry, 2 + energy * 3, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.globalAlpha = 1.0;
}

function fxNeonGlow(ctx: CanvasRenderingContext2D, w: number, h: number, frame: number, energy: number) {
  if (energy < 0.1) return;
  const hue = (frame * 3) % 360;
  ctx.globalAlpha = energy * 0.3;
  ctx.shadowBlur = 15 + energy * 10;
  ctx.shadowColor = `hsl(${hue}, 100%, 60%)`;
  ctx.strokeStyle = `hsl(${hue}, 100%, 60%)`;
  ctx.lineWidth = 2;
  ctx.strokeRect(4, 4, w - 8, h - 8);
  ctx.shadowBlur = 0;
  ctx.globalAlpha = 1.0;
}

function applyVisualEffects(
  ctx: CanvasRenderingContext2D,
  effects: VisualEffect[],
  w: number,
  h: number,
  frame: number,
  energy: number,
  px: number,
) {
  for (const fx of effects) {
    switch (fx) {
      case 'scanlines':    fxScanlines(ctx, w, h, energy); break;
      case 'pixel_rain':   fxPixelRain(ctx, w, h, frame, energy, px); break;
      case 'star_field':   fxStarField(ctx, w, h, frame, energy); break;
      case 'fire_trails':  fxFireTrails(ctx, w, h, frame, energy, px); break;
      case 'mirror_ball':  fxMirrorBall(ctx, w, h, frame, energy); break;
      case 'neon_glow':    fxNeonGlow(ctx, w, h, frame, energy); break;
    }
  }
}

// ─── Crowd + Lights ─────────────────────────────────────────────────────────

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

    pxRect(ctx, x, finalY, 2, 2, color, px);
    pxRect(ctx, x, finalY + 2, 2, 2, color, px);

    if (isJumping && crowd.excitement > 0.5) {
      const armUp = Math.sin((frame + i) * 0.3) > 0;
      if (armUp) {
        pxRect(ctx, x - 1, finalY - 1, 1, 1, color, px);
        pxRect(ctx, x + 2, finalY - 1, 1, 1, color, px);
      }
    }
  }
}

function drawStageLights(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  lighting: StageLighting,
  px: number,
) {
  const [r, g, b] = lighting.spotlight_color;

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

  if (lighting.beat_flash > 0.1) {
    ctx.globalAlpha = lighting.beat_flash * 0.2;
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, width * px, height * px);
    ctx.globalAlpha = 1.0;
  }

  if (lighting.strobe_active) {
    ctx.globalAlpha = 0.3;
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, width * px, height * px);
    ctx.globalAlpha = 1.0;
  }
}

// ─── Camera Viewport ────────────────────────────────────────────────────────

interface CameraViewport {
  /** Grid-space X of viewport left edge */
  x: number;
  /** Grid-space Y of viewport top edge */
  y: number;
  /** Grid-space width of visible area */
  w: number;
  /** Grid-space height of visible area */
  h: number;
}

const MEMBER_ROLES = Object.keys(MEMBER_POSITIONS);

/**
 * Resolve the viewport for the current camera mode.
 * Returns a rectangle in grid-coordinate space that should fill the canvas.
 */
function resolveCamera(
  mode: CameraMode,
  focusRole: string | null,
  band: BandMemberSnapshot[],
  frame: number,
  autoStateRef: React.MutableRefObject<{
    currentMode: CameraMode;
    focusIdx: number;
    holdTimer: number;
    lastSwitch: number;
  }>,
  GRID_W: number,
  GRID_H: number,
): CameraViewport {
  const fullStage: CameraViewport = { x: 0, y: 0, w: GRID_W, h: GRID_H };
  const stageView: CameraViewport = { x: 4, y: 12, w: GRID_W - 8, h: GRID_H - 24 };

  // Find the most energetic member
  const mostActive = band.length > 0
    ? band.reduce((best, m) => m.energy > best.energy ? m : best, band[0])
    : null;

  const zoomOnRole = (role: string): CameraViewport => {
    const pos = MEMBER_POSITIONS[role];
    if (!pos) return stageView;
    // 32x28 grid-unit window centred on the member
    const zw = 32;
    const zh = 28;
    const zx = Math.max(0, Math.min(GRID_W - zw, pos.x - zw / 2));
    const zy = Math.max(0, Math.min(GRID_H - zh, 50 - 2 - zh / 2)); // stage Y is 50
    return { x: zx, y: zy, w: zw, h: zh };
  };

  switch (mode) {
    case 'full_stage':
      return fullStage;

    case 'stage_view':
      return stageView;

    case 'close_up': {
      if (mostActive && mostActive.energy > 0.1) {
        return zoomOnRole(mostActive.role);
      }
      return stageView;
    }

    case 'zoom_character': {
      const target = focusRole && MEMBER_POSITIONS[focusRole] ? focusRole : 'drummer';
      return zoomOnRole(target);
    }

    case 'auto': {
      const st = autoStateRef.current;
      const timeSinceSwitch = frame - st.lastSwitch;
      const holdBeats = 120; // ~4 seconds at 30fps

      if (timeSinceSwitch >= holdBeats) {
        // Time to switch
        const maxEnergy = mostActive?.energy ?? 0;

        if (maxEnergy > 0.6) {
          // High energy → zoom on hottest member
          if (mostActive && st.currentMode !== 'zoom_character') {
            st.currentMode = 'zoom_character';
            st.focusIdx = MEMBER_ROLES.indexOf(mostActive.role);
            if (st.focusIdx < 0) st.focusIdx = 0;
          } else {
            // Already zoomed — rotate to next active member
            const activeMems = band.filter(m => m.energy > 0.2);
            if (activeMems.length > 0) {
              st.focusIdx = (st.focusIdx + 1) % MEMBER_ROLES.length;
              st.currentMode = 'zoom_character';
            }
          }
        } else if (maxEnergy > 0.3) {
          // Medium energy → stage view or close-up
          st.currentMode = st.currentMode === 'stage_view' ? 'close_up' : 'stage_view';
        } else {
          // Low energy / idle → full stage
          st.currentMode = 'full_stage';
        }
        st.lastSwitch = frame;
      }

      if (st.currentMode === 'zoom_character') {
        const role = MEMBER_ROLES[st.focusIdx] || 'drummer';
        return zoomOnRole(role);
      } else if (st.currentMode === 'close_up') {
        if (mostActive && mostActive.energy > 0.1) {
          return zoomOnRole(mostActive.role);
        }
        return stageView;
      } else if (st.currentMode === 'stage_view') {
        return stageView;
      }
      return fullStage;
    }

    default:
      return fullStage;
  }
}

/**
 * Smoothly interpolate between current and target camera viewport.
 */
function lerpViewport(current: CameraViewport, target: CameraViewport, t: number): CameraViewport {
  const s = Math.min(1, Math.max(0, t));
  return {
    x: current.x + (target.x - current.x) * s,
    y: current.y + (target.y - current.y) * s,
    w: current.w + (target.w - current.w) * s,
    h: current.h + (target.h - current.h) * s,
  };
}

// ─── HUD Overlay (drawn in screen-space, outside viewport transform) ────────

const CAMERA_LABELS: Record<string, string> = {
  full_stage: 'FULL STAGE',
  stage_view: 'STAGE VIEW',
  close_up: 'CLOSE UP',
  zoom_character: 'ZOOM',
  auto: 'AUTO',
};

function drawHUD(
  ctx: CanvasRenderingContext2D,
  canvasW: number,
  _canvasH: number,
  cameraMode: CameraMode,
  cameraFocus: string | null,
  snap: PerformanceSnapshot | null,
) {
  const fontSize = Math.max(10, Math.min(14, canvasW / 60));

  // Camera mode badge — top-right
  const modeLabel = CAMERA_LABELS[cameraMode] || cameraMode.toUpperCase();
  let badge = modeLabel;
  if (cameraMode === 'zoom_character' && cameraFocus) {
    const pos = MEMBER_POSITIONS[cameraFocus];
    badge += ` — ${pos?.label || cameraFocus.toUpperCase()}`;
  } else if (cameraMode === 'auto' && snap?.is_playing) {
    badge += ' ●';
  }

  ctx.save();
  ctx.font = `bold ${fontSize}px monospace`;
  ctx.textAlign = 'right';

  // Background pill
  const metrics = ctx.measureText(badge);
  const bw = metrics.width + 12;
  const bh = fontSize + 8;
  const bx = canvasW - 8 - bw;
  const by = 6;
  ctx.globalAlpha = 0.5;
  ctx.fillStyle = '#000';
  ctx.beginPath();
  ctx.roundRect(bx, by, bw, bh, 4);
  ctx.fill();
  ctx.globalAlpha = 1.0;

  ctx.fillStyle = cameraMode === 'auto' ? '#00ff88' : '#aaa';
  ctx.fillText(badge, canvasW - 14, by + fontSize + 1);
  ctx.restore();
}

// ─── Detached Window Component ──────────────────────────────────────────────

const BandVisualizerWindow: React.FC = () => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const snapshotRef = useRef<PerformanceSnapshot | null>(null);
  const localFrameRef = useRef<number>(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  // Camera state refs (kept outside React state for perf — updated every frame)
  const cameraViewportRef = useRef<CameraViewport>({ x: 0, y: 0, w: 128, h: 80 });
  const autoStateRef = useRef({ currentMode: 'full_stage' as CameraMode, focusIdx: 0, holdTimer: 0, lastSwitch: 0 });

  // Sync theme from main app via localStorage
  useEffect(() => {
    const applyTheme = () => {
      const theme = localStorage.getItem('pibeat-theme') || 'pibeat';
      if (theme === 'pibeat') {
        document.documentElement.removeAttribute('data-theme');
      } else {
        document.documentElement.setAttribute('data-theme', theme);
      }
    };
    applyTheme();
    const onStorage = (e: StorageEvent) => {
      if (e.key === 'pibeat-theme') applyTheme();
    };
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, []);

  const appWindow = getCurrentWindow();
  const handleMinimize = () => appWindow.minimize();
  const handleMaximize = () => appWindow.toggleMaximize();
  const handleClose = () => appWindow.close();
  const handleFullscreen = useCallback(async () => {
    const next = !isFullscreen;
    await appWindow.setFullscreen(next);
    setIsFullscreen(next);
  }, [isFullscreen, appWindow]);

  const render = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const GRID_W = 128;
    const GRID_H = 80;

    const snap = snapshotRef.current;
    const frame = localFrameRef.current;
    localFrameRef.current++;

    const stageY = 50;
    const decor: StageDecor = snap?.decor ?? 'retro_stage';
    const danceStyle: DanceStyle = snap?.dance_style ?? 'bounce';
    const activeEffects: VisualEffect[] = snap?.active_effects ?? [];
    const energy = snap?.energy ?? 0;
    const cameraMode: CameraMode = snap?.camera_mode ?? 'full_stage';
    const cameraFocus: string | null = snap?.camera_focus ?? null;
    const visibleMembers: Record<string, boolean> = snap?.visible_members ?? {};

    // ── Compute camera viewport ──────────────────────────────────────
    const targetVP = resolveCamera(
      cameraMode,
      cameraFocus,
      snap?.band ?? [],
      frame,
      autoStateRef,
      GRID_W,
      GRID_H,
    );
    // Smooth lerp towards target (0.08 = ~12 frames to converge)
    cameraViewportRef.current = lerpViewport(cameraViewportRef.current, targetVP, 0.08);
    const vp = cameraViewportRef.current;

    // ── Compute pixel scale from viewport ────────────────────────────
    // Use separate X/Y scales so the scene always fills the full canvas
    const scaleX = canvas.width / vp.w;
    const scaleY = canvas.height / vp.h;
    // Use the average for character/shape drawing (keeps proportions close)
    const px = Math.max(1, (scaleX + scaleY) / 2);
    const snappedPx = px >= 4 ? Math.floor(px) : px;

    // Clear full canvas
    ctx.fillStyle = PALETTE.bg;
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    ctx.save();
    // Scale to fill entire canvas, offset by viewport origin
    ctx.scale(scaleX / snappedPx, scaleY / snappedPx);
    ctx.translate(-vp.x * snappedPx, -vp.y * snappedPx);

    // Clip to viewport area so off-screen drawing is hidden
    ctx.beginPath();
    ctx.rect(vp.x * snappedPx, vp.y * snappedPx, vp.w * snappedPx, vp.h * snappedPx);
    ctx.clip();

    // Background
    ctx.fillStyle = PALETTE.bg;
    ctx.fillRect(0, 0, GRID_W * snappedPx, GRID_H * snappedPx);

    // Draw decor / backdrop
    drawDecor(ctx, decor, GRID_W, GRID_H, stageY, snappedPx, frame, energy);

    const isMemberVisible = (role: string): boolean => visibleMembers[role] !== false;

    if (!snap || !snap.is_playing) {
      // Idle state — show visible members in idle
      for (const [role, pos] of Object.entries(MEMBER_POSITIONS)) {
        if (!isMemberVisible(role)) continue;
        if (role === 'drummer' || role === 'percussionist') {
          drawDrummer(ctx, pos.x, stageY - 2, 0, 'idle', snappedPx, 0, 'bounce', pos.color);
        } else if (role === 'dj') {
          drawDJ(ctx, pos.x, stageY - 2, 0, 'idle', snappedPx, 0, 'bounce');
        } else {
          drawCharacter(ctx, pos.x, stageY - 2, PALETTE.idle, 0, 'idle', snappedPx, 0, 'bounce');
        }
      }

      ctx.fillStyle = PALETTE.textBright;
      ctx.font = `${Math.max(10, snappedPx * 4)}px monospace`;
      ctx.textAlign = 'center';
      ctx.fillText(
        isPlaying ? 'Waiting for events...' : '♪ Press Run to start ♪',
        (GRID_W / 2) * snappedPx,
        25 * snappedPx
      );

      // Apply effects even when idle (for preview)
      if (activeEffects.length > 0) {
        applyVisualEffects(ctx, activeEffects, GRID_W * snappedPx, GRID_H * snappedPx, frame, 0.1, snappedPx);
      }

      ctx.restore();

      // HUD overlay (drawn outside viewport transform for consistent sizing)
      drawHUD(ctx, canvas.width, canvas.height, cameraMode, cameraFocus, snap);

      return;
    }

    // Active performance
    drawStageLights(ctx, GRID_W, GRID_H, snap.lighting, snappedPx);

    for (const member of snap.band) {
      const pos = MEMBER_POSITIONS[member.role];
      if (!pos) continue;
      if (!isMemberVisible(member.role)) continue;

      const state = member.animation_state.state;

      if (member.role === 'drummer' || member.role === 'percussionist') {
        drawDrummer(ctx, pos.x, stageY - 2, member.energy, state, snappedPx, frame, danceStyle, pos.color);
      } else if (member.role === 'dj') {
        drawDJ(ctx, pos.x, stageY - 2, member.energy, state, snappedPx, frame, danceStyle);
      } else {
        drawCharacter(ctx, pos.x, stageY - 2, pos.color, member.energy, state, snappedPx, frame, danceStyle);
      }

      // Energy bar
      const barW = 8;
      const barH = 1;
      const barX = pos.x - barW / 2;
      const barY = stageY + 10;
      pxRect(ctx, barX, barY, barW, barH, '#222', snappedPx);
      pxRect(ctx, barX, barY, Math.round(barW * member.energy), barH, pos.color, snappedPx);
    }

    // Crowd
    drawCrowd(ctx, 4, GRID_H - 8, GRID_W - 8, snap.crowd, snappedPx, frame);

    // In-scene energy bar + beat dots
    const accentColor = '#00ff88';
    const energyBarW = GRID_W - 8;
    pxRect(ctx, 4, 2, energyBarW, 2, '#111', snappedPx);
    pxRect(ctx, 4, 2, Math.round(energyBarW * snap.energy), 2, accentColor, snappedPx);

    ctx.fillStyle = PALETTE.textBright;
    ctx.font = `${Math.max(8, snappedPx * 3)}px monospace`;
    ctx.textAlign = 'left';
    ctx.fillText(`${Math.round(snap.bpm)} BPM`, 4 * snappedPx, 9 * snappedPx);

    const beatInBar = Math.floor(snap.beat_position);
    for (let i = 0; i < 4; i++) {
      const dotX = GRID_W - 14 + i * 3;
      pxRect(ctx, dotX, 3, 2, 2, i <= beatInBar ? accentColor : '#333', snappedPx);
    }

    // Apply visual effects (post-processing layer)
    if (activeEffects.length > 0) {
      applyVisualEffects(ctx, activeEffects, GRID_W * snappedPx, GRID_H * snappedPx, frame, snap.energy, snappedPx);
    }

    ctx.restore();

    // HUD overlay (drawn outside viewport transform for consistent sizing)
    drawHUD(ctx, canvas.width, canvas.height, cameraMode, cameraFocus, snap);

  }, [isPlaying]);

  // Polling loop: fetch snapshot + render
  useEffect(() => {
    let running = true;
    let pollTimer: ReturnType<typeof setTimeout>;

    const poll = async () => {
      if (!running) return;
      try {
        const snap = await invoke<PerformanceSnapshot>('get_visual_snapshot');
        snapshotRef.current = snap;
        setIsPlaying(snap.is_playing);
      } catch {
        // Visual engine not available
      }
      render();
      if (running) {
        pollTimer = setTimeout(poll, 33);
      }
    };

    poll();

    return () => {
      running = false;
      clearTimeout(pollTimer);
    };
  }, [render]);

  // Canvas resize — use ResizeObserver on parent for accurate sizing
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const parent = canvas.parentElement;
    if (!parent) return;

    const resize = () => {
      const dpr = window.devicePixelRatio;
      canvas.width = parent.clientWidth * dpr;
      canvas.height = parent.clientHeight * dpr;
    };

    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(parent);
    return () => ro.disconnect();
  }, [isFullscreen]);

  // F11 fullscreen toggle
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'F11') {
        e.preventDefault();
        handleFullscreen();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [handleFullscreen]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', width: '100%', height: '100vh', overflow: 'hidden', background: 'var(--bg-primary, #0d0d1a)' }}>
      {/* Custom titlebar with integrated controls — hidden in fullscreen */}
      {!isFullscreen && (
        <div className="app-header" style={{ flexShrink: 0 }}>
          <div className="titlebar-left" data-tauri-drag-region>
            <div className="app-logo">
              <span className="logo-icon">&#9835;</span>
              <span className="logo-text">PiBeat</span>
            </div>
          </div>
          <span style={{ color: 'var(--text-secondary)', fontSize: 12, userSelect: 'none', pointerEvents: 'none' }}>Band Visualizer</span>
          <div className="titlebar-spacer" data-tauri-drag-region />
          <div className="titlebar-controls">
            <button className="titlebar-button" onClick={handleMinimize} title="Minimize">
              <svg width="10" height="1" viewBox="0 0 10 1">
                <rect width="10" height="1" fill="currentColor" />
              </svg>
            </button>
            <button className="titlebar-button" onClick={handleFullscreen} title="Fullscreen (F11)">
              <svg width="10" height="10" viewBox="0 0 10 10">
                <path d="M 0,0 L 3,0 L 0,3 Z M 10,0 L 7,0 L 10,3 Z M 0,10 L 3,10 L 0,7 Z M 10,10 L 7,10 L 10,7 Z" fill="currentColor" />
              </svg>
            </button>
            <button className="titlebar-button" onClick={handleMaximize} title="Maximize">
              <svg width="10" height="10" viewBox="0 0 10 10">
                <rect x="0" y="0" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="1" />
              </svg>
            </button>
            <button className="titlebar-button titlebar-close" onClick={handleClose} title="Close">
              <svg width="10" height="10" viewBox="0 0 10 10">
                <path d="M 0,0 L 10,10 M 10,0 L 0,10" stroke="currentColor" strokeWidth="1" />
              </svg>
            </button>
          </div>
        </div>
      )}
      <div style={{ position: 'relative', flex: 1, overflow: 'hidden' }}>
        <canvas
          ref={canvasRef}
          style={{
            width: '100%',
            height: '100%',
            imageRendering: 'pixelated',
            background: 'var(--bg-primary, #0d0d1a)',
            cursor: 'default',
          }}
        />
      </div>
    </div>
  );
};

export default BandVisualizerWindow;
