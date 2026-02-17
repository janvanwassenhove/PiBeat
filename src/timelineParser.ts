/**
 * timelineParser.ts  –  v2
 *
 * Full-structure parser: every audible event in Sonic Pi code becomes a clip
 * on the timeline, not just live_loops.
 *
 * Handles:
 *  - live_loop blocks            → repeating / one-shot clips
 *  - Standalone sample calls     → individual sample clips
 *  - Standalone play / chord     → synth note clips
 *  - play_pattern_timed          → synth pattern clips
 *  - N.times do wrappers         → duration multiplier
 *  - Top-level sleep             → advances the global timeline cursor
 *  - with_fx wrappers            → clip/track effects
 *  - Section comments (## ----)  → section labels
 *  - use_bpm / use_synth         → global state
 */

export interface ClipEffect {
  type: string;
  params: Record<string, number>;
}

export interface TimelineClip {
  id: string;
  name: string;
  startBeat: number;
  durationBeats: number;
  code: string;
  type: 'sample' | 'synth' | 'mixed';
  color: string;
  amp: number;
  effects: ClipEffect[];
  isLooping: boolean;
  loopCount: number;        // 0 = infinite, >0 = finite
  samples: string[];
  /** Source line range in the original code (0-based) */
  srcLineStart: number;
  srcLineEnd: number;
  /** The buffer this clip came from */
  bufferId: number;
}

export interface TimelineTrack {
  id: string;
  name: string;
  clips: TimelineClip[];
  muted: boolean;
  solo: boolean;
  amp: number;
  effects: ClipEffect[];
  color: string;
  /** Optional section label */
  section?: string;
}

export interface SectionMarker {
  label: string;
  beatStart: number;
}

export interface TimelineData {
  tracks: TimelineTrack[];
  bpm: number;
  totalBeats: number;
  sections: SectionMarker[];
}

// ─── Color palette ───────────────────────────────────────────────

const TRACK_COLORS = [
  '#00ff88', '#4488ff', '#aa66ff', '#ff8844',
  '#ffcc00', '#00ccff', '#ff4466', '#88cc44',
  '#ff66aa', '#44ddbb', '#cc88ff', '#ff9966',
];

let _colorIdx = 0;
function nextColor(): string {
  return TRACK_COLORS[_colorIdx++ % TRACK_COLORS.length];
}

// ─── Helpers ─────────────────────────────────────────────────────

function parseSleepValue(line: string): number | null {
  const m = line.match(/^\s*sleep\s+([\d.]+)/);
  return m ? parseFloat(m[1]) : null;
}

function parseAmp(text: string): number {
  const m = text.match(/amp:\s*([\d.]+)/);
  return m ? parseFloat(m[1]) : 1;
}

function parseRelease(text: string): number {
  const m = text.match(/release:\s*([\d.]+)/);
  return m ? parseFloat(m[1]) : 0.3;
}

function parseSustain(text: string): number {
  const m = text.match(/sustain:\s*([\d.]+)/);
  return m ? parseFloat(m[1]) : 0;
}

function parseAttack(text: string): number {
  const m = text.match(/attack:\s*([\d.]+)/);
  return m ? parseFloat(m[1]) : 0;
}

function parseRate(text: string): number {
  const m = text.match(/rate:\s*([\d.]+)/);
  return m ? parseFloat(m[1]) : 1;
}

function sampleDisplayName(line: string): string {
  const sym = line.match(/sample\s+:(\w+)/);
  if (sym) return sym[1];
  const str = line.match(/["']([^"']+)["']/);
  if (str) {
    const parts = str[1].replace(/\\/g, '/').split('/');
    return parts[parts.length - 1].replace(/\.\w+$/, '');
  }
  return 'sample';
}

function extractAllSampleNames(lines: string[]): string[] {
  const names: string[] = [];
  for (const l of lines) {
    if (/\bsample\b/.test(l)) names.push(sampleDisplayName(l));
  }
  return [...new Set(names)];
}

function detectClipType(lines: string[]): 'sample' | 'synth' | 'mixed' {
  let hasSample = false, hasSynth = false;
  for (const l of lines) {
    if (/\bsample\b/.test(l)) hasSample = true;
    if (/\bplay\b|\bplay_pattern/.test(l)) hasSynth = true;
  }
  if (hasSample && hasSynth) return 'mixed';
  if (hasSample) return 'sample';
  return 'synth';
}

