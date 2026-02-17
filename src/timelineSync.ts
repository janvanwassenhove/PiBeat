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
 * Update the amp of a single clip inside the buffer code.
 *
 * Works by locating the clip's source lines (`srcLineStart..srcLineEnd`)
 * and replacing every `amp: X` token within that range.
 */
export function applyClipAmpChange(
  code: string,
  clip: TimelineClip,
  newAmp: number,
): string {
  const lines = code.split('\n');
  const start = clip.srcLineStart;
  const end = Math.min(clip.srcLineEnd, lines.length - 1);

  for (let i = start; i <= end; i++) {
    if (/amp:\s*[\d.]+/.test(lines[i])) {
      lines[i] = lines[i].replace(/amp:\s*[\d.]+/g, `amp: ${fmtNum(newAmp)}`);
    }
  }
  return lines.join('\n');
}

/**
 * Apply a track-level amp change.  This multiplies into every clip's
 * amp values within the track's source range, or – if the track has
 * no per-clip amp tags – inserts one.
 */
export function applyTrackAmpChange(
  code: string,
  track: TimelineTrack,
  newTrackAmp: number,
): string {
  let result = code;
  for (const clip of track.clips) {
    // The effective new amp for each clip is clip.amp * newTrackAmp,
    // but we only want to scale relative to the original track amp.
    // However, since we store the original amp on the clip itself,
    // we just update each clip's amp lines with clip.amp * newTrackAmp.
    result = applyClipAmpChange(result, clip, clip.amp * newTrackAmp);
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

  // Dedent the lines that were inside the fx block
  // (not strictly necessary but keeps code tidy)

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
 * For live_loops that sit after a top-level `sleep N`, this means
 * adjusting the preceding sleep value.  For clips that have an
 * initial internal sleep, we adjust that instead.
 *
 * Strategy:
 *  1. If there is a `sleep X` on the line just before `srcLineStart`,
 *     change that sleep value so the clip starts at `newStartBeat`.
 *  2. Otherwise leave code unchanged (position is visual only).
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
  // preceding `sleep` at the same or lower indentation level.
  let sleepLineIdx = -1;
  for (let i = start - 1; i >= 0; i--) {
    const t = lines[i].trim();
    if (!t || t.startsWith('#') || t.startsWith('##')) continue;
    if (/^sleep\s+[\d.]+/.test(t)) {
      sleepLineIdx = i;
      break;
    }
    // Hit a non-sleep non-blank line → stop looking
    break;
  }

  if (sleepLineIdx === -1) {
    // No preceding sleep found — try inserting one
    if (newStartBeat > 0) {
      lines.splice(start, 0, `sleep ${fmtNum(newStartBeat)}`);
    }
    return lines.join('\n');
  }

  // We found a preceding sleep. Calculate what it should be.
  // The preceding sleep's absolute position is the clip's old
  // startBeat minus any internal sleep.  The simplest correct
  // approach: set preceding sleep = newStartBeat – position of
  // the line before the sleep.
  //
  // Because precisely computing the global cursor up to `sleepLineIdx`
  // is expensive, we use a simpler heuristic: delta-adjust the sleep.
  const currentSleepMatch = lines[sleepLineIdx].match(/sleep\s+([\d.]+)/);
  if (currentSleepMatch) {
    const oldSleep = parseFloat(currentSleepMatch[1]);
    const delta = newStartBeat - _oldStartBeat;
    const newSleep = Math.max(0, oldSleep + delta);
    lines[sleepLineIdx] = replaceSleepValue(lines[sleepLineIdx], newSleep);
  }

  return lines.join('\n');
}

// ─── Duration change ─────────────────────────────────────────────

/**
 * Change the duration of a clip.
 *
 * For live_loops with `N.times do`, we change N to fit the new
 * duration.  For clips with a `sleep` at the end, we adjust that.
 */
export function applyClipDurationChange(
  code: string,
  clip: TimelineClip,
  newDurationBeats: number,
  oldDurationBeats: number,
): string {
  const lines = code.split('\n');
  const start = clip.srcLineStart;
  const end = Math.min(clip.srcLineEnd, lines.length - 1);

  // Strategy 1: Find `N.times do` within the clip and adjust N
  for (let i = start; i <= end; i++) {
    const m = lines[i].match(/^(\s*)(\d+)(\.times\s+do)/);
    if (m) {
      const oldN = parseInt(m[2]);
      if (oldN > 0 && oldDurationBeats > 0) {
        const singleIterDur = oldDurationBeats / oldN;
        const newN = Math.max(1, Math.round(newDurationBeats / singleIterDur));
        lines[i] = `${m[1]}${newN}${m[3]}`;
        return lines.join('\n');
      }
    }
  }

  // Strategy 2: Find the last `sleep` inside the block and adjust it
  for (let i = end; i >= start; i--) {
    const sm = lines[i].match(/sleep\s+([\d.]+)/);
    if (sm) {
      const oldSleep = parseFloat(sm[1]);
      const delta = newDurationBeats - oldDurationBeats;
      const newSleep = Math.max(0, oldSleep + delta);
      lines[i] = replaceSleepValue(lines[i], newSleep);
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
