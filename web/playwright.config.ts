// Playwright against the simnet dev run (spec 8): `npm run e2e` in web/. It reuses a running
// `npm run dev:simnet`, or starts one (the first start builds the wasm and simnet: up to ~20 minutes).
// Browsers live in web/.playwright-browsers (`npm run e2e:install`), never the user profile's cache.
import path from 'node:path';
import { defineConfig, devices } from '@playwright/test';

const web = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'));
process.env.PLAYWRIGHT_BROWSERS_PATH ??= path.join(web, '.playwright-browsers');

export default defineConfig({
  testDir: 'e2e',
  timeout: 180_000,
  expect: { timeout: 90_000 },
  fullyParallel: false,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: 'http://127.0.0.1:5173',
    trace: 'retain-on-failure',
    viewport: { width: 390, height: 844 },
    deviceScaleFactor: 2,
    isMobile: false,
  },
  projects: [{ name: 'chromium-phone', use: { ...devices['Desktop Chrome'], viewport: { width: 390, height: 844 } } }],
  webServer: {
    command: 'node scripts/dev-simnet.mjs',
    url: 'http://127.0.0.1:5173',
    reuseExistingServer: true,
    timeout: 20 * 60_000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
});
