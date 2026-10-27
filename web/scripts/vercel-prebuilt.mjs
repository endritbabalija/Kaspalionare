// F6 part C: vercel.json's installCommand and buildCommand. The site is built and checked on this machine
// (npm run build, npm run check:mainnet, npm run check:csp, npm run check:deploy: docs/launch/site-deploy.md) and
// deployed prebuilt (vercel build, then vercel deploy --prebuilt), so these commands install and build nothing:
// they only make sure a checked web/dist is there for `vercel build` to copy into .vercel/output/static. On a
// machine without one (Vercel's own builders, if a git repository were ever connected) they fail, so no
// unchecked build can be deployed.
import { existsSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { WEB } from './lib.mjs';

const step = process.argv[2] ?? 'build';
const dist = path.join(WEB, 'dist');
const wasm =
  existsSync(path.join(dist, 'assets')) && readdirSync(path.join(dist, 'assets')).some((f) => f.endsWith('.wasm'));
if (!existsSync(path.join(dist, 'index.html')) || !wasm) {
  console.error(
    `vercel-prebuilt (${step}): no built web/dist here. This site is built and checked locally and deployed with ` +
      '`vercel build` + `vercel deploy --prebuilt` (docs/launch/site-deploy.md); it is never built on Vercel.',
  );
  process.exit(1);
}
console.log(
  `vercel-prebuilt (${step}): web/dist is the prebuilt output; nothing to ${step === 'install' ? 'install' : 'build'}`,
);
