# PiBeat Screenshots

These screenshots are used in:
- **[README.md](../README.md)** — project landing page
- **GitHub Release notes** — via `raw.githubusercontent.com` URLs in [release.yml](../.github/workflows/release.yml)

## Required Screenshots

| Filename | Description | What to capture |
|----------|-------------|-----------------|
| `editor.png` | Main editor view | Monaco editor with code, waveform visualizer active, log panel visible |
| `agent-chat.png` | AI assistant panel | Agent chat open with a conversation, quick action buttons visible |
| `timeline.png` | Timeline view | Timeline view mode with clips, instruments visible |
| `band-visualizer.png` | Band visualizer | Band visualizer window with animated characters |

## Guidelines

- **Resolution**: Capture at 1400x900 (the default window size) or higher
- **Theme**: Use the default PiBeat theme (dark blue)
- **Content**: Have meaningful Sonic Pi code in the editor (e.g., a drum beat + melody)
- **Format**: PNG, optimized for web (use a tool like `pngquant` or TinyPNG to compress)
- **Naming**: Use kebab-case filenames, no spaces

## How to Update

1. Run PiBeat locally: `npm run tauri dev`
2. Load an example or write code that showcases the feature
3. Take a screenshot (Win+Shift+S on Windows, Cmd+Shift+4 on macOS)
4. Crop to the application window
5. Save to this directory with the correct filename
6. Commit and push to `main` — release notes reference `raw.githubusercontent.com/...main/screenshots/...`
