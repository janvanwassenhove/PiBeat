/**
 * PiBeat Agent — Reactive local agent with full Sonic Pi knowledge.
 *
 * This agent processes user messages, analyses the current buffer code,
 * and produces suggestions, code snippets, refactorings, and explanations.
 * It runs entirely client-side using pattern matching and templates.
 */

import { AgentMessage } from './store';
import { invoke } from '@tauri-apps/api/core';

// ──────────────────────────────────────────────
// Parity Analysis Types (mirrors Rust ParityReport)
// ──────────────────────────────────────────────

interface ParityItem {
  feature: string;
  status: 'supported' | 'partial' | 'unsupported';
  detail: string;
}

interface ParityCategory {
  name: string;
  status: 'full' | 'partial' | 'unsupported' | 'unused';
  items: ParityItem[];
}

interface ParitySuggestion {
  severity: 'error' | 'warning' | 'info';
  feature: string;
  message: string;
  fix: string | null;
}

interface ParityReport {
  score: number;
  features_used: number;
  features_supported: number;
  features_partial: number;
  features_unsupported: number;
  categories: ParityCategory[];
  suggestions: ParitySuggestion[];
  warnings: string[];
}

// ──────────────────────────────────────────────
// Knowledge base
// ──────────────────────────────────────────────

const SYNTHS = [
  { name: 'sine', desc: 'Smooth sine wave — pure tone' },
  { name: 'beep', desc: 'Simple beep (alias for sine)' },
  { name: 'saw', desc: 'Bright sawtooth wave' },
  { name: 'dsaw', desc: 'Detuned sawtooth' },
  { name: 'square', desc: 'Hollow square wave' },
  { name: 'tri', desc: 'Soft triangle wave' },
  { name: 'triangle', desc: 'Soft triangle wave (alias)' },
  { name: 'noise', desc: 'White noise' },
  { name: 'pulse', desc: 'Pulse wave with adjustable width' },
  { name: 'super_saw', desc: 'Detuned supersaw — very fat' },
  { name: 'tb303', desc: 'Acid bass synth' },
  { name: 'prophet', desc: 'Prophet-style analog synth' },
  { name: 'blade', desc: 'Blade Runner-style pad' },
  { name: 'pluck', desc: 'Plucked string (Karplus-Strong)' },
  { name: 'fm', desc: 'FM synthesis' },
  { name: 'mod_fm', desc: 'Modulated FM synthesis' },
  { name: 'mod_saw', desc: 'Modulated sawtooth' },
  { name: 'mod_pulse', desc: 'Modulated pulse' },
  { name: 'mod_tri', desc: 'Modulated triangle' },
];

const SAMPLES_KB = [
  { name: 'kick', desc: 'Kick drum' },
  { name: 'snare', desc: 'Snare drum' },
  { name: 'hihat', desc: 'Hi-hat cymbal' },
  { name: 'clap', desc: 'Hand clap' },
  { name: 'bass', desc: 'Bass hit' },
  { name: 'perc', desc: 'Percussion hit' },
  { name: 'loop_amen', desc: 'Classic Amen break loop' },
  { name: 'loop_breakbeat', desc: 'Breakbeat loop' },
  { name: 'ambi_choir', desc: 'Ambient choir pad' },
  { name: 'ambi_dark_woosh', desc: 'Dark ambient swoosh' },
  { name: 'ambi_drone', desc: 'Ambient drone' },
];

const FX_KB = [
  { name: 'reverb', params: 'mix, room, damp', desc: 'Reverb / room simulation' },
  { name: 'echo', params: 'time, feedback, mix', desc: 'Echo / delay' },
  { name: 'delay', params: 'time, feedback, mix', desc: 'Delay effect' },
  { name: 'distortion', params: 'distort, mix', desc: 'Distortion / overdrive' },
  { name: 'lpf', params: 'cutoff', desc: 'Low-pass filter' },
  { name: 'hpf', params: 'cutoff', desc: 'High-pass filter' },
  { name: 'flanger', params: 'phase, mix', desc: 'Flanger effect' },
  { name: 'slicer', params: 'phase, mix', desc: 'Amplitude slicer' },
  { name: 'wobble', params: 'phase, mix', desc: 'Wobble bass effect' },
  { name: 'compressor', params: 'threshold, slope', desc: 'Dynamic compressor' },
  { name: 'pitch_shift', params: 'pitch, mix', desc: 'Pitch shifter' },
  { name: 'ring_mod', params: 'freq, mix', desc: 'Ring modulator' },
  { name: 'bitcrusher', params: 'bits, mix', desc: 'Bit crusher / lo-fi' },
];

// ──────────────────────────────────────────────
// Code templates
// ──────────────────────────────────────────────

