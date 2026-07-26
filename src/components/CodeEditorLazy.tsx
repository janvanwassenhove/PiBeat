import React, { Suspense } from 'react';

/**
 * Monaco, loaded after the app shell has painted.
 *
 * Monaco is by far the largest thing PiBeat loads. Importing it statically
 * meant the whole editor had to be fetched, parsed and compiled before React
 * could render anything at all, so the window sat blank through it. Splitting
 * it behind `React.lazy` lets the toolbar, buffer tabs, scope and log panel
 * appear immediately, with the editor filling in a moment later.
 *
 * `monacoSetup` is awaited first: it binds `@monaco-editor/react` to the
 * bundled Monaco, and must run before the editor component mounts or the
 * wrapper falls back to fetching Monaco from a CDN.
 */
const CodeEditor = React.lazy(async () => {
  await import('../monacoSetup');
  return import('./CodeEditor');
});

/** Placeholder shown while the editor chunk loads. */
const EditorSkeleton: React.FC = () => (
  <div className="editor-skeleton" role="status" aria-label="Loading editor">
    <span className="editor-skeleton-text">Loading editor…</span>
  </div>
);

const CodeEditorLazy: React.FC = () => (
  <Suspense fallback={<EditorSkeleton />}>
    <CodeEditor />
  </Suspense>
);

export default CodeEditorLazy;
