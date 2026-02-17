import React, { useRef, useEffect } from 'react';
import Editor, { OnMount } from '@monaco-editor/react';
import { useStore } from '../store';

// Register custom language for Sonic Pi syntax
const SONIC_KEYWORDS = [
  'play', 'sleep', 'sample', 'use_synth', 'use_bpm', 'live_loop',
  'with_fx', 'loop', 'do', 'end', 'puts', 'print', 'log',
  'set_volume', 'play_pattern_timed', 'play_pattern',
  'in_thread', 'sync', 'cue', 'define', 'use_random_seed',
  'rrand', 'rrand_i', 'choose', 'ring', 'knit', 'spread',
  'tick', 'look', 'at', 'density', 'with_bpm',
  'midi_note_on', 'midi_note_off', 'use_midi_defaults',
];

const SYNTH_NAMES = [
  'sine', 'beep', 'saw', 'dsaw', 'square', 'tri', 'triangle',
  'noise', 'cnoise', 'bnoise', 'pulse', 'dpulse', 'supersaw',
  'super_saw', 'blade', 'prophet', 'tb303', 'pluck',
  'fm', 'mod_fm', 'mod_saw', 'mod_pulse', 'mod_tri',
];

const FX_NAMES = [
  'reverb', 'echo', 'delay', 'distortion', 'lpf', 'hpf',
  'flanger', 'slicer', 'wobble', 'panslicer', 'compressor',
  'pitch_shift', 'ring_mod', 'normaliser', 'bitcrusher',
];

const SAMPLE_NAMES = [
  'kick', 'snare', 'hihat', 'clap', 'bass', 'perc',
  'ambi_choir', 'ambi_dark_woosh', 'ambi_drone',
  'bd_ada', 'bd_boom', 'bd_808',
  'drum_bass_hard', 'drum_heavy_kick', 'drum_snare_soft',
  'elec_beep', 'elec_blip',
  'loop_amen', 'loop_breakbeat',
  'misc_cineboom',
];