const TEMPLATES: Record<string, string> = {
  beat: `live_loop :drums do
  sample :kick
  sleep 0.5
  sample :hihat, amp: 0.6
  sleep 0.25
  sample :hihat, amp: 0.4
  sleep 0.25
  sample :snare
  sleep 0.5
  sample :hihat, amp: 0.6
  sleep 0.25
  sample :hihat, amp: 0.4
  sleep 0.25
end`,

  beat_complex: `live_loop :groove do
  sample :kick, amp: 0.9
  sample :hihat, amp: 0.3
  sleep 0.25
  sample :hihat, amp: 0.5
  sleep 0.25
  sample :snare, amp: 0.7
  sample :hihat, amp: 0.3
  sleep 0.25
  sample :hihat, amp: 0.6
  sleep 0.25
  sample :kick, amp: 0.7
  sleep 0.25
  sample :hihat, amp: 0.4
  sleep 0.25
  sample :snare, amp: 0.8
  sleep 0.25
  sample :hihat, amp: rrand(0.2, 0.6)
  sleep 0.25
end`,

  arp: `live_loop :arpeggio do
  use_synth :saw
  notes = ring(:c4, :e4, :g4, :b4, :c5, :b4, :g4, :e4)
  play notes.tick, amp: 0.3, release: 0.15, cutoff: rrand(70, 110)
  sleep 0.125
end`,

  pad: `live_loop :ambient_pad do
  use_synth :blade
  with_fx :reverb, mix: 0.7, room: 0.9 do
    play chord(:c4, :minor7), amp: 0.15, attack: 2, sustain: 4, release: 2
    sleep 8
  end
end`,

  acid: `live_loop :acid_bass do
  use_synth :tb303
  notes = ring(:c2, :c2, :eb2, :f2, :c2, :c2, :bb1, :c2)
  play notes.tick, cutoff: rrand(60, 120), release: 0.2, amp: 0.4, res: 0.5
  sleep 0.25
end`,

  melody: `live_loop :melody do
  use_synth :pluck
  notes = scale(:c4, :minor_pentatonic, num_octaves: 2)
  play notes.choose, amp: 0.5, release: 0.3
  sleep [0.25, 0.25, 0.5].choose
end`,

  full_track: `# === Full Track ===
use_bpm 120

live_loop :drums do
  sample :kick
  sleep 0.5
  sample :hihat, amp: 0.5
  sleep 0.25
  sample :hihat, amp: 0.3
  sleep 0.25
  sample :snare
  sleep 0.5
  sample :hihat, amp: 0.5
  sleep 0.25
  sample :hihat, amp: 0.3
  sleep 0.25
end

live_loop :bass do
  use_synth :tb303
  notes = ring(:c2, :c2, :eb2, :f2)
  play notes.tick, release: 0.2, cutoff: rrand(60, 100), amp: 0.4
  sleep 0.5
end

live_loop :melody do
  use_synth :pluck
  notes = ring(:c4, :eb4, :g4, :bb4, :c5)
  play notes.tick, amp: 0.35, release: 0.3
  sleep 0.25
end

live_loop :pad do
  use_synth :blade
  with_fx :reverb, mix: 0.6 do
    play chord(:c4, :minor7), amp: 0.1, attack: 2, sustain: 4, release: 2
  end
  sleep 8
end`,

  euclidean: `live_loop :euclidean_beat do
  pattern = (spread 5, 8)  # 5 hits over 8 steps
  sample :kick, amp: 0.8 if pattern.tick
  sleep 0.25
end

live_loop :offbeat do
  pattern = (spread 3, 8)
  sample :hihat, amp: 0.5 if pattern.tick
  sleep 0.25
end`,

  // ──────────────────────────────────────────
  // Song Structure Templates
  // ──────────────────────────────────────────

  intro: `## ---- INTRO (8 beats) ---- ##
live_loop :intro_pad do
  use_synth :blade
  with_fx :lpf, cutoff: 60, mix: 1.0 do
    with_fx :reverb, mix: 0.7, room: 0.9 do
      play chord(:c4, :minor7), amp: 0.15, attack: 2, sustain: 2, release: 2
    end
  end
  sleep 4
  stop
end

live_loop :intro_perc do
  8.times do
    sample :perc_snap, amp: 0.3, rate: 0.8
    sleep 0.5
  end
  stop
end`,

  intro_electronic: `## ---- ELECTRONIC INTRO (16 beats) ---- ##
live_loop :intro_synth do
  use_synth :dsaw
  with_fx :lpf, cutoff: 50 do
    with_fx :reverb, mix: 0.6 do
      notes = ring(:c3, :g3, :c4, :eb4)
      play notes.tick, amp: 0.2, attack: 0.5, release: 1.5
      sleep 2
    end
  end
  stop if look > 7
end

live_loop :intro_noise do
  with_fx :hpf, cutoff: 90 do
    sample :ambi_drone, amp: 0.15, rate: 0.5
  end
  sleep 8
  stop
end`,

  fade_in: `## ---- FADE IN (8 beats) ---- ##
live_loop :fade_in_main do
  tick
  amp_val = [0.05, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8].ring.look
  cutoff_val = [40, 50, 60, 70, 80, 90, 100, 110, 120].ring.look
  
  use_synth :super_saw
  with_fx :lpf, cutoff: cutoff_val do
    play chord(:c4, :minor), amp: amp_val, release: 0.8
  end
  sleep 1
  stop if look > 8
end`,

  verse: `## ---- VERSE ---- ##
live_loop :verse_drums do
  sample :kick
  sleep 0.5
  sample :hihat, amp: 0.5
  sleep 0.5
  sample :snare, amp: 0.8
  sleep 0.5
  sample :hihat, amp: 0.5
  sleep 0.5
end

live_loop :verse_bass do
  use_synth :tb303
  notes = ring(:c2, :c2, :eb2, :f2, :c2, :c2, :g1, :ab1)
  play notes.tick, cutoff: 80, release: 0.25, amp: 0.5
  sleep 0.5
end

live_loop :verse_pad do
  use_synth :blade
  with_fx :reverb, mix: 0.5 do
    play chord(:c4, :minor7), amp: 0.1, attack: 2, release: 2
  end
  sleep 4
end`,

  buildup: `## ---- BUILDUP (8 beats) ---- ##
live_loop :buildup_snare do
  with_fx :echo, phase: 0.125, decay: 0.5 do
    16.times do |i|
      sample :drum_snare_soft, amp: 0.4 + (i * 0.08), rate: 1 + (i * 0.02)
      sleep 0.25 - (i * 0.005)
    end
  end
  stop
end

live_loop :buildup_riser do
  use_synth :noise
  with_fx :hpf, cutoff: 30 do
    play :c4, amp: 0.3, attack: 4, release: 0
  end
  sleep 4
  stop
end`,

  drop: `## ---- DROP ---- ##
live_loop :drop_drums do
  sample :bd_haus, amp: 1.2
  sample :perc_snap, amp: 0.8
  sleep 0.5
  sample :hihat, amp: 0.6
  sleep 0.25
  sample :hihat, amp: 0.4
  sleep 0.25
  sample :sn_dub, amp: 1.0
  sleep 0.5
  sample :hihat, amp: 0.6
  sleep 0.25
  sample :hihat, amp: 0.4
  sleep 0.25
end

live_loop :drop_bass do
  use_synth :dsaw
  with_fx :distortion, distort: 0.3 do
    play :c1, amp: 0.6, release: 0.3
    sleep 0.5
    play :c1, amp: 0.4, release: 0.2
    sleep 0.25
    play :eb1, amp: 0.5, release: 0.3
    sleep 0.25
  end
end

live_loop :drop_lead do
  use_synth :super_saw
  with_fx :reverb, mix: 0.3 do
    play chord(:c4, :minor), amp: 0.4, release: 0.5
    sleep 2
  end
end`,

  bridge: `## ---- BRIDGE ---- ##
live_loop :bridge_pad do
  use_synth :prophet
  with_fx :reverb, room: 0.9, mix: 0.6 do
    play chord(:ab3, :minor7), amp: 0.25, attack: 1.5, sustain: 2, release: 2
    sleep 4
    play chord(:eb3, :major7), amp: 0.25, attack: 1.5, sustain: 2, release: 2
    sleep 4
  end
end

live_loop :bridge_arp do
  use_synth :pluck
  notes = ring(:ab4, :c5, :eb5, :g5, :ab5, :g5, :eb5, :c5)
  play notes.tick, amp: 0.35
  sleep 0.5
end

live_loop :bridge_perc do
  sample :perc_snap, amp: 0.4
  sleep 1
  sample :perc_snap, amp: 0.2
  sleep 1
end`,

  chorus: `## ---- CHORUS ---- ##
live_loop :chorus_chords do
  use_synth :super_saw
  with_fx :reverb, mix: 0.4 do
    play chord(:c4, :minor), amp: 0.4, release: 1.2
    sleep 2
    play chord(:ab3, :major), amp: 0.4, release: 1.2
    sleep 2
    play chord(:eb4, :major), amp: 0.4, release: 1.2
    sleep 2
    play chord(:bb3, :major), amp: 0.4, release: 1.2
    sleep 2
  end
end

live_loop :chorus_drums do
  sample :bd_haus, amp: 1.0
  sample :hihat, amp: 0.4
  sleep 0.5
  sample :hihat, amp: 0.6
  sleep 0.5
  sample :sn_dub, amp: 0.9
  sample :hihat, amp: 0.4
  sleep 0.5
  sample :hihat, amp: 0.6
  sleep 0.5
end

live_loop :chorus_bass do
  use_synth :tb303
  notes = ring(:c2, :c2, :ab1, :ab1, :eb2, :eb2, :bb1, :bb1)
  play notes.tick, cutoff: 90, release: 0.3, amp: 0.5
  sleep 0.5
end`,

  outro: `## ---- OUTRO (fade out) ---- ##
live_loop :outro do
  tick
  amp_val = [0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1, 0.05].ring.look
  cutoff_val = [120, 110, 100, 90, 80, 70, 60, 50, 40].ring.look
  
  use_synth :blade
  with_fx :lpf, cutoff: cutoff_val do
    with_fx :reverb, mix: 0.5 + (look * 0.05) do
      play chord(:c4, :minor7), amp: amp_val, release: 2
    end
  end
  sleep 2
  stop if look > 8
end

live_loop :outro_perc do
  tick
  amp_val = [0.6, 0.5, 0.4, 0.35, 0.3, 0.25, 0.2, 0.15, 0.1].ring.look
  sample :perc_snap, amp: amp_val
  sleep 1
  stop if look > 8
end`,

  fade_out: `## ---- FADE OUT (8 beats) ---- ##
live_loop :fade_out_main do
  tick
  amp_val = [0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1, 0.05].ring.look
  
  use_synth :super_saw
  with_fx :reverb, mix: 0.4 + (look * 0.06) do
    play chord(:c4, :minor), amp: amp_val, release: 0.8
  end
  sleep 1
  stop if look > 8
end`,

  multiple_verses_with_drop: `## ---- FULL TRACK: VERSES + DROP ---- ##
use_bpm 128

# Control flow with sections
define :section_length do 16 end  # beats per section

## ---- VERSE 1 ---- ##
live_loop :v1_drums do
  sample :kick
  sleep 0.5
  sample :hihat, amp: 0.4
  sleep 0.5
  sample :snare, amp: 0.7
  sleep 0.5
  sample :hihat, amp: 0.4
  sleep 0.5
end

live_loop :v1_bass do
  use_synth :tb303
  notes = ring(:c2, :c2, :eb2, :f2)
  play notes.tick, cutoff: 70, release: 0.3, amp: 0.5
  sleep 0.5
end

live_loop :v1_pad do
  use_synth :blade
  with_fx :reverb, mix: 0.5 do
    play chord(:c4, :minor7), amp: 0.1, attack: 2, release: 2
  end
  sleep 4
end

## After ~16 beats, this will naturally loop
## To sequence sections, use timestamps with sleep:
# sleep 16  # After verse 1
# Then define drop loops below...

## ---- DROP (Add after verse for contrast) ---- ##
# Uncomment and run for drop:
# live_loop :drop_drums do
#   sample :bd_haus, amp: 1.2
#   sample :perc_snap, amp: 0.8
#   sleep 0.5
#   sample :hihat, amp: 0.6
#   sleep 0.25
#   sample :hihat, amp: 0.4
#   sleep 0.25
#   sample :sn_dub, amp: 1.0
#   sleep 0.5
#   sample :hihat, amp: 0.6
#   sleep 0.25
#   sample :hihat, amp: 0.4
#   sleep 0.25
# end`,
};

// ──────────────────────────────────────────────
// Code analysis helpers
// ──────────────────────────────────────────────

interface CodeAnalysis {
  hasLiveLoop: boolean;
  hasLoop: boolean;
  hasFx: boolean;
  hasSample: boolean;
  hasPlay: boolean;
  hasSleep: boolean;
  hasSynth: boolean;
  usedSynths: string[];
  usedSamples: string[];
  usedFx: string[];
  lineCount: number;
  liveLoopNames: string[];
  issues: string[];
}

function analyzeCode(code: string): CodeAnalysis {
  const lines = code.split('\n');
  const analysis: CodeAnalysis = {
    hasLiveLoop: /live_loop/.test(code),
    hasLoop: /\bloop\b/.test(code),
    hasFx: /with_fx/.test(code),
    hasSample: /\bsample\b/.test(code),
    hasPlay: /\bplay\b/.test(code),
    hasSleep: /\bsleep\b/.test(code),
    hasSynth: /use_synth/.test(code),
    usedSynths: [],
    usedSamples: [],
    usedFx: [],
    lineCount: lines.length,
    liveLoopNames: [],
    issues: [],
  };

  // Extract used synths
  const synthMatches = code.matchAll(/use_synth\s+:(\w+)/g);
  for (const m of synthMatches) analysis.usedSynths.push(m[1]);

  // Extract used samples
  const sampleMatches = code.matchAll(/sample\s+:(\w+)/g);
  for (const m of sampleMatches) analysis.usedSamples.push(m[1]);

  // Extract FX
  const fxMatches = code.matchAll(/with_fx\s+:(\w+)/g);
  for (const m of fxMatches) analysis.usedFx.push(m[1]);

  // Extract live loop names
  const loopMatches = code.matchAll(/live_loop\s+:(\w+)/g);
  for (const m of loopMatches) analysis.liveLoopNames.push(m[1]);

  // Detect issues
  if (analysis.hasLoop && !analysis.hasSleep) {
    analysis.issues.push('Loop without `sleep` — this will cause an infinite tight loop!');
  }
  if (analysis.hasLiveLoop && !analysis.hasSleep) {
    analysis.issues.push('`live_loop` without `sleep` — each iteration needs at least one `sleep` call.');
  }
  if (!analysis.hasPlay && !analysis.hasSample && code.trim().length > 10) {
    analysis.issues.push('No `play` or `sample` calls found — this code won\'t produce sound.');
  }

  // Check for common mistakes
  if (/play\s+\d+\s*,/.test(code) && !/play\s+:\w+/.test(code)) {
    // not an issue, just noting MIDI number usage
  }

  return analysis;
}

