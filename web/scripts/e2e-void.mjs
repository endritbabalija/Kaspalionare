// The void end to end test, on request only (owner, DECISIONS 2026-10-27): it mines about 864,000 simnet blocks
// (about 35 minutes, 2.4 GB of temporary disk, the dev node slow afterwards). Run it before committing any change
// to draws, voids or the rounds pages, after `npm run e2e` in the same dev run (it starts from the TEST-ONLY
// `POST /round`, whatever ran before). `npm run e2e` alone skips it; this sets E2E_VOID=1 and runs only it.
import path from 'node:path';
import { WEB, run } from './lib.mjs';

const env = { ...process.env, E2E_VOID: '1' };
run(
  process.execPath,
  [
    path.join(WEB, 'node_modules', '@playwright', 'test', 'cli.js'),
    'test',
    'e2e/void.spec.ts',
    ...process.argv.slice(2),
  ],
  { cwd: WEB, env },
);
