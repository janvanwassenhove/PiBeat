import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './scripts',
  testMatch: 'capture-screenshots.spec.ts',
  timeout: 60000,
  retries: 0,
  use: {
    baseURL: 'http://localhost:1420',
    headless: true,
    screenshot: 'off',
    video: 'off',
    trace: 'off',
  },
  webServer: {
    command: 'npm run dev -- --port 1420',
    port: 1420,
    timeout: 30000,
    reuseExistingServer: true,
  },
});