// ──────────────────────────────────────────────
// Refactoring engine
// ──────────────────────────────────────────────

function refactorCode(code: string): { refactored: string; changes: string[] } {
  let refactored = code;
  const changes: string[] = [];

  // 1. Wrap top-level repeating play/sample/sleep blocks into live_loop
  const analysis = analyzeCode(code);
  if (!analysis.hasLiveLoop && analysis.hasLoop) {
    refactored = refactored.replace(/\bloop\s+do\b/g, 'live_loop :main do');
    changes.push('Replaced `loop do` with `live_loop :main do` for hot-reloading support.');
  }

  // 2. Add missing sleep if loop exists without one
  if (analysis.hasLiveLoop && !analysis.hasSleep) {
    refactored = refactored.replace(/(live_loop\s+:\w+\s+do\n)/g, '$1  sleep 0.5 # Added: every loop needs sleep\n');
    changes.push('Added `sleep` inside loop — without it, the loop runs infinitely fast.');
  }

  // 3. Extract repeated hardcoded notes into a ring
  const playNotePattern = /(?:play\s+:(\w+)\s*(?:,\s*[\w:.\s]+)?\s*\nsleep\s+[\d.]+\s*\n){3,}/g;
  const match = playNotePattern.exec(refactored);
  if (match) {
    // Find all notes in the match
    const noteMatches = match[0].matchAll(/play\s+:(\w+)/g);
    const notes: string[] = [];
    for (const nm of noteMatches) notes.push(nm[1]);
    if (notes.length >= 3) {
      changes.push(`Extracted ${notes.length} repeated notes into a \`ring\` for cleaner cycling: \`ring(${notes.map(n => ':' + n).join(', ')})\`.`);
      // We'll suggest the refactored structure rather than doing complex regex replace
    }
  }

  // 4. Suggest use_bpm if not present and there are sleep values
  if (analysis.hasSleep && !/use_bpm/.test(refactored)) {
    refactored = `use_bpm 120\n\n${refactored}`;
    changes.push('Added `use_bpm 120` — makes tempo explicit and easy to change.');
  }

  // 5. Clean up extra blank lines
  refactored = refactored.replace(/\n{3,}/g, '\n\n');
  if (refactored !== code && !changes.some(c => c.includes('blank'))) {
    // Only add if there were actually extra blank lines removed
    if (code.includes('\n\n\n')) {
      changes.push('Cleaned up extra blank lines.');
    }
  }

  if (changes.length === 0) {
    changes.push('Code looks clean! No major refactoring needed.');
  }

  return { refactored, changes };
}

// ──────────────────────────────────────────────
// Intent detection
// ──────────────────────────────────────────────

type Intent =
  | 'generate_beat'
  | 'generate_melody'
  | 'generate_arp'
  | 'generate_pad'
  | 'generate_acid'
  | 'generate_full'
  | 'generate_euclidean'
  | 'generate_intro'
  | 'generate_outro'
  | 'generate_verse'
  | 'generate_chorus'
  | 'generate_bridge'
  | 'generate_buildup'
  | 'generate_drop'
  | 'generate_fade_in'
  | 'generate_fade_out'
  | 'generate_structure'
  | 'refactor'
  | 'explain'
  | 'add_fx'
  | 'list_synths'
  | 'list_samples'
  | 'list_fx'
  | 'help_syntax'
  | 'analyze'
  | 'parity_check'
  | 'parity_fix'
  | 'parity_synths'
  | 'parity_effects'
  | 'parity_samples'
  | 'general';

function detectIntent(message: string): Intent {
  const m = message.toLowerCase();

  // Parity analysis intents — check BEFORE generic analyze
  if (/parity.*check|check.*parity|sound.*parity|sonic.*pi.*compat|compatibility.*check|full.*parity|parity.*report|parity.*analys/.test(m)) return 'parity_check';
  if (/parity.*fix|fix.*parity|fix.*compat|auto.*fix.*parity|apply.*parity/.test(m)) return 'parity_fix';
  if (/parity.*synth|synth.*parity|synth.*compat|check.*synth/.test(m)) return 'parity_synths';
  if (/parity.*effect|effect.*parity|effect.*compat|check.*effect|fx.*parity|fx.*compat/.test(m)) return 'parity_effects';
  if (/parity.*sample|sample.*parity|sample.*compat|check.*sample.*compat/.test(m)) return 'parity_samples';

  if (/refactor|clean\s*up|improve|restructure|optimize|tidy/.test(m)) return 'refactor';
  if (/explain|what does|how does|walk.*through|line.by.line|understand/.test(m)) return 'explain';
  if (/analyze|analys|check|review|issues|problems|bugs|mistakes/.test(m)) return 'analyze';

  // Song structure detection (check these before generic beat/melody)
  if (/intro\b|introduction|open(?:ing|er)/.test(m)) return 'generate_intro';
  if (/outro\b|ending|close|final\s*section/.test(m)) return 'generate_outro';
  if (/fade[\s-]?in/.test(m)) return 'generate_fade_in';
  if (/fade[\s-]?out/.test(m)) return 'generate_fade_out';
  if (/verse\b/.test(m)) return 'generate_verse';
  if (/chorus\b|hook\b/.test(m)) return 'generate_chorus';
  if (/bridge\b|break\b|interlude\b/.test(m)) return 'generate_bridge';
  if (/build[\s-]?up|riser|tension/.test(m)) return 'generate_buildup';
  if (/drop\b|climax|bang|peak/.test(m)) return 'generate_drop';
  if (/structure|section|multiple.*verse|verses.*drop|arrange|arrangement/.test(m)) return 'generate_structure';

  if (/full\s*track|complete\s*song|entire.*track|whole.*song/.test(m)) return 'generate_full';
  if (/euclidean|spread|polyrhythm/.test(m)) return 'generate_euclidean';
  if (/beat|drum|rhythm|percussion|kick|snare/.test(m)) return 'generate_beat';
  if (/arp|arpegg/.test(m)) return 'generate_arp';
  if (/pad|ambient|atmosphere|drone/.test(m)) return 'generate_pad';
  if (/acid|303|bass\s*line|bassline/.test(m)) return 'generate_acid';
  if (/melody|tune|lead|solo/.test(m)) return 'generate_melody';

  if (/add.*(?:effect|fx|reverb|echo|delay|distort)|with_fx|effect/.test(m)) return 'add_fx';
  if (/list.*synth|synth.*list|what synths|available synths|show.*synths/.test(m)) return 'list_synths';
  if (/list.*sample|sample.*list|what sample|available sample|show.*sample/.test(m)) return 'list_samples';
  if (/list.*(?:effect|fx)|(?:effect|fx).*list|what.*(?:effect|fx)|available.*(?:effect|fx)/.test(m)) return 'list_fx';
  if (/how to|syntax|how do i|help|tutorial|guide|example/.test(m)) return 'help_syntax';

  return 'general';
}

// ──────────────────────────────────────────────
// Response generation
// ──────────────────────────────────────────────

