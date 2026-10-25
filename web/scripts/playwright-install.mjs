// Install Playwright's Chromium into web/.playwright-browsers (never the user profile's cache), the same
// folder playwright.config.ts points PLAYWRIGHT_BROWSERS_PATH at.
import path from 'node:path';
import { WEB, run } from './lib.mjs';

const env = { ...process.env, PLAYWRIGHT_BROWSERS_PATH: path.join(WEB, '.playwright-browsers') };
run(process.execPath, [path.join(WEB, 'node_modules', '@playwright', 'test', 'cli.js'), 'install', 'chromium'], {
  cwd: WEB,
  env,
});
