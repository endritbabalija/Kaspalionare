// After `vite build`: the production bundle (web/dist) must not contain the dev catalogue, the dev faucet, the
// e2e hooks or the simnet configuration (spec 1.1: the catalogue in development builds only; the faucet never in
// a production build), and its wasm must be the release package with no fixture code (the W4 scans, run on the
// very file the site serves). Run by `npm run build`; exits non-zero on any find.
//
//   node scripts/check-bundle.mjs                 scan dist/ (production): every marker absent
//   node scripts/check-bundle.mjs --control DIR   scan a development build (check-bundle-control.mjs): every
//                                                 dev marker present, so the absence check is not vacuous
import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { WEB } from './lib.mjs';

/** Strings that exist in a development build at run time (each one is used, so tree-shaking keeps it). */
export const DEV_MARKERS = [
  'KASPALIONARE_DEV_CATALOGUE', // Catalogue data-marker
  'KASPALIONARE_DEV_FAUCET', // DevFaucet data-marker
  'KASPALIONARE_DEV_HOOKS', // window.__klDev.marker
  'catalogue-dark',
  '__klDev',
  'Dev faucet',
];
/** Never in any page build (simnet configuration and the fixtures' export). */
const NEVER = ['/faucet?address', '18610', '127.0.0.1:8791', 'devFixtureNames'];
/** The W4 fixture scans (crates/web/tests/w4_mainnet.rs FIXTURES), on the served wasm. */
const WASM_NEVER = ['devFixtureNames', 'web8fixtures', 'web::fixtures', 'digest5lines', 'digest::lines'];
/** The fixtures' wallet key (32 x 0x21, W4). */
const FIXTURE_KEY = Buffer.alloc(32, 0x21);
/** The quick wallet's exports: in the testnet release wasm (W4's positive control, on the served file). */
const WASM_QUICK = ['quickWalletNew', 'quickWalletImport', 'quickWalletSign'];

export function files(dir) {
  return readdirSync(dir).flatMap((f) => {
    const p = path.join(dir, f);
    return statSync(p).isDirectory() ? files(p) : [p];
  });
}

export function count(all, s) {
  return all.filter((f) => readFileSync(f).toString('latin1').includes(s)).length;
}

function main() {
  const control = process.argv[2] === '--control';
  const dist = control ? path.resolve(process.argv[3]) : path.join(WEB, 'dist');
  const all = files(dist);
  let bad = 0;
  const fail = (m) => {
    console.error(`check-bundle: ${m}`);
    bad++;
  };
  for (const s of DEV_MARKERS) {
    const n = count(all, s);
    if (control && n === 0)
      fail(`control: the development build has no ${JSON.stringify(s)}: the check would be vacuous`);
    if (!control && n > 0) fail(`${n} file(s) contain ${JSON.stringify(s)}`);
    if (control) console.log(`check-bundle control: ${s.padEnd(28)} ${n} file(s)`);
  }
  if (!control) {
    for (const s of NEVER) if (count(all, s) > 0) fail(`dist contains ${JSON.stringify(s)}`);
    const wasm = all.filter((f) => f.endsWith('.wasm'));
    if (wasm.length !== 1) fail(`expected one .wasm in dist, found ${wasm.length}`);
    else {
      const bytes = readFileSync(wasm[0]);
      const pkg = readFileSync(path.join(WEB, 'src', 'wasm', 'release', 'web_bg.wasm'));
      if (!bytes.equals(pkg)) fail('the served wasm is not src/wasm/release/web_bg.wasm');
      for (const s of WASM_NEVER)
        if (bytes.includes(Buffer.from(s))) fail(`the served wasm contains ${JSON.stringify(s)}`);
      if (bytes.includes(FIXTURE_KEY)) fail('the served wasm contains the fixtures wallet key bytes');
      for (const s of WASM_QUICK)
        if (!bytes.includes(Buffer.from(s))) fail(`positive control: the served wasm lacks ${JSON.stringify(s)}`);
      console.log(`check-bundle: served wasm ${path.basename(wasm[0])} = the release package, no fixture code or key`);
    }
  }
  const sizes = all.map((f) => `${path.relative(dist, f)} ${statSync(f).size}`).join('\n  ');
  if (!control) console.log(`check-bundle: ${all.length} files\n  ${sizes}`);
  if (bad) {
    console.error(`check-bundle: ${bad} problems`);
    process.exit(1);
  }
  console.log(
    control
      ? 'check-bundle control: every dev marker is in the development build'
      : 'check-bundle: no catalogue, faucet, hooks or simnet configuration in the production bundle',
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) main();