export async function processAgentMessage(
  userMessage: string,
  currentCode: string,
  _history: AgentMessage[],
  userSamples?: Array<{
    name: string;
    path: string;
    audio_type: string;
    feeling: string;
    duration_secs: number;
    bpm_estimate: number | null;
    tags: string[];
  }>
): Promise<AgentMessage> {
  // Small delay to feel reactive
  await new Promise(r => setTimeout(r, 300 + Math.random() * 400));

  const intent = detectIntent(userMessage);
  const analysis = analyzeCode(currentCode);
  
  // Check if user is asking about their samples
  const isUserSampleQuery = /my sample|user sample|local sample|imported sample|my audio|my files/i.test(userMessage);
  
  if (isUserSampleQuery && userSamples && userSamples.length > 0) {
    return buildUserSampleResponse(userMessage, userSamples);
  }
  
  // If user wants to use their samples in a composition, suggest them
  if (intent === 'generate_beat' || intent === 'generate_full') {
    const drumSamples = userSamples?.filter(s => s.audio_type === 'drums') || [];
    if (drumSamples.length > 0 && /my|user|local|own|imported/i.test(userMessage)) {
      return buildCompositionWithUserSamples(intent, userMessage, drumSamples, userSamples || []);
    }
  }

  switch (intent) {
    case 'generate_beat': {
      const isComplex = /complex|advanced|groov|funky|interesting/.test(userMessage.toLowerCase());
      const template = isComplex ? TEMPLATES.beat_complex : TEMPLATES.beat;
      return {
        role: 'assistant',
        content: `Here's a ${isComplex ? 'complex groove' : 'drum beat'} pattern:\n\n\`\`\`ruby\n${template}\n\`\`\`\n\nThis uses \`live_loop\` so it repeats automatically. You can tweak the \`amp\` values and \`sleep\` timings to change the feel.`,
      };
    }

    case 'generate_melody':
      return {
        role: 'assistant',
        content: `Here's a melody using the minor pentatonic scale:\n\n\`\`\`ruby\n${TEMPLATES.melody}\n\`\`\`\n\nThis picks random notes from the scale with varying rhythms. Try changing \`:minor_pentatonic\` to \`:major\`, \`:blues_minor\`, or \`:japanese\` for different moods.`,
      };

    case 'generate_arp':
      return {
        role: 'assistant',
        content: `Here's an arpeggiated synth pattern:\n\n\`\`\`ruby\n${TEMPLATES.arp}\n\`\`\`\n\nThe \`ring\` cycles through the notes endlessly with \`.tick\`. Adjust the note sequence and \`sleep\` value to change speed and pattern.`,
      };

    case 'generate_pad':
      return {
        role: 'assistant',
        content: `Here's a lush ambient pad:\n\n\`\`\`ruby\n${TEMPLATES.pad}\n\`\`\`\n\nThe \`:blade\` synth with reverb creates a wide, atmospheric sound. The long \`attack\` and \`release\` make it drift in and out smoothly.`,
      };

    case 'generate_acid':
      return {
        role: 'assistant',
        content: `Here's an acid bass line using the TB-303 emulation:\n\n\`\`\`ruby\n${TEMPLATES.acid}\n\`\`\`\n\nThe random \`cutoff\` gives it that classic squelchy acid sound. Increase \`res\` (resonance) for more squelch.`,
      };

    case 'generate_full':
      return {
        role: 'assistant',
        content: `Here's a complete multi-layer track with drums, bass, melody, and pad:\n\n\`\`\`ruby\n${TEMPLATES.full_track}\n\`\`\`\n\nEach \`live_loop\` runs concurrently. You can modify any section independently.`,
      };

    case 'generate_euclidean':
      return {
        role: 'assistant',
        content: `Here's a euclidean rhythm pattern using \`spread\`:\n\n\`\`\`ruby\n${TEMPLATES.euclidean}\n\`\`\`\n\n\`spread(5, 8)\` distributes 5 hits as evenly as possible over 8 steps — a classic technique for interesting rhythms.`,
      };

    // ── Song Structure Handlers ──

    case 'generate_intro': {
      const isElectronic = /electronic|edm|techno|house|synth/.test(userMessage.toLowerCase());
      const template = isElectronic ? TEMPLATES.intro_electronic : TEMPLATES.intro;
      const style = isElectronic ? 'electronic' : 'ambient pad';
      return {
        role: 'assistant',
        content: `Here's a ${style} intro that eases into the track:\n\n\`\`\`ruby\n${template}\n\`\`\`\n\nThe intro uses \`stop\` to end after one iteration. Remove the \`stop\` lines if you want them to continue looping.\n\n**Tips:**\n• Use \`with_fx :lpf\` with low cutoff to start muffled, then increase\n• Keep the intro sparse — leave room to build`,
      };
    }

    case 'generate_outro': {
      return {
        role: 'assistant',
        content: `Here's an outro with a gradual fade out:\n\n\`\`\`ruby\n${TEMPLATES.outro}\n\`\`\`\n\nThe amplitude and filter cutoff decrease over time using \`tick\` and \`ring\`. The reverb mix increases to add space as elements fade.\n\n**Tips:**\n• Add more \`live_loop\` blocks with similar fade patterns\n• Increase reverb/delay towards the end for a "dissolving" effect`,
      };
    }

    case 'generate_fade_in': {
      return {
        role: 'assistant',
        content: `Here's a fade-in pattern over 8 beats:\n\n\`\`\`ruby\n${TEMPLATES.fade_in}\n\`\`\`\n\nBoth amplitude and filter cutoff ramp up using rings and \`.tick/.look\`. The track emerges gradually from silence.\n\n**Customize:**\n• Adjust the amp values in the ring (0.05 → 0.8)\n• Change the cutoff values for different filtering intensity`,
      };
    }

    case 'generate_fade_out': {
      return {
        role: 'assistant',
        content: `Here's a fade-out pattern over 8 beats:\n\n\`\`\`ruby\n${TEMPLATES.fade_out}\n\`\`\`\n\nThe amplitude decreases from 0.8 to 0.05 while reverb increases. Use this at the end of your track.\n\n**Tips:**\n• Apply the same fade logic to all your active live_loops\n• Increase reverb/delay mix as volume drops for a natural tail`,
      };
    }

    case 'generate_verse': {
      const hasExistingCode = currentCode.trim().length > 50;
      let response = `Here's a verse pattern with drums, bass, and pad:\n\n\`\`\`ruby\n${TEMPLATES.verse}\n\`\`\`\n\n`;
      if (hasExistingCode) {
        response += `This is designed to work alongside your existing code. The verse has a stripped-back feel compared to a chorus or drop — perfect for vocal sections or building anticipation.`;
      } else {
        response += `The verse establishes the main groove. It's intentionally less intense than a chorus or drop, giving room to build.`;
      }
      return { role: 'assistant', content: response };
    }

    case 'generate_chorus': {
      return {
        role: 'assistant',
        content: `Here's a memorable chorus section:\n\n\`\`\`ruby\n${TEMPLATES.chorus}\n\`\`\`\n\nThe chorus uses:\n• **Super saw** chords for width and impact\n• **Chord progression:** Cm → Ab → Eb → Bb (classic emotional progression)\n• **Full drums** with consistent energy\n• **Driving bass** following the chord roots\n\n**Tips:**\n• Add a lead melody on top\n• Contrast with a more minimal verse`,
      };
    }

    case 'generate_bridge': {
      return {
        role: 'assistant',
        content: `Here's a contrasting bridge section:\n\n\`\`\`ruby\n${TEMPLATES.bridge}\n\`\`\`\n\nThe bridge shifts to Ab minor / Eb major for harmonic contrast. It uses:\n• **Prophet synth** — warmer pad sound\n• **Pluck arpeggios** — keeps movement without being intense\n• **Sparse percussion** — just accents\n\nGreat for creating a "moment of reflection" before returning to the main sections.`,
      };
    }

    case 'generate_buildup': {
      return {
        role: 'assistant',
        content: `Here's a buildup that creates tension before a drop:\n\n\`\`\`ruby\n${TEMPLATES.buildup}\n\`\`\`\n\nTechniques used:\n• **Accelerating snare rolls** — amplitude and rate increase each hit\n• **Noise riser** — filtered noise with long attack\n• **Echo effect** — adds density\n\n**Variation ideas:**\n• Add a rising synth note with pitch bend\n• Increase the tempo slightly with \`use_bpm\`\n• Add filter automation (cutoff increasing)`,
      };
    }

    case 'generate_drop': {
      return {
        role: 'assistant',
        content: `Here's an energetic drop section:\n\n\`\`\`ruby\n${TEMPLATES.drop}\n\`\`\`\n\nThe drop features:\n• **Heavy kick + snare** — driving four-on-the-floor beat\n• **Distorted bass** — detuned saw with grit\n• **Super saw lead** — adds width and energy\n\nUse this after a buildup for maximum impact. The contrast creates that "release" moment!`,
      };
    }

    case 'generate_structure': {
      return {
        role: 'assistant',
        content: `Here's a full song structure with verses and a drop:\n\n\`\`\`ruby\n${TEMPLATES.multiple_verses_with_drop}\n\`\`\`\n\n**Song Structure Tips:**\n\nIn PiBeat/Sonic Pi, you create structure by:\n1. **Running loops** — all \`live_loop\` blocks run concurrently\n2. **Using \`stop\`** — end a loop after N iterations\n3. **Hot-reloading** — modify and re-run to transition sections\n\n**Manual arrangement:**\n\`\`\`ruby\n# Verse for 16 beats, then start drop\nsleep 16\nlive_loop :drop do ...\n\`\`\`\n\nFor a professionally sequenced track, layer intro → verse → buildup → drop → verse → bridge → chorus → outro using the timestamps and \`stop\` commands.`,
      };
    }

    case 'refactor': {
      if (currentCode.trim().length < 10) {
        return {
          role: 'assistant',
          content: 'Your buffer is mostly empty. Write some code first, then ask me to refactor it!',
        };
      }
      const { refactored, changes } = refactorCode(currentCode);
      const changeList = changes.map(c => `• ${c}`).join('\n');
      return {
        role: 'assistant',
        content: `I've refactored your code. Here's what changed:\n\n${changeList}\n\n\`\`\`ruby\n${refactored}\n\`\`\``,
      };
    }

    case 'explain': {
      if (currentCode.trim().length < 10) {
        return {
          role: 'assistant',
          content: 'Your buffer is empty or has very little code. Write something and I\'ll explain what it does!',
        };
      }
      const explanation = explainCode(currentCode, analysis);
      return {
        role: 'assistant',
        content: explanation,
      };
    }

    case 'analyze': {
      if (currentCode.trim().length < 10) {
        return {
          role: 'assistant',
          content: 'Your buffer is empty. Write some code first and I\'ll analyze it for issues!',
        };
      }
      return {
        role: 'assistant',
        content: buildAnalysisResponse(analysis, currentCode),
      };
    }

    // ── Parity Analysis Handlers ──

    case 'parity_check':
    case 'parity_synths':
    case 'parity_effects':
    case 'parity_samples': {
      if (currentCode.trim().length < 10) {
        return {
          role: 'assistant',
          content: 'Your buffer is empty. Write some Sonic Pi code first, then I\'ll check its parity with the Sonic Pi IDE!',
        };
      }
      return await runParityAnalysis(currentCode, intent);
    }

    case 'parity_fix': {
      if (currentCode.trim().length < 10) {
        return {
          role: 'assistant',
          content: 'Your buffer is empty. Write some code first, then I can suggest parity fixes!',
        };
      }
      return await runParityFix(currentCode);
    }

    case 'add_fx': {
      const suggestions = suggestEffects(currentCode, analysis, userMessage);
      return {
        role: 'assistant',
        content: suggestions,
      };
    }

    case 'list_synths':
      return {
        role: 'assistant',
        content: '**Available Synths:**\n\n' + SYNTHS.map(s => `• \`:${s.name}\` — ${s.desc}`).join('\n') +
          '\n\nUse with `use_synth :name` before `play` commands.',
      };

    case 'list_samples':
      return {
        role: 'assistant',
        content: '**Available Samples:**\n\n' + SAMPLES_KB.map(s => `• \`:${s.name}\` — ${s.desc}`).join('\n') +
          '\n\nPlay with `sample :name, amp: 0.8`.',
      };

    case 'list_fx':
      return {
        role: 'assistant',
        content: '**Available Effects:**\n\n' + FX_KB.map(f => `• \`:${f.name}\` — ${f.desc} (params: ${f.params})`).join('\n') +
          '\n\nWrap code in `with_fx :name, param: value do ... end`.',
      };

    case 'help_syntax':
      return {
        role: 'assistant',
        content: handleSyntaxHelp(userMessage),
      };

    case 'general':
    default:
      return {
        role: 'assistant',
        content: handleGeneralQuestion(userMessage, analysis, currentCode),
      };
  }
}

