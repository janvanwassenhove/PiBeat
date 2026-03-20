/**
 * timelineParser.ts  –  v3
 *
 * Event-level parser: every audible event in Sonic Pi code becomes an
 * individual clip on the timeline, positioned at its exact beat offset.
 *
 * Key design:
 *  - Each `play`, `sample`, `play_pattern_timed` becomes its own clip
 *  - Inside live_loop / loop: events repeat across the visible horizon
 *  - Parallel blocks (live_loop, in_thread) don't advance the global cursor
 *  - Each clip carries srcLineStart/srcLineEnd for playhead↔code alignment
 *  - sleep advances the beat cursor; BPM is respected for timing
 *
 * Handles:
 *  - live_loop blocks            → repeated individual event clips
 *  - loop do blocks              → repeated individual event clips
 *  - Standalone sample calls     → individual sample clips
 *  - Standalone play / chord     → synth note clips
 *  - play_pattern_timed          → synth pattern clips
 *  - N.times do wrappers         → unrolled event clips
 *  - Top-level sleep             → advances the global timeline cursor
 *  - with_fx wrappers            → effects attached to inner clips
 *  - in_thread blocks            → parallel event clips
 *  - define blocks               → expanded at call sites
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
  /** Extra beats of audio ringing out past the code-execution window */
  audioTailBeats: number;
  /** Whether this clip references file-path samples that need preloading */
  needsPreload: boolean;
  /** Map of sample-name → duration in seconds (populated async from backend) */
  sampleDurationMap: Record<string, number>;
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
  // Handle negative rate (reverse playback)
  const m = text.match(/rate:\s*(-?[\d.]+)/);
  return m ? parseFloat(m[1]) : 1;
}

function parseBeatStretch(text: string): number | null {
  const m = text.match(/beat_stretch:\s*([\d.]+)/);
  return m ? parseFloat(m[1]) : null;
}

function parseStart(text: string): number {
  const m = text.match(/start:\s*([\d.]+)/);
  return m ? parseFloat(m[1]) : 0;
}

function parseFinish(text: string): number {
  const m = text.match(/finish:\s*([\d.]+)/);
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

function playDurationBeats(line: string): number {
  const atk = parseAttack(line);
  const sus = parseSustain(line);
  const rel = parseRelease(line);
  const total = atk + sus + rel;
  // Default envelope: a=0, s=0, r=0.3 → 0.3 beats, but a bare `play :c4`
  // uses the synth default which is ~0.5 beats
  return total > 0.01 ? total : 0.5;
}

function patternTimedDuration(line: string): number {
  const m = line.match(/play_pattern_timed\s+\[([^\]]*)\]\s*,\s*\[([^\]]*)\]/);
  if (!m) return 0;
  const notes = m[1].split(',').filter(s => s.trim().length > 0);
  const times = m[2].split(',').map(s => parseFloat(s.trim())).filter(n => !isNaN(n));
  if (times.length === 0 || notes.length === 0) return 0;
  // In Sonic Pi, times cycle if there are more notes than time values.
  // The total duration is the sum of one time value per note.
  let total = 0;
  for (let i = 0; i < notes.length; i++) {
    total += times[i % times.length];
  }
  return total;
}

/** Comprehensive built-in sample durations in seconds.
 *  These are the actual durations of samples generated by ensure_default_samples(). */
