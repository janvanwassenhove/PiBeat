/**
 * timelineSync.ts
 *
 * Surgical code-editing helpers that map timeline UI changes
 * back to the Sonic Pi source code.  Each function takes the
 * current buffer code (as a string), together with the clip /
 * track metadata that was produced by the parser, and returns
 * the updated code string.
 *
 * Design goals:
 *  – Preserve all original formatting, comments, structure.
 *  – Only touch the exact lines that need to change.
 *  – Be safe when source lines have shifted (re-parse first).
 */

import { TimelineClip, TimelineTrack, ClipEffect } from './timelineParser';

// ─── Helpers ─────────────────────────────────────────────────────

function fmtNum(n: number): string {
  if (Number.isInteger(n)) return n.toString();
  return n.toFixed(2).replace(/0+$/, '').replace(/\.$/, '');
}

/**
 * Replace the sleep value on a specific sleep line.
 */
function replaceSleepValue(line: string, newVal: number): string {
  return line.replace(/sleep\s+[\d.]+/, `sleep ${fmtNum(newVal)}`);
}

// ─── Amp change ──────────────────────────────────────────────────

/**
 * Insert `, amp: X` into a play/sample line that doesn't already have one.
 */
function insertAmpParam(line: string, amp: number): string {
  // Split off trailing comment
  const commentIdx = line.indexOf('#');
  let codePart = commentIdx >= 0 ? line.slice(0, commentIdx) : line;
  const commentPart = commentIdx >= 0 ? ' ' + line.slice(commentIdx) : '';

  // Check for trailing `if` condition (e.g., `sample :kick if one_in(3)`)
  const ifMatch = codePart.match(/(\s+if\s+.+)$/);
  if (ifMatch) {
    const beforeIf = codePart.slice(0, codePart.length - ifMatch[1].length).trimEnd();
    return `${beforeIf}, amp: ${fmtNum(amp)}${ifMatch[1]}${commentPart}`;
  }

  return `${codePart.trimEnd()}, amp: ${fmtNum(amp)}${commentPart}`;
}

/**
 * Insert `, param: value` into a play/sample line.
 * Handles trailing comments and `if` conditions.
 */
function insertParam(line: string, param: string, value: number): string {
  const commentIdx = line.indexOf('#');
  let codePart = commentIdx >= 0 ? line.slice(0, commentIdx) : line;
  const commentPart = commentIdx >= 0 ? ' ' + line.slice(commentIdx) : '';

  const ifMatch = codePart.match(/(\s+if\s+.+)$/);
  if (ifMatch) {
    const beforeIf = codePart.slice(0, codePart.length - ifMatch[1].length).trimEnd();
    return `${beforeIf}, ${param}: ${fmtNum(value)}${ifMatch[1]}${commentPart}`;
  }

  return `${codePart.trimEnd()}, ${param}: ${fmtNum(value)}${commentPart}`;
}

/**
 * Update the amp of a single clip inside the buffer code.
 *
 * Uses proportional scaling: computes a ratio from the clip's current
 * amp to the desired amp, then scales every `amp: X` in the source
 * range.  For play/sample lines without an explicit `amp:`, inserts one.
 */
export function applyClipAmpChange(
  code: string,
  clip: TimelineClip,
  newAmp: number,
): string {
  const lines = code.split('\n');
  const start = clip.srcLineStart;
  const end = Math.min(clip.srcLineEnd, lines.length - 1);
  const ratio = clip.amp > 0 ? newAmp / clip.amp : newAmp;

  for (let i = start; i <= end; i++) {
    if (/amp:\s*[\d.]+/.test(lines[i])) {
      // Scale existing numeric amp values proportionally
      lines[i] = lines[i].replace(/amp:\s*([\d.]+)/g, (_match, oldVal) => {
        const scaled = parseFloat(oldVal) * ratio;
        return `amp: ${fmtNum(Math.max(0, scaled))}`;
      });
    } else if (/amp:/.test(lines[i])) {
      // Has amp: with non-numeric value (e.g., rrand()) — skip
    } else if (/^\s*(play|sample)\b/.test(lines[i])) {
      // Insert amp for play/sample lines that don't have one
      const scaledAmp = ratio; // default amp 1.0 × ratio
      if (Math.abs(scaledAmp - 1.0) > 0.001) {
        lines[i] = insertAmpParam(lines[i], scaledAmp);
      }
    }
  }
  return lines.join('\n');
}