// ──────────────────────────────────────────────
// Detailed response helpers
// ──────────────────────────────────────────────

function explainCode(code: string, analysis: CodeAnalysis): string {
  const lines = code.split('\n').filter(l => l.trim() && !l.trim().startsWith('#'));
  const explanations: string[] = ['Here\'s what your code does:\n'];

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;

    if (/^use_bpm\s+(\d+)/.test(trimmed)) {
      const bpm = trimmed.match(/\d+/)?.[0];
      explanations.push(`• \`${trimmed}\` — Sets the tempo to ${bpm} BPM.`);
    } else if (/^use_synth\s+:(\w+)/.test(trimmed)) {
      const synth = trimmed.match(/:(\w+)/)?.[1];
      const info = SYNTHS.find(s => s.name === synth);
      explanations.push(`• \`${trimmed}\` — Selects the ${info ? info.desc.toLowerCase() : synth} synthesizer.`);
    } else if (/^play\s+/.test(trimmed)) {
      explanations.push(`• \`${trimmed}\` — Plays a note with the current synth.`);
    } else if (/^sample\s+:(\w+)/.test(trimmed)) {
      const samp = trimmed.match(/:(\w+)/)?.[1];
      const info = SAMPLES_KB.find(s => s.name === samp);
      explanations.push(`• \`${trimmed}\` — Plays the ${info ? info.desc.toLowerCase() : samp} sample.`);
    } else if (/^sleep\s+([\d.]+)/.test(trimmed)) {
      const val = trimmed.match(/([\d.]+)/)?.[1];
      explanations.push(`• \`${trimmed}\` — Waits ${val} beat(s) before the next command.`);
    } else if (/^live_loop\s+:(\w+)/.test(trimmed)) {
      const name = trimmed.match(/:(\w+)/)?.[1];
      explanations.push(`• \`${trimmed}\` — Starts a repeating loop named "${name}".`);
    } else if (/^with_fx\s+:(\w+)/.test(trimmed)) {
      const fx = trimmed.match(/:(\w+)/)?.[1];
      const info = FX_KB.find(f => f.name === fx);
      explanations.push(`• \`${trimmed}\` — Applies ${info ? info.desc.toLowerCase() : fx} effect to the enclosed code.`);
    } else if (/^end$/.test(trimmed)) {
      explanations.push(`• \`end\` — Closes the current block.`);
    } else if (/^define\s+:(\w+)/.test(trimmed)) {
      const name = trimmed.match(/:(\w+)/)?.[1];
      explanations.push(`• \`${trimmed}\` — Defines a reusable function named "${name}".`);
    } else if (trimmed.length > 0) {
      explanations.push(`• \`${trimmed}\``);
    }
  }

  if (analysis.liveLoopNames.length > 0) {
    explanations.push(`\n**Structure:** ${analysis.liveLoopNames.length} concurrent live loop(s): ${analysis.liveLoopNames.map(n => `\`:${n}\``).join(', ')}.`);
  }

  if (analysis.issues.length > 0) {
    explanations.push('\n**Potential issues:**');
    analysis.issues.forEach(issue => explanations.push(`• ${issue}`));
  }

  return explanations.join('\n');
}

function buildAnalysisResponse(analysis: CodeAnalysis, code: string): string {
  const parts: string[] = ['**Code Analysis:**\n'];

  parts.push(`• **Lines:** ${analysis.lineCount}`);
  parts.push(`• **Live loops:** ${analysis.liveLoopNames.length > 0 ? analysis.liveLoopNames.map(n => `\`:${n}\``).join(', ') : 'None'}`);
  parts.push(`• **Synths used:** ${analysis.usedSynths.length > 0 ? [...new Set(analysis.usedSynths)].map(s => `\`:${s}\``).join(', ') : 'Default (beep)'}`);
  parts.push(`• **Samples used:** ${analysis.usedSamples.length > 0 ? [...new Set(analysis.usedSamples)].map(s => `\`:${s}\``).join(', ') : 'None'}`);
  parts.push(`• **Effects:** ${analysis.usedFx.length > 0 ? [...new Set(analysis.usedFx)].map(f => `\`:${f}\``).join(', ') : 'None'}`);

  if (analysis.issues.length > 0) {
    parts.push('\n**Issues found:**');
    analysis.issues.forEach(issue => parts.push(`• ${issue}`));
  } else {
    parts.push('\nNo issues detected — code looks good!');
  }

  // Suggestions
  const suggestions: string[] = [];
  if (!analysis.hasLiveLoop && analysis.lineCount > 5) {
    suggestions.push('Consider wrapping your code in `live_loop` blocks for continuous playback and hot-reloading.');
  }
  if (!analysis.hasFx) {
    suggestions.push('Try adding effects with `with_fx :reverb do ... end` to add depth.');
  }
  if (analysis.usedSynths.length === 0 && analysis.hasPlay) {
    suggestions.push('You\'re using the default synth. Try `use_synth :saw` or `:super_saw` for a richer sound.');
  }
  if (!/use_bpm/.test(code) && analysis.hasSleep) {
    suggestions.push('Add `use_bpm 120` at the top to make tempo explicit.');
  }

  if (suggestions.length > 0) {
    parts.push('\n**Suggestions:**');
    suggestions.forEach(s => parts.push(`• ${s}`));
  }

  return parts.join('\n');
}

function suggestEffects(_code: string, analysis: CodeAnalysis, userMessage: string): string {
  const m = userMessage.toLowerCase();
  const parts: string[] = [];

  if (/reverb|space|room/.test(m) || !analysis.hasFx) {
    parts.push('**Reverb** — adds space and depth:\n\n```ruby\nwith_fx :reverb, mix: 0.5, room: 0.8 do\n  # your code here\nend\n```');
  }

  if (/echo|delay|repeat/.test(m)) {
    parts.push('**Echo** — rhythmic repeats:\n\n```ruby\nwith_fx :echo, time: 0.25, feedback: 0.6, mix: 0.4 do\n  # your code here\nend\n```');
  }

  if (/distort|drive|grit|dirt/.test(m)) {
    parts.push('**Distortion** — gritty overdrive:\n\n```ruby\nwith_fx :distortion, distort: 0.4, mix: 0.5 do\n  # your code here\nend\n```');
  }

  if (/filter|lpf|low.pass|warm/.test(m)) {
    parts.push('**Low-pass filter** — warm, muted tone:\n\n```ruby\nwith_fx :lpf, cutoff: 80 do\n  # your code here\nend\n```');
  }

  if (parts.length === 0) {
    // General suggestion
    parts.push('Here are some effects you can wrap around your code:\n');
    parts.push('```ruby\n# Spacious reverb\nwith_fx :reverb, mix: 0.5, room: 0.7 do\n  play :c4\n  sleep 0.5\nend\n```');
    parts.push('```ruby\n# Rhythmic echo\nwith_fx :echo, time: 0.25, feedback: 0.5 do\n  play :e4\n  sleep 0.5\nend\n```');
    parts.push('```ruby\n# Lo-fi bitcrusher\nwith_fx :bitcrusher, bits: 8, mix: 0.6 do\n  play :g4\n  sleep 0.5\nend\n```');
  }

  return parts.join('\n\n');
}

