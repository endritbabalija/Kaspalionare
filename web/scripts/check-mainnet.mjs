// Spec 6.6: the quick testnet wallet is absent from a mainnet build. The page has no mainnet build (vite.config.ts
// refuses any network but testnet-10), so this makes a TEST-ONLY build with VITE_MAINNET_CHECK=1 into
// dist-mainnet-check/: the page's quick-wallet code compiled out (wallet/quickEnabled.ts) and the mainnet wasm
// package (`node scripts/wasm.mjs mainnet`: crates/web --features mainnet,node, the build W4 checks) in place of
// the testnet one. Every file that build ships is scanned, the worker chunk (the wasm-bindgen glue) and the .wasm
// itself included (F2/F3 review note: the served wasm, not only cargo's artifact): none of the quick wallet's
// marker, the S7 key line or its exports may be in it. Positive control: the same strings must be in the normal
// build (dist/, from `npm run build`). Run: npm run check:mainnet.
// F6 security review S3: src/wasm/mainnet is rebuilt on every run (cargo rebuilds what changed since; wasm-bindgen
// writes the package afresh), so the scan never reads a package left from older sources. Control: the package must
// have been written by this run (its files newer than the run's start), else the check fails as stale.
import { existsSync, readFileSync, rmSync, statSync } from 'node:fs';
import path from 'node:path';
import { WEB, run } from './lib.mjs';
import { files } from './check-bundle.mjs';

const MARKERS = [
  'KASPALIONARE_PAGE_QUICK_WALLET',
  'Testnet key stored in this browser',
  'quickWalletNew',
  'quickWalletImport',
  'quickWalletSign',
  'KASPALIONARE_QUICK_WALLET',
];
const out = path.join(WEB, 'dist-mainnet-check');
const normal = path.join(WEB, 'dist');

const started = Date.now();
run(process.execPath, [path.join(WEB, 'scripts', 'wasm.mjs'), 'mainnet'], { cwd: WEB });
const pkg = path.join(WEB, 'src', 'wasm', 'mainnet');
const stale = ['web_bg.wasm', 'web.js', 'web.d.ts'].filter(
  (f) => !existsSync(path.join(pkg, f)) || statSync(path.join(pkg, f)).mtimeMs < started - 2_000,
);
if (stale.length) {
  console.error(`check-mainnet: src/wasm/mainnet was not written by this run (${stale.join(', ')}): stale package`);
  process.exit(1);
}
console.log('check-mainnet: src/wasm/mainnet rebuilt from the current sources by this run');

function finds(dir) {
  const hits = new Map(MARKERS.map((m) => [m, 0]));
  for (const f of files(dir)) {
    const bytes = readFileSync(f);
    for (const m of MARKERS) if (bytes.includes(Buffer.from(m))) hits.set(m, hits.get(m) + 1);
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
const shipped = files(out);
const wasm = shipped.filter((f) => f.endsWith('.wasm'));
let bad = 0;
if (
  wasm.length !== 1 ||
  !readFileSync(wasm[0]).equals(readFileSync(path.join(WEB, 'src', 'wasm', 'mainnet', 'web_bg.wasm')))
) {
  console.error('check-mainnet: the check build does not ship the mainnet wasm package');
  bad++;
}
const without = finds(out);
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
console.log(
  `check-mainnet: scanned ${shipped.length} files of the check build, ${path.basename(wasm[0] ?? '-')} included`,
);
rmSync(out, { recursive: true, force: true });
if (bad) {
  console.error(`check-mainnet: ${bad} problems`);
  process.exit(1);
}
console.log('check-mainnet: the mainnet check build ships no quick wallet code, page, glue or wasm');
