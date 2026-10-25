// Spec 6.6: the quick testnet wallet is absent from a mainnet build. The page has no mainnet build (vite.config.ts
// refuses any network but testnet-10), so this makes a TEST-ONLY build with VITE_MAINNET_CHECK=1 into
// dist-mainnet-check/ and checks that none of the quick wallet's page code is in it: its marker, the S7 key line and
// its wasm calls outside the worker. The worker chunk carries the wasm-bindgen glue of the
// release package, whose quick-wallet exports W4 already checks on the Rust side (`cargo test -p web --test
// w4_mainnet`: a mainnet wasm has none). Positive control: the same strings must be in the normal build (dist/,
// from `npm run build`). Needs src/wasm/release (npm run wasm:release). Run: npm run check:mainnet.
import { existsSync, readdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import path from 'node:path';
import { WEB, run } from './lib.mjs';

const MARKERS = [
  'KASPALIONARE_PAGE_QUICK_WALLET',
  'Testnet key stored in this browser',
  'quickWalletNew',
  'quickWalletImport',
  'quickWalletSign',
];
const out = path.join(WEB, 'dist-mainnet-check');
const normal = path.join(WEB, 'dist');

if (!existsSync(path.join(WEB, 'src', 'wasm', 'release', 'web.d.ts'))) {
  console.error('check-mainnet: src/wasm/release is missing: run `npm run wasm:release` (or `npm run build`) first');
  process.exit(2);
}

function files(dir) {
  return readdirSync(dir).flatMap((f) => {
    const p = path.join(dir, f);
    return statSync(p).isDirectory() ? files(p) : [p];
  });
}

/** The page's own JS: every .js file but the worker chunk (the wasm glue). */
function pageJs(dir) {
  return files(dir).filter((f) => f.endsWith('.js') && !path.basename(f).startsWith('lottery.worker'));
}

function finds(dir) {
  const hits = new Map(MARKERS.map((m) => [m, 0]));
  for (const f of pageJs(dir)) {
    const text = readFileSync(f).toString('latin1');
    for (const m of MARKERS) if (text.includes(m)) hits.set(m, hits.get(m) + 1);
  }
  return hits;
}

rmSync(out, { recursive: true, force: true });
run(
  process.execPath,
  [path.join(WEB, 'node_modules', 'vite', 'bin', 'vite.js'), 'build', '--outDir', out, '--emptyOutDir'],
  {
    cwd: WEB,
    env: {
      ...process.env,
      VITE_MAINNET_CHECK: '1',
      VITE_INDEXER_URL: process.env.VITE_INDEXER_URL ?? 'https://indexer.invalid/v1/',
      VITE_COVENANT_ID: process.env.VITE_COVENANT_ID ?? '00'.repeat(32),
    },
  },
);
const without = finds(out);
let bad = 0;
const control = existsSync(normal) ? finds(normal) : null;
for (const m of MARKERS) {
  const n = without.get(m);
  const c = control ? control.get(m) : '-';
  console.log(`check-mainnet: ${m.padEnd(36)} normal build ${String(c).padStart(2)}  mainnet check ${n}`);
  if (n > 0) bad++;
  if (control && c === 0) {
    console.error(`check-mainnet: the positive control failed: the normal build has no ${JSON.stringify(m)}`);
    bad++;
  }
}
if (!control) console.warn('check-mainnet: no dist/ (run `npm run build` first for the positive control)');
rmSync(out, { recursive: true, force: true });
if (bad) {
  console.error(`check-mainnet: ${bad} problems`);
  process.exit(1);
}
console.log('check-mainnet: the mainnet check build has no quick wallet page code');