const CodeEditor: React.FC = () => {
  const { buffers, activeBufferId, updateBufferCode, theme } = useStore();
  const activeBuffer = buffers.find(b => b.id === activeBufferId);
  const editorRef = useRef<any>(null);
  const monacoRef = useRef<any>(null);

  const handleEditorMount: OnMount = (editor, monaco) => {
    editorRef.current = editor;

    // Register Sonic Pi language
    monaco.languages.register({ id: 'sonicpi' });

    monaco.languages.setMonarchTokensProvider('sonicpi', {
      keywords: SONIC_KEYWORDS,
      synths: SYNTH_NAMES,
      effects: FX_NAMES,
      samples: SAMPLE_NAMES,

      tokenizer: {
        root: [
          [/#.*$/, 'comment'],
          [/"[^"]*"/, 'string'],
          [/'[^']*'/, 'string'],
          [/:[a-zA-Z_]\w*/, 'type.identifier'],
          [/\b(play|sleep|sample|use_synth|use_bpm|live_loop|with_fx|loop|do|end)\b/, 'keyword'],
          [/\b(puts|print|log)\b/, 'keyword'],
          [/\b(amp|sustain|release|attack|decay|rate|pan|cutoff|res|mix|room|time|feedback|phase)\b/, 'variable'],
          [/\b\d+\.?\d*\b/, 'number'],
          [/[{}()\[\]]/, '@brackets'],
        ],
      },
    });

    monaco.languages.setLanguageConfiguration('sonicpi', {
      comments: {
        lineComment: '#',
      },
      brackets: [
        ['{', '}'],
        ['[', ']'],
        ['(', ')'],
      ],
      autoClosingPairs: [
        { open: '{', close: '}' },
        { open: '[', close: ']' },
        { open: '(', close: ')' },
        { open: '"', close: '"' },
        { open: "'", close: "'" },
      ],
    });

    // Register completions
    monaco.languages.registerCompletionItemProvider('sonicpi', {
      provideCompletionItems: (model: any, position: any) => {
        const word = model.getWordUntilPosition(position);
        const range = {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: word.startColumn,
          endColumn: word.endColumn,
        };

        const suggestions = [
          ...SONIC_KEYWORDS.map(k => ({
            label: k,
            kind: monaco.languages.CompletionItemKind.Keyword,
            insertText: k === 'live_loop' ? 'live_loop :${1:name} do\n  ${2:# code}\nend' :
              k === 'with_fx' ? 'with_fx :${1:reverb} do\n  ${2:# code}\nend' :
              k === 'play' ? 'play :${1:c4}, amp: ${2:0.5}, sustain: ${3:0.5}' :
              k === 'sample' ? 'sample :${1:kick}, amp: ${2:1}' :
              k === 'sleep' ? 'sleep ${1:0.5}' :
              k === 'use_synth' ? 'use_synth :${1:sine}' :
              k === 'use_bpm' ? 'use_bpm ${1:120}' :
              k,
            insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            range,
          })),
          ...SYNTH_NAMES.map(s => ({
            label: `:${s}`,
            kind: monaco.languages.CompletionItemKind.Value,
            insertText: s,
            range,
            detail: 'Synth',
          })),
          ...SAMPLE_NAMES.map(s => ({
            label: `:${s}`,
            kind: monaco.languages.CompletionItemKind.Value,
            insertText: s,
            range,
            detail: 'Sample',
          })),
          ...FX_NAMES.map(f => ({
            label: `:${f}`,
            kind: monaco.languages.CompletionItemKind.Value,
            insertText: f,
            range,
            detail: 'Effect',
          })),
        ];

        return { suggestions };
      },
    });

    // Custom theme — PiBeat (default)
    monaco.editor.defineTheme('sonicDark', {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'comment', foreground: '6A9955', fontStyle: 'italic' },
        { token: 'keyword', foreground: 'C586C0', fontStyle: 'bold' },
        { token: 'type.identifier', foreground: '4EC9B0' },
        { token: 'variable', foreground: '9CDCFE' },
        { token: 'number', foreground: 'B5CEA8' },
        { token: 'string', foreground: 'CE9178' },
      ],
      colors: {
        'editor.background': '#1a1a2e',
        'editor.foreground': '#e0e0e0',
        'editor.lineHighlightBackground': '#232345',
        'editorCursor.foreground': '#00ff88',
        'editor.selectionBackground': '#3a3a6a',
        'editorLineNumber.foreground': '#555580',
        'editorLineNumber.activeForeground': '#8888bb',
      },
    });

    // Custom theme — Sonic Pi Classic
    monaco.editor.defineTheme('sonicPiClassic', {
      base: 'vs-dark',
      inherit: true,
      rules: [
        { token: 'comment', foreground: '666666', fontStyle: 'italic' },
        { token: 'keyword', foreground: 'FF59B2', fontStyle: 'bold' },
        { token: 'type.identifier', foreground: 'FFDD00' },
        { token: 'variable', foreground: 'FF59B2' },
        { token: 'number', foreground: 'FFDD00' },
        { token: 'string', foreground: '5EC44F' },
      ],
      colors: {
        'editor.background': '#0a0a0a',
        'editor.foreground': '#ffffff',
        'editor.lineHighlightBackground': '#151515',
        'editorCursor.foreground': '#ff59b2',
        'editor.selectionBackground': '#2a1a22',
        'editorLineNumber.foreground': '#444444',
        'editorLineNumber.activeForeground': '#888888',
      },
    });

    monacoRef.current = monaco;

    // Apply theme based on current store state
    const currentTheme = useStore.getState().theme;
    monaco.editor.setTheme(currentTheme === 'sonicpi' ? 'sonicPiClassic' : 'sonicDark');

    // Key bindings
    editor.addAction({
      id: 'run-code',
      label: 'Run Code',
      keybindings: [monaco.KeyMod.Alt | monaco.KeyCode.KeyR],
      run: () => {
        useStore.getState().runCode();
      },
    });

    editor.addAction({
      id: 'stop-code',
      label: 'Stop',
      keybindings: [monaco.KeyMod.Alt | monaco.KeyCode.KeyS],
      run: () => {
        useStore.getState().stopAudio();
      },
    });

    editor.addAction({
      id: 'toggle-recording',
      label: 'Toggle Recording',
      keybindings: [monaco.KeyMod.Alt | monaco.KeyMod.Shift | monaco.KeyCode.KeyR],
      run: () => {
        const state = useStore.getState();
        if (state.isRecording) {
          state.stopRecording();
        } else {
          state.startRecording();
        }
      },
    });
  };

  // Switch Monaco theme when app theme changes
  useEffect(() => {
    if (monacoRef.current) {
      monacoRef.current.editor.setTheme(
        theme === 'sonicpi' ? 'sonicPiClassic' : 'sonicDark'
      );
    }
  }, [theme]);

  return (
    <div className="code-editor">
      <Editor
        height="100%"
        language="sonicpi"
        theme={theme === 'sonicpi' ? 'sonicPiClassic' : 'sonicDark'}
        value={activeBuffer?.code || ''}
        onChange={(value) => {
          if (value !== undefined) {
            updateBufferCode(activeBufferId, value);
          }
        }}
        onMount={handleEditorMount}
        options={{
          fontSize: 15,
          fontFamily: "'Fira Code', 'Cascadia Code', 'JetBrains Mono', monospace",
          fontLigatures: true,
          minimap: { enabled: false },
          scrollBeyondLastLine: false,
          lineNumbers: 'on',
          renderLineHighlight: 'all',
          cursorBlinking: 'smooth',
          cursorSmoothCaretAnimation: 'on',
          smoothScrolling: true,
          tabSize: 2,
          wordWrap: 'on',
          automaticLayout: true,
          padding: { top: 10 },
        }}
      />
    </div>
  );
};

export default CodeEditor;