const BUILTIN_SAMPLE_DURATIONS: Record<string, number> = {
  // Bass drums
  bd_haus: 0.5, bd_ada: 0.3, bd_boom: 0.8, bd_808: 0.75,
  bd_chip: 0.15, bd_fat: 0.4, bd_gas: 0.35, bd_klub: 0.35,
  bd_mehackit: 0.5, bd_pure: 0.4, bd_sone: 0.3, bd_tek: 0.25,
  bd_zome: 0.35, bd_zum: 0.4,
  // Snares
  sn_dub: 0.3, sn_dolf: 0.25, sn_zome: 0.25, sn_generic: 0.3,
  drum_snare_soft: 0.3, drum_snare_hard: 0.3,
  // Hats & percussion
  hat_bdu: 0.15, hat_cab: 0.1, hat_cats: 0.12, hat_em: 0.1,
  hat_gnu: 0.08, hat_metal: 0.2, hat_noiz: 0.15, hat_raw: 0.1,
  hat_snap: 0.05, hat_star: 0.12, hat_tap: 0.08, hat_zan: 0.1,
  drum_cymbal_hard: 1.5, drum_cymbal_soft: 1.2, drum_cymbal_open: 2.0,
  perc_snap: 0.15, perc_snap2: 0.2, perc_bell: 1.0, perc_bell2: 0.8,
  perc_door: 0.25, perc_impact1: 0.5, perc_impact2: 0.4,
  perc_swoosh: 0.5, perc_till: 0.3,
  // Electronic
  elec_beep: 0.15, elec_bell: 0.5, elec_blip: 0.1, elec_blip2: 0.12,
  elec_blup: 0.2, elec_bong: 0.4, elec_chime: 0.8, elec_cymbal: 1.0,
  elec_filt_snare: 0.3, elec_flip: 0.15, elec_fuzz_tom: 0.3,
  elec_hollow_kick: 0.4, elec_lo_snare: 0.3, elec_mid_snare: 0.25,
  elec_ping: 0.2, elec_plip: 0.1, elec_pop: 0.15, elec_snare: 0.25,
  elec_soft_kick: 0.35, elec_tick: 0.05, elec_triangle: 0.3,
  elec_twang: 0.3, elec_twip: 0.1, elec_wood: 0.2,
  // Ambient
  ambi_choir: 4.0, ambi_dark_woosh: 3.0, ambi_drone: 5.0,
  ambi_glass_hum: 3.5, ambi_glass_rub: 2.5, ambi_haunted_hum: 4.0,
  ambi_lunar_land: 4.5, ambi_piano: 3.0, ambi_sauna: 4.0,
  ambi_soft_buzz: 3.0, ambi_swoosh: 2.0,
  // Bass
  bass_hit_c: 0.5, bass_hard_c: 0.4, bass_thick_c: 0.6,
  bass_drop_c: 0.8, bass_woodsy_c: 0.5, bass_voxy_c: 0.7,
  bass_voxy_hit_c: 0.4, bass_dnb_f: 0.5, bass_trance_c: 0.6,
  // Loops
  loop_amen: 1.753, loop_amen_full: 3.507, loop_breakbeat: 1.882,
  loop_compus: 1.882, loop_garzul: 1.882, loop_industrial: 0.941,
  loop_mika: 1.882, loop_safari: 1.882, loop_tabla: 3.764,
  loop_mehackit1: 1.882, loop_mehackit2: 1.882,
  // Tabla
  tabla_dhec: 0.3, tabla_ghe1: 0.35, tabla_ghe2: 0.4,
  tabla_ghe3: 0.35, tabla_ghe4: 0.3, tabla_ghe5: 0.4,
  tabla_ghe6: 0.35, tabla_ghe7: 0.3, tabla_ghe8: 0.4,
  tabla_ke1: 0.2, tabla_ke2: 0.25, tabla_ke3: 0.2,
  tabla_na: 0.3, tabla_na_o: 0.35, tabla_na_s: 0.3,
  tabla_re: 0.25, tabla_tas1: 0.2, tabla_tas2: 0.25,
  tabla_tas3: 0.2, tabla_te1: 0.2, tabla_te2: 0.25,
  tabla_te_m: 0.2, tabla_te_ne: 0.3, tabla_tun1: 0.35,
  tabla_tun2: 0.4, tabla_tun3: 0.35,
  // Vinyl
  vinyl_backspin: 1.5, vinyl_hiss: 3.0, vinyl_rewind: 1.0, vinyl_scratch: 0.5,
  // Glitch
  glitch_bass_g: 0.3, glitch_perc1: 0.15, glitch_perc2: 0.2,
  glitch_perc3: 0.1, glitch_perc4: 0.25, glitch_perc5: 0.15,
  glitch_robot1: 0.4, glitch_robot2: 0.35,
  // Misc
  misc_burp: 0.5, misc_cineboom: 2.0, misc_crow: 1.5,
  // Common aliases / Sonic Pi shorthand
  kick: 0.5, snare: 0.3, hihat: 0.1, clap: 0.3,
};