/**
 * Apply a track-level amp change.  The caller should pass an amp
 * ratio (newTrackAmp / oldTrackAmp) so that clip amps are scaled
 * rather than compounded.  Each clip's source amp values are
 * multiplied by the ratio.
 */
export function applyTrackAmpChange(
  code: string,
  track: TimelineTrack,
  ampRatio: number,
): string {
  let result = code;
  for (const clip of track.clips) {
    result = applyClipAmpChange(result, clip, clip.amp * ampRatio);
  }
  return result;
}

// ─── Effects change ──────────────────────────────────────────────

/**
 * Wrap a clip's code block with a new `with_fx` wrapper.
 * Inserts the fx line before the clip's first source line and
 * an `end` after the last source line.
 */
export function applyAddEffect(
  code: string,
  clip: TimelineClip,
  effect: ClipEffect,
): string {
  const lines = code.split('\n');
  const start = clip.srcLineStart;
  const end = Math.min(clip.srcLineEnd, lines.length - 1);

  // Determine indentation from the clip's first line
  const indent = lines[start].match(/^(\s*)/)?.[1] || '';

  const paramStr = Object.entries(effect.params)
    .map(([k, v]) => `${k}: ${fmtNum(v)}`)
    .join(', ');
  const fxLine = `${indent}with_fx :${effect.type}${paramStr ? ', ' + paramStr : ''} do`;

  // Insert the fx wrapper before the clip's code block
  lines.splice(start, 0, fxLine);
  // Insert `end` after (now shifted by +1)
  lines.splice(end + 2, 0, `${indent}end`);

  return lines.join('\n');
}

/**
 * Remove a `with_fx` wrapper from a clip's source range by
 * finding the matching `with_fx :TYPE` line and its closing `end`.
 * Also dedents the inner lines that were wrapped.
 */
export function applyRemoveEffect(
  code: string,
  clip: TimelineClip,
  effectType: string,
): string {
  const lines = code.split('\n');
  const start = clip.srcLineStart;
  const end = Math.min(clip.srcLineEnd, lines.length - 1);

  // Find the with_fx line for this effect type within the clip range
  let fxLineIdx = -1;
  for (let i = start; i <= end; i++) {
    if (new RegExp(`with_fx\\s+:${effectType}`).test(lines[i])) {
      fxLineIdx = i;
      break;
    }
  }
  if (fxLineIdx === -1) return code;

  // Detect indentation of the with_fx line to determine how much to dedent
  const fxIndent = lines[fxLineIdx].match(/^(\s*)/)?.[1] || '';

  // Find the matching `end` by tracking depth
  let depth = 0;
  let endLineIdx = -1;
  for (let i = fxLineIdx; i <= end; i++) {
    const t = lines[i].trim();
    if (/\bdo\s*$/.test(t) || /\bdo\s*\|/.test(t)) depth++;
    if (t === 'end') {
      depth--;
      if (depth === 0) { endLineIdx = i; break; }
    }
  }

  // Remove the `end` first (higher index), then the with_fx line
  if (endLineIdx !== -1) {
    lines.splice(endLineIdx, 1);
  }
  lines.splice(fxLineIdx, 1);

  // Dedent the lines that were inside the fx block by 2 spaces
  const dedentRange = endLineIdx !== -1
    ? { start: fxLineIdx, end: endLineIdx - 2 }   // adjusted for removed lines
    : { start: fxLineIdx, end: Math.min(end - 1, lines.length - 1) };
  for (let i = dedentRange.start; i <= dedentRange.end && i < lines.length; i++) {
    if (lines[i].startsWith(fxIndent + '  ')) {
      lines[i] = fxIndent + lines[i].slice(fxIndent.length + 2);
    }
  }

  return lines.join('\n');
}