function extractEffects(lines: string[]): ClipEffect[] {
  const effects: ClipEffect[] = [];
  for (const l of lines) {
    const m = l.match(/with_fx\s+:(\w+)(?:,\s*(.*))?/);
    if (m) {
      const params: Record<string, number> = {};
      if (m[2]) {
        for (const pm of m[2].matchAll(/(\w+):\s*([\d.]+)/g)) {
          params[pm[1]] = parseFloat(pm[2]);
        }
      }
      effects.push({ type: m[1], params });
    }
  }
  return effects;
}

function playDurationBeats(line: string): number {
  return parseAttack(line) + parseSustain(line) + parseRelease(line);
}

function patternTimedDuration(line: string): number {
  const m = line.match(/play_pattern_timed\s+\[([^\]]*)\]\s*,\s*\[([^\]]*)\]/);
  if (!m) return 0;
  const times = m[2].split(',').map(s => parseFloat(s.trim())).filter(n => !isNaN(n));
  return times.reduce((a, b) => a + b, 0);
}

function builtinSampleDurationBeats(name: string, bpm: number): number {
  const secPerBeat = 60 / bpm;
  const known: Record<string, number> = {
    bd_haus: 0.3, bd_ada: 0.3, bd_boom: 0.4, bd_808: 0.4,
    sn_dub: 0.25, drum_snare_soft: 0.25, perc_snap: 0.15,
  };
  const secs = known[name] ?? 2;
  return secs / secPerBeat;
}

function parseSectionLabel(line: string): string | null {
  const m = line.match(/##\s*-+\s*(.*?)\s*-+\s*##/);
  if (m) return m[1].trim();
  return null;
}

// ─── Block duration calculator ───────────────────────────────────

function calculateBlockDuration(lines: string[], _bpm: number): number {
  let cursor = 0;
  for (const raw of lines) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const sv = parseSleepValue(line);
    if (sv !== null) { cursor += sv; continue; }
    if (/play_pattern_timed/.test(line)) { cursor += patternTimedDuration(line); continue; }
  }
  return cursor;
}

// ─── Extract N.times wrappers ────────────────────────────────────

function extractTimesBlocks(lines: string[]): { count: number; innerLines: string[]; outerLines: string[] }[] {
  const results: { count: number; innerLines: string[]; outerLines: string[] }[] = [];
  let i = 0;
  const outerBuf: string[] = [];
  while (i < lines.length) {
    const t = lines[i].trim();
    const m = t.match(/^(\d+)\.times\s+do/);
    if (m) {
      if (outerBuf.length > 0) {
        results.push({ count: 1, innerLines: [...outerBuf], outerLines: [] });
        outerBuf.length = 0;
      }
      const count = parseInt(m[1]);
      const inner: string[] = [];
      let depth = 1;
      i++;
      while (i < lines.length && depth > 0) {
        const lt = lines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') { depth--; if (depth === 0) { i++; break; } }
        inner.push(lines[i]);
        i++;
      }
      results.push({ count, innerLines: inner, outerLines: [] });
    } else {
      outerBuf.push(lines[i]);
      i++;
    }
  }
  if (outerBuf.length > 0) {
    results.push({ count: 1, innerLines: outerBuf, outerLines: [] });
  }
  return results;
}

// ─── Main parser ─────────────────────────────────────────────────

