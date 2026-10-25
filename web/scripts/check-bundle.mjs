// After `vite build`: the production bundle (web/dist) must not contain the dev catalogue, the dev faucet or
// the simnet configuration (spec 1.1: the catalogue in development builds only; the faucet never in a
// production build). Run by `npm run build`; exits non-zero on any find.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { WEB } from './lib.mjs';

const dist = path.join(WEB, 'dist');
const FORBIDDEN = [
  'KASPALIONARE_DEV_CATALOGUE',
  'KASPALIONARE_DEV_FAUCET',
  'catalogue-dark',
  '/faucet?address',
  '18610',
  '127.0.0.1:8791',
  'devFixtureNames',
  'KASPALIONARE_DEV_HOOKS',
  '__klDev',
  'Dev faucet',
];

function files(dir) {
  return readdirSync(dir).flatMap((f) => {
    const p = path.join(dir, f);
    return statSync(p).isDirectory() ? files(p) : [p];
  });
}

const all = files(dist);
let bad = 0;
for (const f of all) {
  const text = readFileSync(f).toString('latin1');
  for (const s of FORBIDDEN) {
    if (text.includes(s)) {
      console.error(`check-bundle: ${path.relative(WEB, f)} contains ${JSON.stringify(s)}`);
      bad++;
    }
  }
}
const wasm = all.filter((f) => f.endsWith('.wasm'));
if (wasm.length !== 1) {
  console.error(`check-bundle: expected one .wasm in dist, found ${wasm.length}`);
  bad++;
}
const sizes = all.map((f) => `${path.relative(dist, f)} ${statSync(f).size}`).join('\n  ');
console.log(`check-bundle: ${all.length} files\n  ${sizes}`);
if (bad) {
  console.error(`check-bundle: ${bad} forbidden finds`);
  process.exit(1);
}
console.log('check-bundle: no catalogue, faucet or simnet configuration in the production bundle');