/**
 * Update the parameters of an existing `with_fx` effect within
 * a clip's source range.
 */
export function applyUpdateEffectParams(
  code: string,
  clip: TimelineClip,
  effectType: string,
  params: Record<string, number>,
): string {
  const lines = code.split('\n');
  const start = clip.srcLineStart;
  const end = Math.min(clip.srcLineEnd, lines.length - 1);

  for (let i = start; i <= end; i++) {
    const m = lines[i].match(new RegExp(`(\\s*with_fx\\s+:${effectType})`));
    if (m) {
      const indent = lines[i].match(/^(\s*)/)?.[1] || '';
      const paramStr = Object.entries(params)
        .map(([k, v]) => `${k}: ${fmtNum(v)}`)
        .join(', ');
      lines[i] = `${indent}with_fx :${effectType}${paramStr ? ', ' + paramStr : ''} do`;
      break;
    }
  }
  return lines.join('\n');
}

// ─── Timing / position changes ───────────────────────────────────

/**
 * Change the beat at which a clip starts.
 *
 * Walks backwards from the clip to find the nearest preceding `sleep`,
 * stopping at structural boundaries (loop headers, `end`, `define`,
 * or other audible statements). Delta-adjusts the sleep value, or
 * inserts a new sleep if none is found.
 */
export function applyClipStartChange(
  code: string,
  clip: TimelineClip,
  newStartBeat: number,
  _oldStartBeat: number,
): string {
  const lines = code.split('\n');
  const start = clip.srcLineStart;

  // Walk backwards from the clip's first line to find the nearest
  // preceding `sleep`. Skip blanks and comments, but stop at
  // structural boundaries or other audible statements.
  let sleepLineIdx = -1;
  for (let i = start - 1; i >= 0; i--) {
    const t = lines[i].trim();
    if (!t || t.startsWith('#')) continue;
    if (/^sleep\s+[\d.]+/.test(t)) {
      sleepLineIdx = i;
      break;
    }
    // Stop at structural boundaries
    if (/\bdo\s*$/.test(t) || t === 'end' || /^live_loop\b/.test(t)
        || /^in_thread\b/.test(t) || /^define\b/.test(t)
        || /^loop\b/.test(t) || /^def\b/.test(t)) {
      break;
    }
    // Stop at other audible lines (different clips)
    if (/^(play|sample|play_pattern_timed)\b/.test(t)) break;
  }

  if (sleepLineIdx === -1) {
    // No preceding sleep found — insert one if the clip should be offset
    if (newStartBeat > 0 && _oldStartBeat === 0) {
      const indent = lines[start].match(/^(\s*)/)?.[1] || '';
      lines.splice(start, 0, `${indent}sleep ${fmtNum(newStartBeat)}`);
    }
    return lines.join('\n');
  }

  // Delta-adjust the preceding sleep value
  const currentSleepMatch = lines[sleepLineIdx].match(/sleep\s+([\d.]+)/);
  if (currentSleepMatch) {
    const oldSleep = parseFloat(currentSleepMatch[1]);
    const delta = newStartBeat - _oldStartBeat;
    const newSleep = Math.max(0, oldSleep + delta);
    if (newSleep < 0.001) {
      // Remove the sleep line entirely when it becomes zero
      lines.splice(sleepLineIdx, 1);
    } else {
      lines[sleepLineIdx] = replaceSleepValue(lines[sleepLineIdx], newSleep);
    }
  }

  return lines.join('\n');
}

// ─── Duration change ─────────────────────────────────────────────

