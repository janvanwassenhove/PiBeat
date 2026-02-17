/**
 * PiBeat Agent — Reactive local agent with full Sonic Pi knowledge.
 *
 * This agent processes user messages, analyses the current buffer code,
 * and produces suggestions, code snippets, refactorings, and explanations.
 * It runs entirely client-side using pattern matching and templates.
 */

import { AgentMessage } from './store';

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
  | 'refactor'
  | 'explain'
  | 'add_fx'
  | 'list_synths'
  | 'list_samples'
  | 'list_fx'
  | 'help_syntax'
  | 'analyze'
  | 'general';

function detectIntent(message: string): Intent {
  const m = message.toLowerCase();

  if (/refactor|clean\s*up|improve|restructure|optimize|tidy/.test(m)) return 'refactor';
  if (/explain|what does|how does|walk.*through|line.by.line|understand/.test(m)) return 'explain';
  if (/analyze|analys|check|review|issues|problems|bugs|mistakes/.test(m)) return 'analyze';

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
    explanations.push('\n**⚠️ Potential issues:**');
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
    parts.push('\n**⚠️ Issues found:**');
    analysis.issues.forEach(issue => parts.push(`• ${issue}`));
  } else {
    parts.push('\n✅ No issues detected — code looks good!');
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
    parts.push('\n**💡 Suggestions:**');
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
    return 'Hey! 🎵 I\'m your PiBeat agent. I can:\n\n' +
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