export function parseCodeToTimeline(code: string, bufferId: number): TimelineData {
  // Pre-process: join continuation lines (lines ending with ',')
  const preLines = code.split('\n');
  const joinedLines: string[] = [];
  for (let j = 0; j < preLines.length; j++) {
    let current = preLines[j];
    while (j + 1 < preLines.length && current.trimEnd().endsWith(',')) {
      current = current.trimEnd() + ' ' + preLines[j + 1].trim();
      j++;
    }
    joinedLines.push(current);
  }
  const rawLines = joinedLines;
  let bpm = 120;
  const sections: SectionMarker[] = [];
  const tracks: TimelineTrack[] = [];

  // First pass: find globals
  for (const line of rawLines) {
    const bpmM = line.match(/use_bpm\s+(\d+)/);
    if (bpmM) bpm = parseInt(bpmM[1]);
  }

  _colorIdx = 0;
  let globalCursor = 0;
  let currentSection = '';
  let clipCounter = 0;
  let trackCounter = 0;
  const nextClipId = () => `b${bufferId}_c${clipCounter++}`;
  const nextTrackId = () => `b${bufferId}_t${trackCounter++}`;

  // Track registry to merge clips into named tracks
  const trackMap = new Map<string, TimelineTrack>();
  // Stored function definitions from `define :name do ... end`
  const definedFunctions = new Map<string, string[]>();

  function getOrCreateTrack(name: string, section?: string): TimelineTrack {
    let t = trackMap.get(name);
    if (!t) {
      t = {
        id: nextTrackId(),
        name,
        clips: [],
        muted: false,
        solo: false,
        amp: 1,
        effects: [],
        color: nextColor(),
        section,
      };
      trackMap.set(name, t);
      tracks.push(t);
    }
    return t;
  }

  // ── Walk top-level lines ──
  let i = 0;
  while (i < rawLines.length) {
    const line = rawLines[i];
    const trimmed = line.trim();

    // Section comment
    const secLabel = parseSectionLabel(trimmed);
    if (secLabel) {
      currentSection = secLabel;
      sections.push({ label: secLabel, beatStart: globalCursor });
      i++; continue;
    }

    // Skip blanks, comments, pragmas
    if (!trimmed || trimmed.startsWith('#') || /^use_bpm\b/.test(trimmed)
        || /^use_synth\b/.test(trimmed) || /^sample_path\s*=/.test(trimmed)
        || /^use_synth_defaults\b/.test(trimmed) || /^use_sample_defaults\b/.test(trimmed)
        || /^use_merged_synth_defaults\b/.test(trimmed) || /^use_merged_sample_defaults\b/.test(trimmed)
        || /^use_random_seed\b/.test(trimmed) || /^use_random_source\b/.test(trimmed)
        || /^use_timing_guarantees\b/.test(trimmed) || /^use_arg_checks\b/.test(trimmed)
        || /^use_debug\b/.test(trimmed) || /^use_cue_logging\b/.test(trimmed)
        || /^use_external_synths\b/.test(trimmed) || /^use_arg_bpm_scaling\b/.test(trimmed)
        || /^cue\b/.test(trimmed) || /^set\b/.test(trimmed) || /^get\b/.test(trimmed)
        || /^control\b/.test(trimmed) || /^midi\b/.test(trimmed)
        || /^tick\b/.test(trimmed) || /^look\b/.test(trimmed)
        || /^stop\b/.test(trimmed)) {
      i++; continue;
    }

    // ── Variable assignments (ring, spread, or regular) ──
    if (/^\w+\s*=\s*/.test(trimmed) && !/^(play|sample|sleep|use_|live_|with_|in_thread|define|def|if|loop)/.test(trimmed)) {
      // Skip ring/spread/regular variable assignments
      i++; continue;
    }

    // ── define :name do ... end — store function body ──
    const defineMatch = trimmed.match(/^define\s+:(\w+)\s+do/);
    if (defineMatch) {
      const funcName = defineMatch[1];
      const blockLines: string[] = [];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt) || /\bthen\s*$/.test(lt) || /^def\s+/.test(lt)) depth++;
        if (lt === 'end') { depth--; if (depth === 0) { i++; break; } }
        blockLines.push(rawLines[i]);
        i++;
      }
      definedFunctions.set(funcName, blockLines);
      continue;
    }

    // ── Ruby-style def name(args) ... end — store function body ──
    const defMatch = trimmed.match(/^def\s+(\w+[?!]?)/);
    if (defMatch) {
      const funcName = defMatch[1];
      const blockLines: string[] = [];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt) || /\bthen\s*$/.test(lt) || /^def\s+/.test(lt)) depth++;
        if (lt === 'end') { depth--; if (depth === 0) { i++; break; } }
        blockLines.push(rawLines[i]);
        i++;
      }
      definedFunctions.set(funcName, blockLines);
      continue;
    }

    // ── if ... do ... end blocks — skip the block structure, include contents ──
    if (/^if\s+/.test(trimmed) && (/\bdo\s*$/.test(trimmed) || /\bthen\s*$/.test(trimmed))) {
      const blockLines: string[] = [line];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        // Don't increase depth for elsif/else at our level
        const isElsifElse = (lt.startsWith('elsif') || lt === 'else') && depth === 1;
        if (!isElsifElse && (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt) || /\bthen\s*$/.test(lt))) depth++;
        if (lt === 'end') depth--;
        blockLines.push(rawLines[i]);
        i++;
      }
      // Include the inner content (optimistic: assume condition is true for timeline)
      const innerLines = blockLines.slice(1, -1)
        .filter(l => !l.trim().startsWith('elsif') && l.trim() !== 'else');
      const dur = calculateBlockDuration(innerLines, bpm);
      if (dur > 0) {
        const track = getOrCreateTrack('Conditional', currentSection);
        track.clips.push({
          id: nextClipId(),
          name: 'if block', startBeat: globalCursor,
          durationBeats: Math.max(dur, 0.5),
          code: blockLines.join('\n'), type: detectClipType(innerLines),
          color: track.color, amp: parseAmp(innerLines.join(' ')),
          effects: extractEffects(innerLines), isLooping: false,
          loopCount: 1, samples: extractAllSampleNames(innerLines),
          srcLineStart: i - blockLines.length, srcLineEnd: i - 1, bufferId,
        });
        globalCursor += dur;
      }
      continue;
    }

    // ── unless ... do ... end ──
    if (/^unless\s+/.test(trimmed) && (/\bdo\s*$/.test(trimmed) || /\bthen\s*$/.test(trimmed))) {
      const blockLines: string[] = [line];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt) || /\bthen\s*$/.test(lt)) depth++;
        if (lt === 'end') depth--;
        blockLines.push(rawLines[i]);
        i++;
      }
      const innerLines = blockLines.slice(1, -1);
      const dur = calculateBlockDuration(innerLines, bpm);
      if (dur > 0) {
        const track = getOrCreateTrack('Conditional', currentSection);
        track.clips.push({
          id: nextClipId(),
          name: 'unless block', startBeat: globalCursor,
          durationBeats: Math.max(dur, 0.5),
          code: blockLines.join('\n'), type: detectClipType(innerLines),
          color: track.color, amp: parseAmp(innerLines.join(' ')),
          effects: extractEffects(innerLines), isLooping: false,
          loopCount: 1, samples: extractAllSampleNames(innerLines),
          srcLineStart: i - blockLines.length, srcLineEnd: i - 1, bufferId,
        });
        globalCursor += dur;
      }
      continue;
    }

    // ── with_synth :name do ... end ──
    if (/^with_synth\s+/.test(trimmed)) {
      const blockLines: string[] = [line];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') depth--;
        blockLines.push(rawLines[i]);
        i++;
      }
      const innerLines = blockLines.slice(1, -1);
      const dur = calculateBlockDuration(innerLines, bpm);
      if (dur > 0) {
        const synthName = trimmed.match(/with_synth\s+:(\w+)/)?.[1] || 'synth';
        const track = getOrCreateTrack(`Synth: ${synthName}`, currentSection);
        track.clips.push({
          id: nextClipId(),
          name: synthName, startBeat: globalCursor,
          durationBeats: Math.max(dur, 0.5),
          code: blockLines.join('\n'), type: 'synth',
          color: track.color, amp: parseAmp(innerLines.join(' ')),
          effects: [], isLooping: false, loopCount: 1,
          samples: [], srcLineStart: i - blockLines.length, srcLineEnd: i - 1, bufferId,
        });
        globalCursor += dur;
      }
      continue;
    }

    // ── with_bpm N do ... end ──
    if (/^with_bpm\b/.test(trimmed) || /^with_bpm_mul\b/.test(trimmed)) {
      const blockLines: string[] = [line];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') depth--;
        blockLines.push(rawLines[i]);
        i++;
      }
      const innerLines = blockLines.slice(1, -1);
      const dur = calculateBlockDuration(innerLines, bpm);
      if (dur > 0) {
        const track = getOrCreateTrack('BPM Block', currentSection);
        track.clips.push({
          id: nextClipId(),
          name: 'bpm block', startBeat: globalCursor,
          durationBeats: Math.max(dur, 0.5),
          code: blockLines.join('\n'), type: detectClipType(innerLines),
          color: track.color, amp: parseAmp(innerLines.join(' ')),
          effects: [], isLooping: false, loopCount: 1,
          samples: extractAllSampleNames(innerLines),
          srcLineStart: i - blockLines.length, srcLineEnd: i - 1, bufferId,
        });
        globalCursor += dur;
      }
      continue;
    }

    // ── .each do |x| ... end (list iteration) ──
    if (/\.each(_with_index)?\s+do/.test(trimmed)) {
      const blockLines: string[] = [line];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') depth--;
        blockLines.push(rawLines[i]);
        i++;
      }
      const innerLines = blockLines.slice(1, -1);
      const dur = calculateBlockDuration(innerLines, bpm);
      if (dur > 0) {
        const track = getOrCreateTrack('Iteration', currentSection);
        track.clips.push({
          id: nextClipId(),
          name: 'each', startBeat: globalCursor,
          durationBeats: Math.max(dur, 0.5),
          code: blockLines.join('\n'), type: detectClipType(innerLines),
          color: track.color, amp: parseAmp(innerLines.join(' ')),
          effects: extractEffects(innerLines), isLooping: false,
          loopCount: 1, samples: extractAllSampleNames(innerLines),
          srcLineStart: i - blockLines.length, srcLineEnd: i - 1, bufferId,
        });
        globalCursor += dur;
      }
      continue;
    }

    // ── Top-level sleep ──
    const sv = parseSleepValue(trimmed);
    if (sv !== null) {
      globalCursor += sv;
      i++; continue;
    }

    // ── live_loop ──
    const llMatch = trimmed.match(/live_loop\s+:(\w+)\s+do/);
    if (llMatch) {
      const loopName = llMatch[1];
      const blockStart = i;
      const blockLines: string[] = [line];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') depth--;
        blockLines.push(rawLines[i]);
        i++;
      }
      const innerLines = blockLines.slice(1, -1);
      const hasStop = innerLines.some(l => l.trim() === 'stop');
      const effects = extractEffects(innerLines);
      const samples = extractAllSampleNames(innerLines);
      const clipType = detectClipType(innerLines);

      // Calculate duration considering N.times blocks
      const contentLines = innerLines.filter(l => l.trim() !== 'stop');
      const timesBlocks = extractTimesBlocks(contentLines);
      let totalDur = 0;

      // Check for initial sleep (internal offset)
      let internalOffset = 0;
      const firstContent = innerLines.find(l => l.trim() && !l.trim().startsWith('#'));
      if (firstContent) {
        const firstSV = parseSleepValue(firstContent.trim());
        if (firstSV !== null && !/sample|play/.test(firstContent)) {
          internalOffset = firstSV;
        }
      }

      for (const tb of timesBlocks) {
        const dur = calculateBlockDuration(tb.innerLines, bpm);
        totalDur += dur * tb.count;
      }
      if (totalDur === 0) {
        totalDur = calculateBlockDuration(innerLines, bpm);
      }

      const displayName = loopName.replace(/_/g, ' ');
      const track = getOrCreateTrack(displayName, currentSection);
      const amp = parseAmp(innerLines.join(' '));

      const clip: TimelineClip = {
        id: nextClipId(),
        name: displayName,
        startBeat: globalCursor + internalOffset,
        durationBeats: Math.max(totalDur, 0.5),
        code: blockLines.join('\n'),
        type: clipType,
        color: track.color,
        amp: amp || 1,
        effects,
        isLooping: !hasStop,
        loopCount: hasStop ? 1 : 0,
        samples,
        srcLineStart: blockStart,
        srcLineEnd: blockStart + blockLines.length - 1,
        bufferId,
      };
      track.clips.push(clip);
      continue;
    }

    // ── Standalone sample ──
    if (/^sample\b/.test(trimmed)) {
      const name = sampleDisplayName(trimmed);
      const amp = parseAmp(trimmed);
      const rate = parseRate(trimmed);
      const symMatch = trimmed.match(/sample\s+:(\w+)/);
      const durBeats = symMatch
        ? builtinSampleDurationBeats(symMatch[1], bpm)
        : (2 * (60 / bpm));
      const adjustedDur = durBeats / Math.abs(rate || 1);

      const track = getOrCreateTrack('Samples', currentSection);
      track.clips.push({
        id: nextClipId(),
        name, startBeat: globalCursor,
        durationBeats: Math.max(adjustedDur, 0.25),
        code: line, type: 'sample', color: track.color,
        amp, effects: [], isLooping: false, loopCount: 1,
        samples: [name], srcLineStart: i, srcLineEnd: i, bufferId,
      });
      i++; continue;
    }

    // ── Standalone play / play chord ──
    if (/^play\b/.test(trimmed)) {
      const dur = playDurationBeats(trimmed);
      const amp = parseAmp(trimmed);
      let noteName = 'note';
      const noteMatch = trimmed.match(/play\s+:(\w+)/);
      if (noteMatch) noteName = noteMatch[1];
      const chordMatch = trimmed.match(/play\s+chord\(\s*:(\w+)/);
      if (chordMatch) noteName = chordMatch[1] + ' chord';

      const track = getOrCreateTrack('Synth', currentSection);
      track.clips.push({
        id: nextClipId(),
        name: noteName, startBeat: globalCursor,
        durationBeats: Math.max(dur, 0.25),
        code: line, type: 'synth', color: track.color,
        amp, effects: [], isLooping: false, loopCount: 1,
        samples: [], srcLineStart: i, srcLineEnd: i, bufferId,
      });
      i++; continue;
    }

    // ── Standalone play_pattern_timed ──
    if (/^play_pattern_timed\b/.test(trimmed)) {
      const dur = patternTimedDuration(trimmed);
      const amp = parseAmp(trimmed);
      const track = getOrCreateTrack('Synth Pattern', currentSection);
      track.clips.push({
        id: nextClipId(),
        name: 'pattern', startBeat: globalCursor,
        durationBeats: Math.max(dur, 0.5),
        code: line, type: 'synth', color: track.color,
        amp, effects: [], isLooping: false, loopCount: 1,
        samples: [], srcLineStart: i, srcLineEnd: i, bufferId,
      });
      globalCursor += dur;
      i++; continue;
    }

    // ── Top-level with_fx block ──
    if (/^with_fx\s+:(\w+)/.test(trimmed)) {
      const blockStart = i;
      const blockLines: string[] = [line];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') depth--;
        blockLines.push(rawLines[i]);
        i++;
      }
      const innerLines = blockLines.slice(1, -1);
      const effects = extractEffects(blockLines);
      const dur = calculateBlockDuration(innerLines, bpm);
      const fxName = trimmed.match(/with_fx\s+:(\w+)/)?.[1] || 'fx';

      const track = getOrCreateTrack(`FX: ${fxName}`, currentSection);
      track.clips.push({
        id: nextClipId(),
        name: fxName, startBeat: globalCursor,
        durationBeats: Math.max(dur, 0.5),
        code: blockLines.join('\n'), type: detectClipType(innerLines),
        color: track.color, amp: parseAmp(innerLines.join(' ')),
        effects, isLooping: false, loopCount: 1,
        samples: extractAllSampleNames(innerLines),
        srcLineStart: blockStart, srcLineEnd: blockStart + blockLines.length - 1,
        bufferId,
      });
      globalCursor += dur;
      continue;
    }

    // ── Top-level N.times do block ──
    const timesMatch = trimmed.match(/^(\d+)\.times\s+do/);
    if (timesMatch) {
      const count = parseInt(timesMatch[1]);
      const blockStart = i;
      const blockLines: string[] = [line];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') depth--;
        blockLines.push(rawLines[i]);
        i++;
      }
      const innerLines = blockLines.slice(1, -1);
      const singleDur = calculateBlockDuration(innerLines, bpm);
      const totalDur = singleDur * count;

      const track = getOrCreateTrack('Loop', currentSection);
      track.clips.push({
        id: nextClipId(),
        name: `${count}x loop`, startBeat: globalCursor,
        durationBeats: Math.max(totalDur, 0.5),
        code: blockLines.join('\n'), type: detectClipType(innerLines),
        color: track.color, amp: parseAmp(innerLines.join(' ')),
        effects: extractEffects(innerLines), isLooping: false,
        loopCount: count, samples: extractAllSampleNames(innerLines),
        srcLineStart: blockStart, srcLineEnd: blockStart + blockLines.length - 1,
        bufferId,
      });
      globalCursor += totalDur;
      continue;
    }

    // ── Function call (from define) ──
    const funcCallMatch = trimmed.match(/^(\w+)\s*$/);
    if (funcCallMatch && definedFunctions.has(funcCallMatch[1])) {
      const funcName = funcCallMatch[1];
      const funcLines = definedFunctions.get(funcName)!;
      const effects = extractEffects(funcLines);
      const samples = extractAllSampleNames(funcLines);
      const clipType = detectClipType(funcLines);
      const dur = calculateBlockDuration(funcLines, bpm);
      const displayName = funcName.replace(/_/g, ' ');

      const track = getOrCreateTrack(displayName, currentSection);
      track.clips.push({
        id: nextClipId(),
        name: displayName, startBeat: globalCursor,
        durationBeats: Math.max(dur, 0.5),
        code: funcLines.join('\n'), type: clipType,
        color: track.color, amp: parseAmp(funcLines.join(' ')),
        effects, isLooping: false, loopCount: 1,
        samples, srcLineStart: i, srcLineEnd: i, bufferId,
      });
      globalCursor += dur;
      i++; continue;
    }

    // ── Function call inside N.times do (e.g. "4.times do\n  guitar_riff\nend") ──
    // already handled by N.times do block above

    // ── Any other do/end block ──
    if (/\bdo\s*$/.test(trimmed) || /\bthen\s*$/.test(trimmed)) {
      const blockLines: string[] = [line];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') depth--;
        blockLines.push(rawLines[i]);
        i++;
      }
      continue;
    }

    i++;
  }

  // Calculate total beats
  let totalBeats = globalCursor;
  for (const track of tracks) {
    for (const clip of track.clips) {
      totalBeats = Math.max(totalBeats, clip.startBeat + clip.durationBeats);
    }
  }

  return { tracks, bpm, totalBeats: Math.max(totalBeats, 16), sections };
}

