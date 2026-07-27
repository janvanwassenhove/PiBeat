/// <reference types="vite/client" />

declare const __APP_VERSION__: string;

/**
 * Monaco's language-free entry point.
 *
 * `edcore.main.js` is the full editor — find, folding, suggest, multi-cursor,
 * the lot — without the bundled languages or the TypeScript/CSS/HTML/JSON
 * language services, which together are several megabytes PiBeat never uses.
 * The package ships no declaration file for this subpath, so re-export the
 * public API's types here.
 */
declare module 'monaco-editor/esm/vs/editor/edcore.main.js' {
  export * from 'monaco-editor';
}