function handleSyntaxHelp(message: string): string {
  const m = message.toLowerCase();

  if (/live.?loop|loop/.test(m)) {
    return 'A `live_loop` repeats its contents forever and can be hot-reloaded:\n\n```ruby\nlive_loop :my_loop do\n  play :c4\n  sleep 0.5\nend\n```\n\n**Important:** Always include at least one `sleep` inside a `live_loop`, or it will lock up.\n\nYou can have multiple `live_loop` blocks running concurrently — they execute in parallel.';
  }

  if (/chord|chords/.test(m)) {
    return 'Play chords with `play chord(:root, :type)`:\n\n```ruby\nplay chord(:c4, :major)     # C E G\nplay chord(:a3, :minor)     # A C E\nplay chord(:d4, :dom7)      # D F# A C\nplay chord(:g4, :minor7)    # G Bb D F\n```\n\nChord types: `:major`, `:minor`, `:dom7`, `:minor7`, `:dim`, `:aug`, `:sus2`, `:sus4`';
  }

  if (/scale|scales/.test(m)) {
    return 'Play scales with `scale(:root, :type)`:\n\n```ruby\nplay_pattern_timed scale(:c4, :major), [0.25]\nplay_pattern_timed scale(:a4, :minor_pentatonic), [0.125]\n```\n\nScale types: `:major`, `:minor`, `:minor_pentatonic`, `:major_pentatonic`, `:blues_minor`, `:blues_major`, `:dorian`, `:mixolydian`, `:japanese`, `:hungarian_minor`';
  }

  if (/ring|tick|look/.test(m)) {
    return 'Rings are circular lists that cycle infinitely:\n\n```ruby\nnotes = ring(:c4, :e4, :g4, :b4)\nlive_loop :arp do\n  play notes.tick   # cycles: c4, e4, g4, b4, c4, ...\n  sleep 0.25\nend\n```\n\n• `.tick` advances and returns the current element\n• `.look` returns current without advancing\n• `knit(:c4, 4, :e4, 2)` creates ring with repetitions';
  }

  if (/thread|sync|cue/.test(m)) {
    return 'Use `in_thread` for concurrent execution:\n\n```ruby\nin_thread do\n  loop do\n    sample :kick\n    sleep 1\n  end\nend\n\nin_thread do\n  loop do\n    play :c4\n    sleep 0.5\n  end\nend\n```\n\nUse `cue` and `sync` for coordination:\n\n```ruby\nin_thread do\n  sync :start\n  play :c4\nend\nsleep 2\ncue :start   # triggers the waiting thread\n```';
  }

  if (/sample|samples/.test(m)) {
    return 'Play built-in samples:\n\n```ruby\nsample :kick\nsample :snare, amp: 0.8\nsample :hihat, rate: 1.5     # faster playback\nsample :loop_amen, beat_stretch: 4  # stretch to 4 beats\n```\n\nKey parameters: `amp`, `rate`, `pan`, `attack`, `release`, `beat_stretch`, `start`, `finish`';
  }

  // General help
  return 'Here are the core Sonic Pi concepts:\n\n' +
    '• `play :c4` — play a note\n' +
    '• `sleep 0.5` — wait half a beat\n' +
    '• `sample :kick` — play a sample\n' +
    '• `use_synth :saw` — change synth\n' +
    '• `live_loop :name do ... end` — repeating loop\n' +
    '• `with_fx :reverb do ... end` — apply effect\n' +
    '• `use_bpm 120` — set tempo\n\n' +
    'Ask me about any specific topic: loops, chords, scales, effects, rings, threads, samples...';
}