/**
 * Change the duration of a clip.
 *
 * Strategies (in priority order):
 *  0. Single-line clip → modify sustain / release / beat_stretch params directly
 *  1. N.times do → adjust N
 *  2. Scale all sleeps proportionally within the block
 *  3. Scale play_pattern_timed time values
 */
export function applyClipDurationChange(
  code: string,
  clip: TimelineClip,
  newDurationBeats: number,
  oldDurationBeats: number,
): string {
  if (oldDurationBeats <= 0 || Math.abs(newDurationBeats - oldDurationBeats) < 0.01) {
    return code;
  }

  const lines = code.split('\n');
  const start = clip.srcLineStart;
  const end = Math.min(clip.srcLineEnd, lines.length - 1);

  // ── Strategy 0: Single-line clip → modify params directly ──
  if (start === end) {
    const line = lines[start];

    // Sample lines: prefer beat_stretch, then sustain
    if (/^\s*sample\b/.test(line)) {
      if (/beat_stretch:\s*[\d.]+/.test(line)) {
        lines[start] = line.replace(/beat_stretch:\s*[\d.]+/, `beat_stretch: ${fmtNum(newDurationBeats)}`);
        return lines.join('\n');
      }
      if (/sustain:\s*[\d.]+/.test(line)) {
        lines[start] = line.replace(/sustain:\s*[\d.]+/, `sustain: ${fmtNum(newDurationBeats)}`);
        return lines.join('\n');
      }
      // Insert beat_stretch param
      lines[start] = insertParam(line, 'beat_stretch', newDurationBeats);
      return lines.join('\n');
    }

    // Play lines: adjust sustain, fallback to release
    if (/^\s*play\b/.test(line)) {
      const parseNum = (pattern: RegExp): number => {
        const m = line.match(pattern);
        return m ? parseFloat(m[1]) : 0;
      };
      const attack = parseNum(/attack:\s*([\d.]+)/);
      const hasRelease = /release:\s*[\d.]+/.test(line);
      const release = parseNum(/release:\s*([\d.]+)/) || 0.3;

      if (/sustain:\s*[\d.]+/.test(line)) {
        const newSustain = Math.max(0, newDurationBeats - attack - release);
        lines[start] = line.replace(/sustain:\s*[\d.]+/, `sustain: ${fmtNum(newSustain)}`);
        return lines.join('\n');
      }
      if (hasRelease) {
        const newRelease = Math.max(0.05, newDurationBeats - attack);
        lines[start] = line.replace(/release:\s*[\d.]+/, `release: ${fmtNum(newRelease)}`);
        return lines.join('\n');
      }
      // No explicit envelope — insert sustain
      const newSustain = Math.max(0, newDurationBeats - 0.3); // default release = 0.3
      lines[start] = insertParam(line, 'sustain', newSustain);
      return lines.join('\n');
    }

    // play_pattern_timed: scale time values
    const ptm = line.match(/play_pattern_timed\s+\[([^\]]*)\]\s*,\s*\[([^\]]*)\]/);
    if (ptm) {
      const ratio = newDurationBeats / oldDurationBeats;
      const times = ptm[2].split(',').map(s => parseFloat(s.trim())).filter(n => !isNaN(n));
      const newTimes = times.map(t => Math.max(0.0625, t * ratio));
      const newTimesStr = newTimes.map(t => fmtNum(t)).join(', ');
      lines[start] = line.replace(
        /play_pattern_timed\s+\[([^\]]*)\]\s*,\s*\[([^\]]*)\]/,
        `play_pattern_timed [${ptm[1]}], [${newTimesStr}]`
      );
      return lines.join('\n');
    }
  }

  // ── Strategy 1: Find `N.times do` within the clip and adjust N ──
  for (let i = start; i <= end; i++) {
    const m = lines[i].match(/^(\s*)(\d+)(\.times\s+do)/);
    if (m) {
      const oldN = parseInt(m[2]);
      if (oldN > 0) {
        const singleIterDur = oldDurationBeats / oldN;
        const newN = Math.max(1, Math.round(newDurationBeats / singleIterDur));
        lines[i] = `${m[1]}${newN}${m[3]}`;
        return lines.join('\n');
      }
    }
  }

  // ── Strategy 2: Scale all sleeps proportionally ──
  const ratio = newDurationBeats / oldDurationBeats;
  let foundSleep = false;
  for (let i = start; i <= end; i++) {
    if (/sleep\s+[\d.]+/.test(lines[i])) {
      const sm = lines[i].match(/sleep\s+([\d.]+)/);
      if (sm) {
        const newSleep = Math.max(0.0625, parseFloat(sm[1]) * ratio);
        lines[i] = replaceSleepValue(lines[i], newSleep);
        foundSleep = true;
      }
    }
  }
  if (foundSleep) return lines.join('\n');

  // ── Strategy 3: Scale play_pattern_timed time values within block ──
  for (let i = start; i <= end; i++) {
    const ptm = lines[i].match(/play_pattern_timed\s+\[([^\]]*)\]\s*,\s*\[([^\]]*)\]/);
    if (ptm) {
      const times = ptm[2].split(',').map(s => parseFloat(s.trim())).filter(n => !isNaN(n));
      const newTimes = times.map(t => Math.max(0.0625, t * ratio));
      const newTimesStr = newTimes.map(t => fmtNum(t)).join(', ');
      lines[i] = lines[i].replace(
        /play_pattern_timed\s+\[([^\]]*)\]\s*,\s*\[([^\]]*)\]/,
        `play_pattern_timed [${ptm[1]}], [${newTimesStr}]`
      );
      return lines.join('\n');
    }
  }

  return lines.join('\n');
}

