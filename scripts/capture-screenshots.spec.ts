/**
 * Automated screenshot capture for PiBeat.
 *
 * Runs the Vite dev server, opens the app in a headless browser,
 * and captures screenshots of each major view.
 *
 * Usage:
 *   npx playwright install chromium   # one-time setup
 *   npx playwright test scripts/capture-screenshots.ts
 *
 * Or via the npm script:
 *   npm run screenshots
 */
import { test, expect } from '@playwright/test';

const VIEWPORT = { width: 1400, height: 900 };

// Sample code to display in the editor for a realistic screenshot
const DEMO_CODE = `live_loop :drums do
  sample :kick
  sleep 0.5
  sample :hihat, amp: 0.6
  sleep 0.25
  sample :hihat, amp: 0.4
  sleep 0.25
end

live_loop :bass do
  use_synth :tb303
  play :c2, cutoff: 80, release: 0.3
  sleep 0.5
end`;

/**
 * Inject mock for window.__TAURI_INTERNALS__ so the app boots
 * without the Rust backend.
 */
async function mockTauriIPC(page: import('@playwright/test').Page) {
  await page.addInitScript(() => {
    const sampleNames = [
      'kick', 'snare', 'hihat', 'clap', 'rim',
      'loop_amen', 'loop_breakbeat', 'bass_hit_c',
      'ambi_choir', 'ambi_drone',
    ];

    const mockSamples = sampleNames.map((name, i) => ({
      name,
      category: i < 5 ? 'drums' : i < 8 ? 'loops' : 'ambient',
      path: `/samples/${name}.wav`,
    }));

    const mockWaveform = Array.from({ length: 200 }, (_, i) =>
      Math.sin(i * 0.15) * 0.5 + Math.sin(i * 0.07) * 0.3
    );

    const mockLogs = [
      { timestamp: Date.now() - 3000, message: '=> Playing live_loop :drums', level: 'info' },
      { timestamp: Date.now() - 2500, message: '=> Playing live_loop :bass', level: 'info' },
      { timestamp: Date.now() - 1000, message: 'sample :kick', level: 'debug' },
      { timestamp: Date.now() - 800, message: 'synth :tb303, note: 36', level: 'debug' },
    ];

    const mockScStatus = {
      available: false,
      booted: false,
      enabled: false,
      message: 'Not initialized',
    };

    let callbackId = 0;
    const callbacks: Record<number, Function> = {};

    (window as any).__TAURI_INTERNALS__ = {
      invoke(cmd: string, args?: any) {
        switch (cmd) {
          case 'get_waveform':
            return Promise.resolve(mockWaveform);
          case 'get_status':
            return Promise.resolve({
              is_playing: true,
              is_recording: false,
              bpm: 120,
              volume: 0.8,
            });
          case 'get_logs':
            return Promise.resolve(mockLogs);
          case 'get_active_lines':
            return Promise.resolve([]);
          case 'list_samples':
            return Promise.resolve(mockSamples);
          case 'get_sample_durations':
            return Promise.resolve({});
          case 'run_code':
            return Promise.resolve({ success: true, message: 'Code running' });
          case 'stop_audio':
          case 'pause_audio':
          case 'resume_audio':
          case 'clear_logs':
          case 'set_volume':
          case 'set_bpm':
          case 'set_effects':
          case 'start_recording':
          case 'stop_recording':
            return Promise.resolve(null);
          case 'get_env_var':
            return Promise.resolve(null);
          case 'get_user_samples_dir':
            return Promise.resolve(null);
          case 'sc_status':
          case 'get_sc_status':
          case 'init_supercollider':
            return Promise.resolve(mockScStatus);
          case 'init_sc':
          case 'toggle_sc_engine':
            return Promise.resolve(mockScStatus);
          case 'preview_synth':
          case 'play_sample_file':
            return Promise.resolve(null);
          case 'validate_parity':
            return Promise.resolve({ score: 95, categories: [], suggestions: [] });
          case 'get_performance_snapshot':
            return Promise.resolve({ members: [] });
          default:
            console.log(`[mock] unhandled invoke: ${cmd}`);
            return Promise.resolve(null);
        }
      },
      transformCallback(callback: Function, once: boolean) {
        const id = callbackId++;
        callbacks[id] = callback;
        return id;
      },
      unregisterCallback(id: number) {
        delete callbacks[id];
      },
      metadata: {
        currentWindow: { label: 'main' },
        currentWebview: { label: 'main' },
      },
    };
  });
}

test.describe('PiBeat Screenshots', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize(VIEWPORT);
    await mockTauriIPC(page);
  });

  test('editor view', async ({ page }) => {
    await page.goto('/', { waitUntil: 'networkidle' });
    await page.waitForSelector('.monaco-editor', { timeout: 15000 });

    // Type demo code into Monaco editor
    const editor = page.locator('.monaco-editor textarea');
    await editor.focus();
    await page.keyboard.press('Control+a');
    await page.keyboard.type(DEMO_CODE, { delay: 5 });

    // Wait for rendering to settle
    await page.waitForTimeout(1000);

    await page.screenshot({
      path: 'screenshots/editor.png',
      fullPage: false,
    });
  });

  test('agent chat view', async ({ page }) => {
    await page.goto('/', { waitUntil: 'networkidle' });
    await page.waitForSelector('.monaco-editor', { timeout: 15000 });

    // Open agent chat panel
    const agentBtn = page.locator('button[title="AI Agent Chat"]');
    await agentBtn.click();
    await page.waitForSelector('.agent-chat-panel', { timeout: 5000 });

    // Wait for panel to render
    await page.waitForTimeout(500);

    await page.screenshot({
      path: 'screenshots/agent-chat.png',
      fullPage: false,
    });
  });

  test('timeline view', async ({ page }) => {
    await page.goto('/', { waitUntil: 'networkidle' });
    await page.waitForSelector('.monaco-editor', { timeout: 15000 });

    // Type some code first so the timeline has content
    const editor = page.locator('.monaco-editor textarea');
    await editor.focus();
    await page.keyboard.press('Control+a');
    await page.keyboard.type(DEMO_CODE, { delay: 5 });

    // Switch to timeline view
    const timelineBtn = page.locator('.view-toggle-btn', { hasText: 'Timeline' });
    await timelineBtn.click();

    await page.waitForTimeout(1000);

    await page.screenshot({
      path: 'screenshots/timeline.png',
      fullPage: false,
    });
  });

  test('band visualizer', async ({ page }) => {
    await page.goto('/', { waitUntil: 'networkidle' });
    await page.waitForSelector('.monaco-editor', { timeout: 15000 });

    // Open band visualizer panel
    const bandBtn = page.locator('button[title="Band Visualizer"]');
    await bandBtn.click();

    await page.waitForTimeout(1000);

    await page.screenshot({
      path: 'screenshots/band-visualizer.png',
      fullPage: false,
    });
  });
});