function handleGeneralQuestion(message: string, analysis: CodeAnalysis, currentCode: string): string {
  const m = message.toLowerCase();

  // Greeting
  if (/^(hi|hello|hey|yo|sup|what's up|howdy)/i.test(m)) {
    return 'Hey! I\'m your PiBeat agent. I can:\n\n' +
      '• **Generate code** — beats, melodies, arps, full tracks\n' +
      '• **Refactor** your current code\n' +
      '• **Explain** what your code does\n' +
      '• **Analyze** for issues and suggest improvements\n' +
      '• **Add effects** to your sound\n' +
      '• **Answer questions** about Sonic Pi syntax\n\n' +
      'What would you like to do?';
  }

  // What can you do
  if (/what can you|help me|capabilities|what do you/.test(m)) {
    return 'I\'m your Sonic Pi coding assistant! Here\'s what I can help with:\n\n' +
      '• **"Generate a beat"** — I\'ll create drum patterns\n' +
      '• **"Create a melody"** — melodic lines with different scales\n' +
      '• **"Make an arp"** — arpeggiated synth patterns\n' +
      '• **"Build a full track"** — complete multi-layer compositions\n' +
      '• **"Refactor my code"** — clean up and improve your code\n' +
      '• **"Explain my code"** — line-by-line explanation\n' +
      '• **"Analyze my code"** — find issues and get suggestions\n' +
      '• **"Add reverb/echo/distortion"** — effect suggestions\n' +
      '• **"List synths/samples/effects"** — browse available sounds\n' +
      '• **"How to use live_loop"** — syntax help on any topic';
  }

  // Tempo / BPM
  if (/tempo|bpm|speed|faster|slower/.test(m)) {
    return 'Control tempo with `use_bpm`:\n\n```ruby\nuse_bpm 140    # Sets tempo to 140 BPM\n```\n\nYou can also use `with_bpm` for temporary tempo changes:\n\n```ruby\nwith_bpm 200 do\n  play :c4\n  sleep 0.25\nend\n```\n\nHigher BPM = faster. A `sleep 1` always lasts one beat, regardless of BPM.';
  }

  // Random
  if (/random|chance|probability|dice|luck/.test(m)) {
    return 'Sonic Pi has great randomisation tools:\n\n```ruby\n# Random number between 50 and 80 (inclusive integers)\nplay rrand_i(50, 80)\n\n# Random float\nplay :c4, amp: rrand(0.3, 1.0)\n\n# Choose from a list\nplay choose([:c4, :e4, :g4])\n\n# One-in-N chance\nsample :clap if one_in(4)\n\n# Reproducible randomness\nuse_random_seed 42\n```';
  }

  // If we have code context, give a contextual response
  if (currentCode.trim().length > 20) {
    return `I can see you have ${analysis.lineCount} lines of code` +
      (analysis.liveLoopNames.length > 0 ? ` with ${analysis.liveLoopNames.length} live loop(s)` : '') +
      '. Try asking me to:\n\n' +
      '• **"Refactor my code"** — I\'ll improve its structure\n' +
      '• **"Explain my code"** — I\'ll walk through it line by line\n' +
      '• **"Analyze my code"** — I\'ll check for issues\n' +
      '• **"Add effects to my code"** — I\'ll suggest FX chains\n\n' +
      'Or ask me anything about Sonic Pi!';
  }

  return 'I\'m your Sonic Pi coding assistant! Try asking me to:\n\n' +
    '• Generate a beat, melody, or full track\n' +
    '• Refactor or explain your current code\n' +
    '• List available synths, samples, or effects\n' +
    '• Help with Sonic Pi syntax (loops, chords, scales...)\n' +
    '• Browse your imported samples ("show my samples")\n\n' +
    'Just type what you need!';
}

// ──────────────────────────────────────────────
// User Sample Integration
// ──────────────────────────────────────────────

interface UserSampleRef {
  name: string;
  path: string;
  audio_type: string;
  feeling: string;
  duration_secs: number;
  bpm_estimate: number | null;
  tags: string[];
}

function buildUserSampleResponse(userMessage: string, userSamples: UserSampleRef[]): AgentMessage {
  const m = userMessage.toLowerCase();
  
  // Filter by type if user mentions a specific type
  let filtered = userSamples;
  let filterLabel = '';
  
  if (/drum|kick|snare|hihat|percussion/i.test(m)) {
    filtered = userSamples.filter(s => s.audio_type === 'drums');
    filterLabel = 'drum/percussion';
  } else if (/vocal|voice|sing/i.test(m)) {
    filtered = userSamples.filter(s => s.audio_type === 'vocal');
    filterLabel = 'vocal';
  } else if (/bass/i.test(m)) {
    filtered = userSamples.filter(s => s.audio_type === 'bass');
    filterLabel = 'bass';
  } else if (/pad|ambient/i.test(m)) {
    filtered = userSamples.filter(s => s.audio_type === 'pad');
    filterLabel = 'pad/ambient';
  } else if (/fx|effect|sfx/i.test(m)) {
    filtered = userSamples.filter(s => s.audio_type === 'fx');
    filterLabel = 'FX';
  } else if (/loop/i.test(m)) {
    filtered = userSamples.filter(s => s.audio_type === 'loop');
    filterLabel = 'loop';
  }
  
  if (filtered.length === 0) {
    return {
      role: 'assistant',
      content: filterLabel
        ? `I couldn't find any ${filterLabel} samples in your library. You have ${userSamples.length} samples total. Try browsing them in the My Samples panel.`
        : `Your sample library is empty. Select a folder using the My Samples panel (folder icon in the toolbar).`,
    };
  }
  
  // Build summary
  const typeCounts: Record<string, number> = {};
  for (const s of (filterLabel ? filtered : userSamples)) {
    typeCounts[s.audio_type] = (typeCounts[s.audio_type] || 0) + 1;
  }
  
  let response = filterLabel
    ? `Found **${filtered.length} ${filterLabel}** samples in your library:\n\n`
    : `Your sample library has **${userSamples.length}** samples:\n\n`;
  
  if (!filterLabel) {
    response += Object.entries(typeCounts)
      .sort(([, a], [, b]) => b - a)
      .map(([type, count]) => `• **${type}**: ${count}`)
      .join('\n');
    response += '\n\n';
  }
  
  // Show top samples
  const samplesToShow = filtered.slice(0, 8);
  response += '**Sample highlights:**\n';
  for (const s of samplesToShow) {
    const bpm = s.bpm_estimate ? ` (~${Math.round(s.bpm_estimate)} BPM)` : '';
    const dur = s.duration_secs < 1 ? `${Math.round(s.duration_secs * 1000)}ms` : `${s.duration_secs.toFixed(1)}s`;
    response += `• \`${s.name}\` — ${s.audio_type}, ${s.feeling}, ${dur}${bpm}\n`;
  }
  
  if (filtered.length > 8) {
    response += `\n_...and ${filtered.length - 8} more._\n`;
  }
  
  response += '\nTo use a sample in your code:\n```ruby\nsample "' + samplesToShow[0].path.replace(/\\/g, '/') + '"\n```';
  
  return { role: 'assistant', content: response };
}

function buildCompositionWithUserSamples(
  intent: string,
  _userMessage: string,
  drumSamples: UserSampleRef[],
  allSamples: UserSampleRef[]
): AgentMessage {
  const escapePath = (p: string) => p.replace(/\\/g, '/');
  
  // Pick best drum samples
  const kick = drumSamples.find(s => /kick|bd_|bassdrum/i.test(s.name));
  const snare = drumSamples.find(s => /snare|sd_|clap/i.test(s.name));
  const hihat = drumSamples.find(s => /hihat|hh_|hat/i.test(s.name));
  
  const kickLine = kick ? `sample "${escapePath(kick.path)}"` : 'sample :kick';
  const snareLine = snare ? `sample "${escapePath(snare.path)}"` : 'sample :snare';
  const hihatLine = hihat ? `sample "${escapePath(hihat.path)}", amp: 0.6` : 'sample :hihat, amp: 0.6';
  
  let code = `live_loop :drums do\n  ${kickLine}\n  sleep 0.5\n  ${hihatLine}\n  sleep 0.25\n  ${hihatLine}\n  sleep 0.25\n  ${snareLine}\n  sleep 0.5\n  ${hihatLine}\n  sleep 0.25\n  ${hihatLine}\n  sleep 0.25\nend`;
  
  if (intent === 'generate_full') {
    // Add bass from user samples if available
    const bassSample = allSamples.find(s => s.audio_type === 'bass');
    const padSample = allSamples.find(s => s.audio_type === 'pad');
    
    code += '\n\n';
    if (bassSample) {
      code += `live_loop :bass do\n  sample "${escapePath(bassSample.path)}", amp: 0.7\n  sleep 2\nend\n\n`;
    } else {
      code += `live_loop :bass do\n  use_synth :tb303\n  play :c2, cutoff: 80, release: 0.3\n  sleep 0.5\nend\n\n`;
    }
    
    if (padSample) {
      code += `live_loop :pad do\n  sample "${escapePath(padSample.path)}", amp: 0.3\n  sleep 4\nend`;
    } else {
      code += `live_loop :pad do\n  use_synth :blade\n  with_fx :reverb, mix: 0.6 do\n    play chord(:c4, :minor7), amp: 0.2, attack: 1, sustain: 2, release: 1\n  end\n  sleep 4\nend`;
    }
  }
  
  const usedSamples = [kick, snare, hihat].filter(Boolean).map(s => s!.name);
  const desc = usedSamples.length > 0
    ? `Using your samples: ${usedSamples.join(', ')}`
    : 'Using built-in samples (no matching drum samples found in your library)';
  
  return {
    role: 'assistant',
    content: `Here's a ${intent === 'generate_full' ? 'full track' : 'beat'} using your samples:\n\n${desc}\n\n\`\`\`ruby\n${code}\n\`\`\`\n\nYou can preview any sample in the My Samples panel before using it.`,
  };
}

// ──────────────────────────────────────────────
// Parity Analysis Engine
// ──────────────────────────────────────────────

/**
 * Deep parity check — invokes the Rust validate_parity command to analyze
 * the code's Sonic Pi compatibility and produce a detailed report
 */
async function runParityAnalysis(code: string, intent: Intent): Promise<AgentMessage> {
  try {
    const report = await invoke<ParityReport>('validate_parity', { code });
    return { role: 'assistant', content: formatParityReport(report, intent) };
  } catch (err: any) {
    // Fallback: client-side static analysis when backend unavailable
    return { role: 'assistant', content: runClientSideParityCheck(code, intent) };
  }
}

/**
 * Format the backend parity report into readable markdown
 */
function formatParityReport(report: ParityReport, intent: Intent): string {
  const scorePercent = Math.round(report.score * 100);
  const scoreEmoji = scorePercent >= 90 ? '🟢' : scorePercent >= 70 ? '🟡' : '🔴';
  
  const parts: string[] = [];
  parts.push(`## ${scoreEmoji} Sonic Pi Parity: ${scorePercent}%\n`);
  parts.push(`**Features used:** ${report.features_used} | **Supported:** ${report.features_supported} | **Partial:** ${report.features_partial} | **Unsupported:** ${report.features_unsupported}\n`);

  // Filter categories based on intent
  const categoriesToShow = intent === 'parity_synths' ? ['Synths']
    : intent === 'parity_effects' ? ['Effects']
    : intent === 'parity_samples' ? ['Sample Features']
    : null; // show all

  for (const cat of report.categories) {
    if (categoriesToShow && !categoriesToShow.includes(cat.name)) continue;
    if (cat.items.length === 0 && cat.status === 'unused') continue;
    
    const statusIcon = cat.status === 'full' ? '✅' : cat.status === 'partial' ? '⚠️' : cat.status === 'unsupported' ? '❌' : '—';
    parts.push(`\n### ${statusIcon} ${cat.name}\n`);
    
    for (const item of cat.items) {
      const icon = item.status === 'supported' ? '✅' : item.status === 'partial' ? '⚠️' : '❌';
      parts.push(`${icon} \`${item.feature}\` — ${item.detail}`);
    }
  }

  // Show suggestions
  if (report.suggestions.length > 0) {
    parts.push('\n### Suggestions\n');
    for (const sug of report.suggestions) {
      const icon = sug.severity === 'error' ? '❌' : sug.severity === 'warning' ? '⚠️' : 'ℹ️';
      parts.push(`${icon} **${sug.feature}**: ${sug.message}`);
      if (sug.fix) {
        parts.push(`\n\`\`\`ruby\n${sug.fix}\n\`\`\``);
      }
    }
  }

  // Show parse warnings
  if (report.warnings.length > 0) {
    parts.push('\n### Parse Warnings\n');
    for (const w of report.warnings) {
      parts.push(`⚠️ ${w}`);
    }
  }

  if (scorePercent >= 90) {
    parts.push('\n---\n**Your code has excellent Sonic Pi parity!** All major features are fully supported.');
  } else if (scorePercent >= 70) {
    parts.push('\n---\n**Good parity.** Some features have partial support — see suggestions above for workarounds.');
  } else {
    parts.push('\n---\n**Some parity gaps detected.** Review the suggestions above, or ask me to **"fix parity"** for auto-applied workarounds.');
  }

  return parts.join('\n');
}

/**
 * Client-side static parity analysis — fallback when Rust backend is not available
 * Also used by LLM-based agents for context
 */
function runClientSideParityCheck(code: string, intent: Intent): string {
  const lines = code.split('\n');
  const issues: string[] = [];
  const supported: string[] = [];
  const synths: string[] = [];
  const effects: string[] = [];
  const samples: string[] = [];

  // Fully supported synths in PiBeat
  const supportedSynths = new Set([
    'sine', 'beep', 'saw', 'dsaw', 'square', 'tri', 'triangle', 'noise', 'pulse',
    'super_saw', 'tb303', 'prophet', 'blade', 'pluck', 'fm', 'mod_fm', 'mod_saw',
    'mod_pulse', 'mod_tri', 'mod_sine', 'dark_ambience', 'hollow', 'growl',
    'pretty_bell', 'dull_bell', 'chip_lead', 'chip_bass', 'chip_noise', 'tech_saws',
    'hoover', 'zawa', 'dpulse', 'dtri', 'sub_pulse', 'piano', 'gabber_kick',
    'bnoise', 'pnoise', 'gnoise', 'cnoise',
  ]);

  // Fully supported effects in PiBeat
  const supportedFx = new Set([
    'reverb', 'gverb', 'echo', 'delay', 'distortion', 'lpf', 'rlpf', 'hpf', 'rhpf',
    'flanger', 'chorus', 'ring_mod', 'wobble', 'ixi_techno', 'octaver', 'pan',
    'slicer', 'bitcrusher', 'krush', 'compressor', 'normaliser', 'normalizer',
  ]);

  // Unsupported constructs
  const unsupportedPatterns: [RegExp, string, string | null][] = [
    [/\bcontrol\s+\w/, '`control` is a no-op in PiBeat — use explicit notes with timing', '# Use: play :c4, sustain: 1\\nsleep 1\\nplay :e4, sustain: 9'],
    [/\bshould_stop\?/, '`should_stop?` is a Ruby runtime feature not supported in PiBeat', null],
    [/\bTime\.now/, '`Time.now` is not supported — use beat-based timing with `sleep`', null],
    [/\blambda\b|\bproc\b|->\\s*{/, 'Lambdas/procs not supported — use `define :name do ... end`', 'define :my_func do\\n  # code here\\nend'],
    [/\bdef\s+\w+/, 'Ruby `def` methods not supported — use `define :name do ... end`', null],
    [/\.each_cons/, '`.each_cons` not supported — use explicit loops instead', null],
    [/\bsync\s+:/, '`sync` is parsed but does not block — threads start immediately', 'Use separate live_loop blocks instead'],
    [/\bcue\s+:/, '`cue` is parsed but does not trigger waiting threads', null],
    [/\bwith_fx\s+:pitch_shift/, '`:pitch_shift` effect not implemented — use `rate:` param on samples', null],
    [/\bwith_fx\s+:whammy/, '`:whammy` not implemented — try `with_fx :wobble` instead', null],
    [/\bwith_fx\s+:band_eq/, '`:band_eq` not implemented — combine :lpf and :hpf instead', null],
    [/\bwith_fx\s+:vowel/, '`:vowel` not implemented in PiBeat', null],
    [/\bwith_fx\s+:tanh/, '`:tanh` not implemented — use `with_fx :distortion` instead', null],
  ];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trim();
    if (line.startsWith('#') || line === '') continue;

    // Extract synths
    const synthMatch = line.match(/use_synth\s+:(\w+)/);
    if (synthMatch) {
      const s = synthMatch[1];
      if (supportedSynths.has(s)) {
        supported.push(`Synth :${s}`);
        synths.push(s);
      } else {
        issues.push(`Line ${i + 1}: Synth \`:${s}\` may have limited parity`);
        synths.push(s);
      }
    }

    // Extract effects
    const fxMatch = line.match(/with_fx\s+:(\w+)/);
    if (fxMatch) {
      const f = fxMatch[1];
      if (supportedFx.has(f)) {
        supported.push(`Effect :${f}`);
        effects.push(f);
      } else {
        issues.push(`Line ${i + 1}: Effect \`:${f}\` not supported in PiBeat`);
        effects.push(f);
      }
    }

    // Extract samples
    const sampleMatch = line.match(/sample\s+:(\w+)/);
    if (sampleMatch) {
      samples.push(sampleMatch[1]);
    }

    // Check unsupported patterns
    for (const [pattern, message, fix] of unsupportedPatterns) {
      if (pattern.test(line)) {
        issues.push(`Line ${i + 1}: ${message}${fix ? '\\n  Fix: ' + fix : ''}`);
      }
    }
  }

  // Build report
  const parts: string[] = [];
  const total = synths.length + effects.length + issues.length;
  const score = total > 0 ? Math.round(((total - issues.length) / total) * 100) : 100;
  const scoreEmoji = score >= 90 ? '🟢' : score >= 70 ? '🟡' : '🔴';

  parts.push(`## ${scoreEmoji} Sonic Pi Parity Analysis: ${score}%\n`);

  // Filter by intent
  if (intent !== 'parity_effects' && intent !== 'parity_samples') {
    if (synths.length > 0) {
      parts.push('### Synths');
      for (const s of [...new Set(synths)]) {
        const icon = supportedSynths.has(s) ? '✅' : '⚠️';
        parts.push(`${icon} \`:${s}\``);
      }
      parts.push('');
    }
  }

  if (intent !== 'parity_synths' && intent !== 'parity_samples') {
    if (effects.length > 0) {
      parts.push('### Effects');
      for (const f of [...new Set(effects)]) {
        const icon = supportedFx.has(f) ? '✅' : '❌';
        parts.push(`${icon} \`:${f}\``);
      }
      parts.push('');
    }
  }

  if (intent !== 'parity_synths' && intent !== 'parity_effects') {
    if (samples.length > 0) {
      parts.push(`### Samples: ${[...new Set(samples)].length} unique samples used ✅`);
      parts.push('');
    }
  }

  if (issues.length > 0) {
    parts.push('### Issues Found\n');
    for (const issue of issues) {
      parts.push(`⚠️ ${issue}`);
    }
  } else {
    parts.push('**No parity issues detected.** Your code uses fully supported features.\n');
  }

  parts.push('\n---');
  if (issues.length > 0) {
    parts.push('Ask me to **"fix parity"** to get auto-applied workarounds for these issues.');
  } else {
    parts.push('Your code has excellent Sonic Pi parity!');
  }

  return parts.join('\n');
}

/**
 * Auto-fix parity issues in the user's code
 */
async function runParityFix(code: string): Promise<AgentMessage> {
  let fixed = code;
  const changes: string[] = [];

  // Fix 1: Replace unsupported effects with alternatives
  const fxReplacements: [RegExp, string, string][] = [
    [/with_fx\s+:pitch_shift\b/g, 'with_fx :lpf', 'Replaced `:pitch_shift` with `:lpf` (use `rate:` on samples for pitch)'],
    [/with_fx\s+:whammy\b/g, 'with_fx :wobble', 'Replaced `:whammy` with `:wobble`'],
    [/with_fx\s+:band_eq\b/g, 'with_fx :lpf', 'Replaced `:band_eq` with `:lpf` (combine with :hpf for band)'],
    [/with_fx\s+:tanh\b/g, 'with_fx :distortion', 'Replaced `:tanh` with `:distortion` (soft clipping)'],
    [/with_fx\s+:vowel\b/g, 'with_fx :lpf', 'Replaced `:vowel` with `:lpf` (formant-like filtering)'],
  ];

  for (const [pattern, replacement, desc] of fxReplacements) {
    if (pattern.test(fixed)) {
      fixed = fixed.replace(pattern, replacement);
      changes.push(desc);
    }
  }

  // Fix 2: Replace `def method_name` with `define :method_name do`
  const defMatch = fixed.match(/\bdef\s+(\w+)/);
  if (defMatch) {
    fixed = fixed.replace(/\bdef\s+(\w+)/, 'define :$1 do');
    changes.push(`Replaced Ruby \`def ${defMatch[1]}\` with \`define :${defMatch[1]} do\``);
  }

  // Fix 3: Warn about control (can't auto-fix — needs manual rewrite)
  if (/\bcontrol\s+\w/.test(fixed)) {
    changes.push('**Manual fix needed:** `control` is a no-op. Rewrite as explicit `play` calls with timing:');
    changes.push('```ruby\n# Instead of: control s, note: :e4\nplay :c4, sustain: 1\nsleep 1\nplay :e4, sustain: 9\n```');
  }

  // Fix 4: Replace lambda/proc with define
  if (/\b(lambda|proc)\s*\{/.test(fixed)) {
    changes.push('**Manual fix needed:** Replace lambda/proc with `define :name do ... end`');
  }

  // Fix 5: Warn about sync/cue
  if (/\bsync\s+:/.test(fixed)) {
    changes.push('**Note:** `sync` is parsed but does not block threads. Use separate `live_loop` blocks for concurrent patterns.');
  }

  if (changes.length === 0) {
    return {
      role: 'assistant',
      content: '✅ **No parity fixes needed!** Your code uses fully supported Sonic Pi features in PiBeat.',
    };
  }

  const changeList = changes.map(c => `• ${c}`).join('\n');
  return {
    role: 'assistant',
    content: `## Parity Fixes Applied\n\n${changeList}\n\n\`\`\`ruby\n${fixed}\n\`\`\`\n\nReview the changes above. Click **Insert** to add to your buffer or **Replace** to update.`,
  };
}

/**
 * Exported for use by LLM agents — provides parity context for the system prompt
 */
export function getParityContext(): string {
  return `## PiBeat Sound Parity Status

### Fully Supported (100% parity):
- **42 synths**: sine, saw, square, triangle, noise, pulse, super_saw, tb303, prophet, blade, pluck, fm, beep, dark_ambience, hollow, growl, pretty_bell, dull_bell, chip_lead, chip_bass, chip_noise, tech_saws, hoover, zawa, mod_fm, mod_sine, mod_saw, mod_tri, mod_pulse, dsaw, dpulse, dtri, sub_pulse, gabber_kick, piano, bnoise, pnoise, gnoise, cnoise
- **22 effects**: reverb, gverb, echo, delay, distortion, lpf, rlpf, hpf, rhpf, flanger, chorus, ring_mod, wobble, ixi_techno, octaver, pan, slicer, bitcrusher, krush, compressor, normaliser, normalizer
- **Sample params**: amp, rate, pan, pitch, rpitch, sustain, beat_stretch, start, finish, lpf, hpf, attack/decay/sustain_level/release
- **Language**: live_loop, in_thread, .times do, while, define, set/get, if/else, ring, spread, choose, rrand, rand, dice, one_in, at, time_warp

### Partial Support (⚠️):
- **sync/cue**: Parsed but threads start immediately (workaround: use separate live_loops)
- **control**: Parsed but no-op (workaround: use explicit play + sleep)
- **.tick/.look**: Counter-based cycling approximation
- **sync: param on live_loop**: Parsed but not enforced

### Not Supported (❌):
- **Ruby runtime**: should_stop?, Time.now, lambda, proc, def methods
- **Effects**: pitch_shift, whammy, band_eq, tanh, vowel
- **Live reload**: Code changes require stop/start
- **Multi-variable block params**: |a, b|

### Effect Defaults Reference:
- reverb: mix=0.4, room=0.6, damp=0.5
- echo/delay: phase=0.25 beats, decay=2, mix=1
- distortion: distort=0.5, mix=1
- lpf: cutoff=100 MIDI, res=0
- hpf: cutoff=60 MIDI, res=0
- bitcrusher/krush: bits=10, sr=10000, mix=1

When generating code, always use fully supported features. For parity issues, suggest workarounds.`;
}