// ─── Mute / solo ─────────────────────────────────────────────────

/**
 * Mute a clip by inserting `# MUTED ` at the start of each line,
 * or unmute by removing it.
 */
export function applyClipMute(
  code: string,
  clip: TimelineClip,
  muted: boolean,
): string {
  const lines = code.split('\n');
  const start = clip.srcLineStart;
  const end = Math.min(clip.srcLineEnd, lines.length - 1);

  for (let i = start; i <= end; i++) {
    if (muted) {
      if (!lines[i].trimStart().startsWith('# MUTED ')) {
        const indent = lines[i].match(/^(\s*)/)?.[1] || '';
        const content = lines[i].trimStart();
        lines[i] = `${indent}# MUTED ${content}`;
      }
    } else {
      lines[i] = lines[i].replace(/^(\s*)# MUTED /, '$1');
    }
  }
  return lines.join('\n');
}

// ─── Composite applier ──────────────────────────────────────────

export type ClipChange =
  | { kind: 'amp'; newAmp: number }
  | { kind: 'startBeat'; newStartBeat: number; oldStartBeat: number }
  | { kind: 'duration'; newDuration: number; oldDuration: number }
  | { kind: 'addEffect'; effect: ClipEffect }
  | { kind: 'removeEffect'; effectType: string }
  | { kind: 'updateEffectParams'; effectType: string; params: Record<string, number> }
  | { kind: 'mute'; muted: boolean };

/**
 * Apply a single change to the buffer code for a specific clip.
 * Returns the updated code string.
 */
export function applyClipChange(
  code: string,
  clip: TimelineClip,
  change: ClipChange,
): string {
  switch (change.kind) {
    case 'amp':
      return applyClipAmpChange(code, clip, change.newAmp);
    case 'startBeat':
      return applyClipStartChange(code, clip, change.newStartBeat, change.oldStartBeat);
    case 'duration':
      return applyClipDurationChange(code, clip, change.newDuration, change.oldDuration);
    case 'addEffect':
      return applyAddEffect(code, clip, change.effect);
    case 'removeEffect':
      return applyRemoveEffect(code, clip, change.effectType);
    case 'updateEffectParams':
      return applyUpdateEffectParams(code, clip, change.effectType, change.params);
    case 'mute':
      return applyClipMute(code, clip, change.muted);
  }
}
