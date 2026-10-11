import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 45_000,
  retries: process.env.CI ? 1 : 0,
  use: { baseURL: 'http://127.0.0.1:5173', browserName: 'chromium', headless: true },
  webServer: [
    { command: 'python3 scripts/studio_bridge.py', cwd: '..', url: 'http://127.0.0.1:8765/api/catalog', reuseExistingServer: !process.env.CI, timeout: 120_000 },
    { command: 'npm run dev', url: 'http://127.0.0.1:5173', reuseExistingServer: !process.env.CI, timeout: 120_000 }
  ]
});