function parseSectionLabel(line: string): string | null {
  const m = line.match(/##\s*-+\s*(.*?)\s*-+\s*##/);
  if (m) return m[1].trim();
  return null;
}

/** Returns true if the sample identifier is a file path (not a built-in name) */
export function isFilePathSample(name: string): boolean {
  return name.includes('/') || name.includes('\\') || name.includes('.');
}

/** Collect all distinct sample identifiers from lines (both built-in and file paths) */
export function extractSampleIdentifiers(lines: string[]): string[] {
  const ids: string[] = [];
  for (const l of lines) {
    if (!/\bsample\b/.test(l)) continue;
    // Built-in symbol
    const sym = l.match(/sample\s+:(\w+)/);
    if (sym) { ids.push(sym[1]); continue; }
    // String path (with or without variable concatenation)
    // Try to extract the file name from string literal
    const str = l.match(/["']([^"']+)["']/);
    if (str) {
      ids.push(str[1].replace(/\\/g, '/').split('/').pop()?.replace(/\.\w+$/, '') || str[1]);
      continue;
    }
    // Variable-based path + string concat: sample sample_path + "file.wav"
    const concat = l.match(/sample\s+\w+\s*\+\s*["']([^"']+)["']/);
    if (concat) {
      ids.push(concat[1].replace(/\\/g, '/').split('/').pop()?.replace(/\.\w+$/, '') || concat[1]);
    }
  }
  return [...new Set(ids)];
}

/** Get duration of a sample in beats, accounting for beat_stretch, sustain, rate, start/finish */
function getSampleDurationBeats(line: string, bpm: number, sampleDurations?: Record<string, number>): number {
  const secPerBeat = 60 / bpm;
  const rate = parseRate(line);
  const beatStretch = parseBeatStretch(line);
  const sustain = parseSustain(line);
  const startFrac = parseStart(line);
  const finishFrac = parseFinish(line);

  // beat_stretch overrides: the sample is stretched to fit exactly N beats
  if (beatStretch !== null && beatStretch > 0) {
    // sustain can further truncate even a beat-stretched sample
    if (sustain > 0) return Math.min(beatStretch, sustain);
    return beatStretch;
  }

  // sustain truncates: the sample plays for exactly N beats
  if (sustain > 0) {
    return sustain;
  }

  // Resolve raw sample duration in seconds
  let rawDurSecs = 0.5; // fallback
  const sym = line.match(/sample\s+:(\w+)/);
  if (sym) {
    if (sampleDurations && sampleDurations[sym[1]] !== undefined && sampleDurations[sym[1]] > 0) {
      rawDurSecs = sampleDurations[sym[1]];
    } else {
      rawDurSecs = BUILTIN_SAMPLE_DURATIONS[sym[1]] ?? 0.5;
    }
  } else if (sampleDurations) {
    const str = line.match(/["']([^"']+)["']/);
    if (str) {
      const fullPath = str[1].replace(/\\/g, '/');
      const fname = fullPath.split('/').pop()?.replace(/\.\w+$/, '') || '';
      // Try exact full-path match first (keys from backend are full paths)
      if (sampleDurations[str[1]] !== undefined) {
        rawDurSecs = sampleDurations[str[1]];
      } else if (sampleDurations[fullPath] !== undefined) {
        rawDurSecs = sampleDurations[fullPath];
      } else {
        // Fall back to fuzzy filename matching
        for (const [key, dur] of Object.entries(sampleDurations)) {
          const keyNorm = key.replace(/\\/g, '/');
          if (keyNorm === fullPath || key === fname || keyNorm.endsWith('/' + fname) || fname.endsWith(key)) {
            rawDurSecs = dur;
            break;
          }
        }
      }
    }
  }

  // Apply start/finish trimming
  const effectiveFraction = Math.abs(finishFrac - startFrac);
  const trimmedSecs = rawDurSecs * effectiveFraction;

  // Apply rate (absolute value — reverse playback takes the same time)
  const effectiveSecs = trimmedSecs / Math.abs(rate || 1);

  return effectiveSecs / secPerBeat;
}

// ─── Main parser ─────────────────────────────────────────────────

/** Maximum visible horizon in beats for looping constructs */
const LOOP_HORIZON_BEATS = 64;

export function parseCodeToTimeline(code: string, bufferId: number, sampleDurations?: Record<string, number>): TimelineData {
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

  /** Create a clip with all required fields including new ones with defaults */
  function makeClip(base: Omit<TimelineClip, 'audioTailBeats' | 'needsPreload' | 'sampleDurationMap'>): TimelineClip {
    return { ...base, audioTailBeats: 0, needsPreload: false, sampleDurationMap: {} };
  }

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

  // ── Event emitter: walk lines and emit individual sound-event clips ──

  interface EmittedEvent {
    name: string;
    beatOffset: number;       // offset within the block
    durationBeats: number;    // how long the event sounds
    code: string;             // the source line
    type: 'sample' | 'synth';
    amp: number;
    effects: ClipEffect[];
    samples: string[];
    srcLine: number;          // 0-based line index in rawLines
  }

  /**
   * Walk a block of lines and emit individual sound events with their beat offsets.
   * This is the core of the event-level decomposition.
   * `baseLineIdx` is the 0-based index of lines[0] in rawLines.
   * `inheritedEffects` are effects from enclosing with_fx blocks.
   */
  function emitBlockEvents(
    lines: string[],
    baseLineIdx: number,
    inheritedEffects: ClipEffect[],
  ): { events: EmittedEvent[]; totalDuration: number } {
    const events: EmittedEvent[] = [];
    let cursor = 0;
    let idx = 0;

    while (idx < lines.length) {
      const line = lines[idx];
      const trimmed = line.trim();
      const lineIdx = baseLineIdx + idx;

      // Skip blanks, comments, pragmas, variable assignments
      if (!trimmed || trimmed.startsWith('#')
          || /^use_bpm\b/.test(trimmed) || /^use_synth\b/.test(trimmed)
          || /^use_random_seed\b/.test(trimmed) || /^use_synth_defaults\b/.test(trimmed)
          || /^use_sample_defaults\b/.test(trimmed) || /^cue\b/.test(trimmed)
          || /^set\b/.test(trimmed) || /^get\b/.test(trimmed)
          || /^control\b/.test(trimmed) || /^stop\b/.test(trimmed)
          || /^tick\b/.test(trimmed) || /^look\b/.test(trimmed)
          || /^puts\b/.test(trimmed) || /^print\b/.test(trimmed)
          || (/^\w+\s*=\s*/.test(trimmed) && !/^(play|sample|sleep)/.test(trimmed))) {
        idx++;
        continue;
      }

      // ── sleep ──
      const sv = parseSleepValue(trimmed);
      if (sv !== null) { cursor += sv; idx++; continue; }

      // ── sample ──
      if (/^\bsample\b/.test(trimmed) && !/sample_path|sample_rate/.test(trimmed)) {
        // Handle trailing `if one_in(N)` — still show on timeline (optimistic)
        const name = sampleDisplayName(trimmed);
        const amp = parseAmp(trimmed);
        const dur = getSampleDurationBeats(trimmed, bpm, sampleDurations);
        events.push({
          name,
          beatOffset: cursor,
          durationBeats: Math.max(dur, 0.1),
          code: line,
          type: 'sample',
          amp,
          effects: [...inheritedEffects],
          samples: [name],
          srcLine: lineIdx,
        });
        idx++;
        continue;
      }

      // ── play (note or chord) ──
      if (/^\bplay\b/.test(trimmed) && !/play_pattern/.test(trimmed)) {
        const dur = playDurationBeats(trimmed);
        const amp = parseAmp(trimmed);
        let noteName = 'note';
        const noteMatch = trimmed.match(/play\s+:(\w+)/);
        if (noteMatch) noteName = noteMatch[1];
        const chordMatch = trimmed.match(/play\s+chord\(\s*:(\w+)/);
        if (chordMatch) noteName = chordMatch[1] + ' chord';
        events.push({
          name: noteName,
          beatOffset: cursor,
          durationBeats: Math.max(dur, 0.1),
          code: line,
          type: 'synth',
          amp,
          effects: [...inheritedEffects],
          samples: [],
          srcLine: lineIdx,
        });
        idx++;
        continue;
      }

      // ── play_pattern_timed ──
      if (/^\bplay_pattern_timed\b/.test(trimmed)) {
        const dur = patternTimedDuration(trimmed);
        const amp = parseAmp(trimmed);
        events.push({
          name: 'pattern',
          beatOffset: cursor,
          durationBeats: Math.max(dur, 0.25),
          code: line,
          type: 'synth',
          amp,
          effects: [...inheritedEffects],
          samples: [],
          srcLine: lineIdx,
        });
        cursor += dur;
        idx++;
        continue;
      }

      // ── N.times do ... end — unroll inner events N times ──
      const timesM = trimmed.match(/^(\d+)\.times\s+do/);
      if (timesM) {
        const count = parseInt(timesM[1]);
        const innerLines: string[] = [];
        const innerStart = idx + 1;
        let depth = 1;
        idx++;
        while (idx < lines.length && depth > 0) {
          const lt = lines[idx].trim();
          if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
          if (lt === 'end') { depth--; if (depth === 0) { idx++; break; } }
          innerLines.push(lines[idx]);
          idx++;
        }
        // Emit events for each repetition
        const innerResult = emitBlockEvents(innerLines, baseLineIdx + innerStart, inheritedEffects);
        const iterDur = innerResult.totalDuration;
        for (let rep = 0; rep < count; rep++) {
          for (const ev of innerResult.events) {
            events.push({
              ...ev,
              beatOffset: cursor + rep * iterDur + ev.beatOffset,
            });
          }
        }
        cursor += iterDur * count;
        continue;
      }

      // ── with_fx :name do ... end — pass effects to inner events ──
      const fxMatch = trimmed.match(/^with_fx\s+:(\w+)/);
      if (fxMatch) {
        const fxName = fxMatch[1];
        const fxParams: Record<string, number> = {};
        const paramStr = trimmed.match(/with_fx\s+:\w+(?:,\s*(.*))?/);
        if (paramStr && paramStr[1]) {
          for (const pm of paramStr[1].matchAll(/(\w+):\s*([\d.]+)/g)) {
            fxParams[pm[1]] = parseFloat(pm[2]);
          }
        }
        const newEffect: ClipEffect = { type: fxName, params: fxParams };

        const innerLines: string[] = [];
        const innerStart = idx + 1;
        let depth = 1;
        idx++;
        while (idx < lines.length && depth > 0) {
          const lt = lines[idx].trim();
          if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
          if (lt === 'end') { depth--; if (depth === 0) { idx++; break; } }
          innerLines.push(lines[idx]);
          idx++;
        }
        const innerResult = emitBlockEvents(innerLines, baseLineIdx + innerStart, [...inheritedEffects, newEffect]);
        for (const ev of innerResult.events) {
          events.push({ ...ev, beatOffset: cursor + ev.beatOffset });
        }
        cursor += innerResult.totalDuration;
        continue;
      }

      // ── with_synth / with_bpm blocks — recurse ──
      if (/^with_synth\s+/.test(trimmed) || /^with_bpm\s+/.test(trimmed)) {
        const innerLines: string[] = [];
        const innerStart = idx + 1;
        let depth = 1;
        idx++;
        while (idx < lines.length && depth > 0) {
          const lt = lines[idx].trim();
          if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
          if (lt === 'end') { depth--; if (depth === 0) { idx++; break; } }
          innerLines.push(lines[idx]);
          idx++;
        }
        const innerResult = emitBlockEvents(innerLines, baseLineIdx + innerStart, inheritedEffects);
        for (const ev of innerResult.events) {
          events.push({ ...ev, beatOffset: cursor + ev.beatOffset });
        }
        cursor += innerResult.totalDuration;
        continue;
      }

      // ── if ... do ... end — optimistic (assume condition true) ──
      if (/^if\s+/.test(trimmed) && (/\bdo\s*$/.test(trimmed) || /\bthen\s*$/.test(trimmed))) {
        const innerLines: string[] = [];
        const innerStart = idx + 1;
        let depth = 1;
        idx++;
        while (idx < lines.length && depth > 0) {
          const lt = lines[idx].trim();
          const isElsifElse = (lt.startsWith('elsif') || lt === 'else') && depth === 1;
          if (!isElsifElse && (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt) || /\bthen\s*$/.test(lt))) depth++;
          if (lt === 'end') depth--;
          innerLines.push(lines[idx]);
          idx++;
        }
        const filteredInner = innerLines.slice(0, -1)
          .filter(l => !l.trim().startsWith('elsif') && l.trim() !== 'else');
        const innerResult = emitBlockEvents(filteredInner, baseLineIdx + innerStart, inheritedEffects);
        for (const ev of innerResult.events) {
          events.push({ ...ev, beatOffset: cursor + ev.beatOffset });
        }
        cursor += innerResult.totalDuration;
        continue;
      }

      // ── Function call (from define) ──
      const funcCallMatch = trimmed.match(/^(\w+)\s*$/);
      if (funcCallMatch && definedFunctions.has(funcCallMatch[1])) {
        const funcName = funcCallMatch[1];
        const funcLines = definedFunctions.get(funcName)!;
        const innerResult = emitBlockEvents(funcLines, lineIdx, inheritedEffects);
        for (const ev of innerResult.events) {
          events.push({ ...ev, beatOffset: cursor + ev.beatOffset, srcLine: lineIdx });
        }
        cursor += innerResult.totalDuration;
        idx++;
        continue;
      }

      // ── Any other do...end block — recurse ──
      if (/\bdo\s*$/.test(trimmed) || /\bdo\s*\|/.test(trimmed) || /\bthen\s*$/.test(trimmed)) {
        const innerLines: string[] = [];
        const innerStart = idx + 1;
        let depth = 1;
        idx++;
        while (idx < lines.length && depth > 0) {
          const lt = lines[idx].trim();
          if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
          if (lt === 'end') { depth--; if (depth === 0) { idx++; break; } }
          innerLines.push(lines[idx]);
          idx++;
        }
        const innerResult = emitBlockEvents(innerLines, baseLineIdx + innerStart, inheritedEffects);
        for (const ev of innerResult.events) {
          events.push({ ...ev, beatOffset: cursor + ev.beatOffset });
        }
        cursor += innerResult.totalDuration;
        continue;
      }

      idx++;
    }

    return { events, totalDuration: cursor };
  }

  /** Convert emitted events into clips on a track, with optional looping repetition */
  function addEventsToTrack(
    trackName: string,
    events: EmittedEvent[],
    blockStart: number,
    looping: boolean,
    loopDuration: number,
  ) {
    const track = getOrCreateTrack(trackName, currentSection);
    if (looping && loopDuration > 0) {
      // Repeat events across the visible horizon
      const maxBeat = Math.max(LOOP_HORIZON_BEATS, blockStart + 64);
      let iter = 0;
      let t = blockStart;
      while (t < maxBeat && iter < 500) {
        for (const ev of events) {
          const clipStart = t + ev.beatOffset;
          if (clipStart >= maxBeat) break;
          track.clips.push(makeClip({
            id: nextClipId(),
            name: ev.name,
            startBeat: clipStart,
            durationBeats: Math.min(ev.durationBeats, maxBeat - clipStart),
            code: ev.code,
            type: ev.type,
            color: track.color,
            amp: ev.amp,
            effects: ev.effects,
            isLooping: false,      // individual events don't loop
            loopCount: 1,
            samples: ev.samples,
            srcLineStart: ev.srcLine,
            srcLineEnd: ev.srcLine,
            bufferId,
          }));
        }
        t += loopDuration;
        iter++;
      }
    } else {
      // One-shot: emit events once
      for (const ev of events) {
        track.clips.push(makeClip({
          id: nextClipId(),
          name: ev.name,
          startBeat: blockStart + ev.beatOffset,
          durationBeats: ev.durationBeats,
          code: ev.code,
          type: ev.type,
          color: track.color,
          amp: ev.amp,
          effects: ev.effects,
          isLooping: false,
          loopCount: 1,
          samples: ev.samples,
          srcLineStart: ev.srcLine,
          srcLineEnd: ev.srcLine,
          bufferId,
        }));
      }
    }
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

    // ── Variable assignments ──
    if (/^\w+\s*=\s*/.test(trimmed) && !/^(play|sample|sleep|use_|live_|with_|in_thread|define|def|if|loop)/.test(trimmed)) {
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

    // ── Ruby-style def name(args) ... end ──
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

    // ── Top-level sleep ──
    const sv = parseSleepValue(trimmed);
    if (sv !== null) {
      globalCursor += sv;
      i++; continue;
    }

    // ── live_loop :name do ... end ──
    const llMatch = trimmed.match(/live_loop\s+:(\w+)\s+do/);
    if (llMatch) {
      const loopName = llMatch[1];
      const blockStart = i;
      const blockLines: string[] = [];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') { depth--; if (depth === 0) { i++; break; } }
        blockLines.push(rawLines[i]);
        i++;
      }
      const hasStop = blockLines.some(l => l.trim() === 'stop');
      const innerLines = blockLines.filter(l => l.trim() !== 'stop');
      const displayName = loopName.replace(/_/g, ' ');
      const result = emitBlockEvents(innerLines, blockStart + 1, []);

      addEventsToTrack(
        displayName,
        result.events,
        globalCursor,
        !hasStop,                       // looping unless has stop
        result.totalDuration,           // loop period = one iteration
      );
      // live_loop is parallel — does NOT advance globalCursor
      continue;
    }

    // ── loop do ... end ──
    if (/^loop\s+do\s*$/.test(trimmed)) {
      const blockStart = i;
      const blockLines: string[] = [];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') { depth--; if (depth === 0) { i++; break; } }
        blockLines.push(rawLines[i]);
        i++;
      }
      const result = emitBlockEvents(blockLines, blockStart + 1, []);
      addEventsToTrack(
        'Loop',
        result.events,
        globalCursor,
        true,
        result.totalDuration,
      );
      // loop is parallel — does NOT advance globalCursor
      continue;
    }

    // ── in_thread do ... end ──
    if (/^in_thread\s+do/.test(trimmed)) {
      const blockStart = i;
      const blockLines: string[] = [];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') { depth--; if (depth === 0) { i++; break; } }
        blockLines.push(rawLines[i]);
        i++;
      }
      const result = emitBlockEvents(blockLines, blockStart + 1, []);
      addEventsToTrack(
        'Thread',
        result.events,
        globalCursor,
        false,
        result.totalDuration,
      );
      // in_thread is parallel — does NOT advance globalCursor
      continue;
    }

    // ── N.times do ... end (top-level) ──
    const timesMatch = trimmed.match(/^(\d+)\.times\s+do/);
    if (timesMatch) {
      const count = parseInt(timesMatch[1]);
      const blockStart = i;
      const blockLines: string[] = [];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') { depth--; if (depth === 0) { i++; break; } }
        blockLines.push(rawLines[i]);
        i++;
      }
      const result = emitBlockEvents(blockLines, blockStart + 1, []);
      // Emit unrolled events — N.times is sequential, advances cursor
      const iterDur = result.totalDuration;
      for (let rep = 0; rep < count; rep++) {
        for (const ev of result.events) {
          const track = getOrCreateTrack('Loop', currentSection);
          track.clips.push(makeClip({
            id: nextClipId(),
            name: ev.name,
            startBeat: globalCursor + rep * iterDur + ev.beatOffset,
            durationBeats: ev.durationBeats,
            code: ev.code,
            type: ev.type,
            color: track.color,
            amp: ev.amp,
            effects: ev.effects,
            isLooping: false,
            loopCount: 1,
            samples: ev.samples,
            srcLineStart: ev.srcLine,
            srcLineEnd: ev.srcLine,
            bufferId,
          }));
        }
      }
      globalCursor += iterDur * count;
      continue;
    }

    // ── Top-level with_fx :name do ... end ──
    const fxTopMatch = trimmed.match(/^with_fx\s+:(\w+)/);
    if (fxTopMatch) {
      const blockStart = i;
      const blockLines: string[] = [];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') { depth--; if (depth === 0) { i++; break; } }
        blockLines.push(rawLines[i]);
        i++;
      }
      const fxParams: Record<string, number> = {};
      const paramStr = trimmed.match(/with_fx\s+:\w+(?:,\s*(.*))?/);
      if (paramStr && paramStr[1]) {
        for (const pm of paramStr[1].matchAll(/(\w+):\s*([\d.]+)/g)) {
          fxParams[pm[1]] = parseFloat(pm[2]);
        }
      }
      const fxEffect: ClipEffect = { type: fxTopMatch[1], params: fxParams };
      const result = emitBlockEvents(blockLines, blockStart + 1, [fxEffect]);
      const fxName = fxTopMatch[1];

      for (const ev of result.events) {
        const track = getOrCreateTrack(`FX: ${fxName}`, currentSection);
        track.clips.push(makeClip({
          id: nextClipId(),
          name: ev.name,
          startBeat: globalCursor + ev.beatOffset,
          durationBeats: ev.durationBeats,
          code: ev.code,
          type: ev.type,
          color: track.color,
          amp: ev.amp,
          effects: ev.effects,
          isLooping: false,
          loopCount: 1,
          samples: ev.samples,
          srcLineStart: ev.srcLine,
          srcLineEnd: ev.srcLine,
          bufferId,
        }));
      }
      globalCursor += result.totalDuration;
      continue;
    }

    // ── Top-level with_synth / with_bpm blocks ──
    if (/^with_synth\s+/.test(trimmed) || /^with_bpm\b/.test(trimmed) || /^with_bpm_mul\b/.test(trimmed)) {
      const blockStart = i;
      const blockLines: string[] = [];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') { depth--; if (depth === 0) { i++; break; } }
        blockLines.push(rawLines[i]);
        i++;
      }
      const result = emitBlockEvents(blockLines, blockStart + 1, []);
      const blockName = trimmed.match(/with_(?:synth|bpm)\s+:?(\w+)/)?.[1] || 'block';
      for (const ev of result.events) {
        const track = getOrCreateTrack(`Synth: ${blockName}`, currentSection);
        track.clips.push(makeClip({
          id: nextClipId(),
          name: ev.name,
          startBeat: globalCursor + ev.beatOffset,
          durationBeats: ev.durationBeats,
          code: ev.code,
          type: ev.type,
          color: track.color,
          amp: ev.amp,
          effects: ev.effects,
          isLooping: false,
          loopCount: 1,
          samples: ev.samples,
          srcLineStart: ev.srcLine,
          srcLineEnd: ev.srcLine,
          bufferId,
        }));
      }
      globalCursor += result.totalDuration;
      continue;
    }

    // ── at [...] do ... end — schedule code at specific beat times ──
    const atMatch = trimmed.match(/^at\s+\[([^\]]*)\]\s*do/);
    if (atMatch) {
      const blockStart = i;
      const blockLines: string[] = [];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') { depth--; if (depth === 0) { i++; break; } }
        blockLines.push(rawLines[i]);
        i++;
      }
      const beatTimes = atMatch[1].split(',').map(s => parseFloat(s.trim())).filter(n => !isNaN(n));
      const result = emitBlockEvents(blockLines, blockStart + 1, []);
      for (const beatTime of beatTimes) {
        for (const ev of result.events) {
          const track = getOrCreateTrack('Scheduled', currentSection);
          track.clips.push(makeClip({
            id: nextClipId(),
            name: ev.name,
            startBeat: globalCursor + beatTime + ev.beatOffset,
            durationBeats: ev.durationBeats,
            code: ev.code,
            type: ev.type,
            color: track.color,
            amp: ev.amp,
            effects: ev.effects,
            isLooping: false,
            loopCount: 1,
            samples: ev.samples,
            srcLineStart: ev.srcLine,
            srcLineEnd: ev.srcLine,
            bufferId,
          }));
        }
      }
      continue;
    }

    // ── if / unless blocks (top-level) ──
    if ((/^if\s+/.test(trimmed) || /^unless\s+/.test(trimmed))
        && (/\bdo\s*$/.test(trimmed) || /\bthen\s*$/.test(trimmed))) {
      const blockStart = i;
      const blockLines: string[] = [line];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        const isElsifElse = (lt.startsWith('elsif') || lt === 'else') && depth === 1;
        if (!isElsifElse && (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt) || /\bthen\s*$/.test(lt))) depth++;
        if (lt === 'end') depth--;
        blockLines.push(rawLines[i]);
        i++;
      }
      const innerLines = blockLines.slice(1, -1)
        .filter(l => !l.trim().startsWith('elsif') && l.trim() !== 'else');
      const result = emitBlockEvents(innerLines, blockStart + 1, []);
      for (const ev of result.events) {
        const track = getOrCreateTrack('Conditional', currentSection);
        track.clips.push(makeClip({
          id: nextClipId(),
          name: ev.name,
          startBeat: globalCursor + ev.beatOffset,
          durationBeats: ev.durationBeats,
          code: ev.code,
          type: ev.type,
          color: track.color,
          amp: ev.amp,
          effects: ev.effects,
          isLooping: false,
          loopCount: 1,
          samples: ev.samples,
          srcLineStart: ev.srcLine,
          srcLineEnd: ev.srcLine,
          bufferId,
        }));
      }
      globalCursor += result.totalDuration;
      continue;
    }

    // ── .each do |x| ... end ──
    if (/\.each(_with_index)?\s+do/.test(trimmed)) {
      const blockStart = i;
      const blockLines: string[] = [];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') { depth--; if (depth === 0) { i++; break; } }
        blockLines.push(rawLines[i]);
        i++;
      }
      const result = emitBlockEvents(blockLines, blockStart + 1, []);
      for (const ev of result.events) {
        const track = getOrCreateTrack('Iteration', currentSection);
        track.clips.push(makeClip({
          id: nextClipId(),
          name: ev.name,
          startBeat: globalCursor + ev.beatOffset,
          durationBeats: ev.durationBeats,
          code: ev.code,
          type: ev.type,
          color: track.color,
          amp: ev.amp,
          effects: ev.effects,
          isLooping: false,
          loopCount: 1,
          samples: ev.samples,
          srcLineStart: ev.srcLine,
          srcLineEnd: ev.srcLine,
          bufferId,
        }));
      }
      globalCursor += result.totalDuration;
      continue;
    }

    // ── Function call (from define) ──
    const funcCallMatch = trimmed.match(/^(\w+)\s*$/);
    if (funcCallMatch && definedFunctions.has(funcCallMatch[1])) {
      const funcName = funcCallMatch[1];
      const funcLines = definedFunctions.get(funcName)!;
      const result = emitBlockEvents(funcLines, i, []);
      const displayName = funcName.replace(/_/g, ' ');
      for (const ev of result.events) {
        const track = getOrCreateTrack(displayName, currentSection);
        track.clips.push(makeClip({
          id: nextClipId(),
          name: ev.name,
          startBeat: globalCursor + ev.beatOffset,
          durationBeats: ev.durationBeats,
          code: ev.code,
          type: ev.type,
          color: track.color,
          amp: ev.amp,
          effects: ev.effects,
          isLooping: false,
          loopCount: 1,
          samples: ev.samples,
          srcLineStart: ev.srcLine,
          srcLineEnd: ev.srcLine,
          bufferId,
        }));
      }
      globalCursor += result.totalDuration;
      i++; continue;
    }

    // ── Standalone sample ──
    if (/^sample\b/.test(trimmed)) {
      const name = sampleDisplayName(trimmed);
      const amp = parseAmp(trimmed);
      const dur = getSampleDurationBeats(trimmed, bpm, sampleDurations);
      const track = getOrCreateTrack('Samples', currentSection);
      track.clips.push(makeClip({
        id: nextClipId(),
        name, startBeat: globalCursor,
        durationBeats: Math.max(dur, 0.1),
        code: line, type: 'sample', color: track.color,
        amp, effects: [], isLooping: false, loopCount: 1,
        samples: [name], srcLineStart: i, srcLineEnd: i, bufferId,
      }));
      // sample is non-blocking — does NOT advance cursor
      i++; continue;
    }

    // ── Standalone play / play chord ──
    if (/^play\b/.test(trimmed) && !/play_pattern/.test(trimmed)) {
      const dur = playDurationBeats(trimmed);
      const amp = parseAmp(trimmed);
      let noteName = 'note';
      const noteMatch = trimmed.match(/play\s+:(\w+)/);
      if (noteMatch) noteName = noteMatch[1];
      const chordMatch = trimmed.match(/play\s+chord\(\s*:(\w+)/);
      if (chordMatch) noteName = chordMatch[1] + ' chord';
      const track = getOrCreateTrack('Synth', currentSection);
      track.clips.push(makeClip({
        id: nextClipId(),
        name: noteName, startBeat: globalCursor,
        durationBeats: Math.max(dur, 0.1),
        code: line, type: 'synth', color: track.color,
        amp, effects: [], isLooping: false, loopCount: 1,
        samples: [], srcLineStart: i, srcLineEnd: i, bufferId,
      }));
      // play is non-blocking — does NOT advance cursor
      i++; continue;
    }

    // ── Standalone play_pattern_timed ──
    if (/^play_pattern_timed\b/.test(trimmed)) {
      const dur = patternTimedDuration(trimmed);
      const amp = parseAmp(trimmed);
      const track = getOrCreateTrack('Synth Pattern', currentSection);
      track.clips.push(makeClip({
        id: nextClipId(),
        name: 'pattern', startBeat: globalCursor,
        durationBeats: Math.max(dur, 0.25),
        code: line, type: 'synth', color: track.color,
        amp, effects: [], isLooping: false, loopCount: 1,
        samples: [], srcLineStart: i, srcLineEnd: i, bufferId,
      }));
      globalCursor += dur;
      i++; continue;
    }

    // ── Any other do/end block ──
    if (/\bdo\s*$/.test(trimmed) || /\bthen\s*$/.test(trimmed)) {
      const blockLines: string[] = [];
      let depth = 1;
      i++;
      while (i < rawLines.length && depth > 0) {
        const lt = rawLines[i].trim();
        if (/\bdo\s*$/.test(lt) || /\bdo\s*\|/.test(lt)) depth++;
        if (lt === 'end') { depth--; if (depth === 0) { i++; break; } }
        blockLines.push(rawLines[i]);
        i++;
      }
      continue;
    }

    i++;
  }

  // Calculate total beats from all clips
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

// ─── Exported utility: extract all sample names from code ────────

/** Extract all unique sample identifiers from Sonic Pi code.
 *  Returns both built-in names (e.g. "bd_haus") and file paths.
 *  Used by the frontend to query sample durations from the backend. */
export function extractCodeSampleNames(code: string): { builtins: string[]; filePaths: string[] } {
  const builtins = new Set<string>();
  const filePaths = new Set<string>();

  for (const raw of code.split('\n')) {
    const line = raw.trim();
    if (!/\bsample\b/.test(line) || /sample_path|sample_rate/.test(line)) continue;

    // Built-in symbol: sample :kick
    const sym = line.match(/sample\s+:(\w+)/);
    if (sym) { builtins.add(sym[1]); continue; }

    // String literal path: sample "path/to/file.wav"
    const str = line.match(/sample\s+["']([^"']+)["']/);
    if (str) { filePaths.add(str[1]); continue; }

    // Variable + string concat: sample sample_path + "file.wav"
    const concat = line.match(/sample\s+\w+\s*\+\s*["']([^"']+)["']/);
    if (concat) { filePaths.add(concat[1]); continue; }
  }

  return { builtins: [...builtins], filePaths: [...filePaths] };
}