// ─── Merge multiple buffer timelines ─────────────────────────────

export function mergeTimelines(timelines: TimelineData[]): TimelineData {
  if (timelines.length === 0) {
    return { tracks: [], bpm: 120, totalBeats: 32, sections: [] };
  }
  const merged: TimelineData = {
    tracks: [], bpm: timelines[0].bpm, totalBeats: 0, sections: [],
  };
  for (const tl of timelines) {
    merged.tracks.push(...tl.tracks);
    merged.sections.push(...tl.sections);
    merged.totalBeats = Math.max(merged.totalBeats, tl.totalBeats);
  }
  return merged;
}

// ─── Code generation from timeline (write-back) ─────────────────

export function timelineToCode(timeline: TimelineData): string {
  const lines: string[] = [];
  lines.push(`use_bpm ${timeline.bpm}`);
  lines.push('');

  const allClips: { clip: TimelineClip; track: TimelineTrack }[] = [];
  for (const track of timeline.tracks) {
    if (track.muted) continue;
    for (const clip of track.clips) {
      allClips.push({ clip, track });
    }
  }
  allClips.sort((a, b) => a.clip.startBeat - b.clip.startBeat);

  let cursor = 0;
  let lastSection = '';

  for (const { clip, track } of allClips) {
    const gap = clip.startBeat - cursor;
    if (gap > 0.01) {
      lines.push(`sleep ${fmtNum(gap)}`);
      lines.push('');
    }

    if (track.section && track.section !== lastSection) {
      lines.push(`## ---- ${track.section} ---- ##`);
      lastSection = track.section;
    }

    const clipCode = updateClipAmp(clip.code, clip.amp * track.amp);

    if (track.effects.length > 0 && !clip.code.includes('with_fx')) {
      for (const fx of track.effects) {
        const ps = Object.entries(fx.params).map(([k, v]) => `${k}: ${fmtNum(v)}`).join(', ');
        lines.push(`with_fx :${fx.type}${ps ? ', ' + ps : ''} do`);
      }
      lines.push(clipCode);
      for (let j = 0; j < track.effects.length; j++) lines.push('end');
    } else {
      lines.push(clipCode);
    }
    lines.push('');
    cursor = clip.startBeat;
  }

  return lines.join('\n');
}

function updateClipAmp(code: string, newAmp: number): string {
  return code.replace(/amp:\s*[\d.]+/g, `amp: ${fmtNum(newAmp)}`);
}

function fmtNum(n: number): string {
  if (Number.isInteger(n)) return n.toString();
  return n.toFixed(2).replace(/0+$/, '').replace(/\.$/, '');
}
