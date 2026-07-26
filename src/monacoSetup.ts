/**
 * Bind `@monaco-editor/react` to the locally installed Monaco.
 *
 * By default the React wrapper pulls Monaco from the jsDelivr CDN at runtime.
 * In a packaged desktop app that means the editor — the centre of the whole
 * screen — cannot appear until a network round trip completes, and does not
 * appear at all offline. Bundling Monaco instead makes the editor available
 * from local files.
 *
 * Importing this module for its side effect must happen before the first
 * `<Editor>` renders, so it is imported from `main.tsx`.
 */
// `edcore.main` is the complete editor — find, folding, suggest, multi-cursor
// — minus the bundled languages. The default `monaco-editor` entry point also
// drags in the TypeScript, CSS, HTML and JSON language services and their web
// workers, roughly 9 MB of code for languages PiBeat never opens. It only
// edits its own Sonic Pi dialect, registered as a Monarch grammar in
// CodeEditor.tsx.
import * as monaco from 'monaco-editor/esm/vs/editor/edcore.main.js';
import { loader } from '@monaco-editor/react';

import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';

// Monaco asks the host page which worker to use per language. With no language
// services bundled, the generic editor worker (tokenisation, find, diff) is
// the only one that can be asked for.
self.MonacoEnvironment = {
  getWorker() {
    return new editorWorker();
  },
};

loader.config({ monaco });

export {};
