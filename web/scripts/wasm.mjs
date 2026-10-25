// Build crates/web for the browser and write the wasm-bindgen package the page imports as `@wasm/*`.
//   node scripts/wasm.mjs dev      features simnet,node -> src/wasm/dev      (the private simnet dev run)
//   node scripts/wasm.mjs release  feature node         -> src/wasm/release  (testnet-10 only; + wasm-opt -Oz)
//   node scripts/wasm.mjs mainnet  features mainnet,node -> src/wasm/mainnet  (TEST-ONLY: what check-mainnet.mjs
//                                  builds the page with and scans; it refuses to load, W4)
// The release build is the documented wasm build command (CLAUDE.md) plus `--features node`, into the
// default target dir, so W4 (`cargo test -p web --test w4_mainnet`) checks this very artifact. The dev build
// uses its own target dir (crates/target/web-dev) and never overwrites it.
import { existsSync, mkdirSync, rmSync, statSync } from 'node:fs';
import path from 'node:path';
import { CRATES, WEB, isWin, run, wasmEnv } from './lib.mjs';

const variant = process.argv[2];
if (variant !== 'dev' && variant !== 'release' && variant !== 'mainnet') {
  console.error('usage: node scripts/wasm.mjs <dev|release|mainnet>');
  process.exit(2);
}
const features = { dev: 'simnet,node', release: 'node', mainnet: 'mainnet,node' }[variant];
const targetDir = {
  dev: path.join(CRATES, 'target', 'web-dev'),
  release: path.join(CRATES, 'target'),
  mainnet: path.join(CRATES, 'target', 'web-mainnet'),
}[variant];
const out = path.join(WEB, 'src', 'wasm', variant);
const env = wasmEnv();

run(
  'cargo',
  [
    'build',
    '-j',
    process.env.CARGO_JOBS ?? '2',
    '-p',
    'web',
    '--target',
    'wasm32-unknown-unknown',
    '--release',
    '--features',
    features,
    ...(variant !== 'release' ? ['--target-dir', targetDir] : []),
  ],
  { cwd: CRATES, env },
);
const wasm = path.join(targetDir, 'wasm32-unknown-unknown', 'release', 'web.wasm');
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
run('wasm-bindgen', ['--target', 'web', '--out-dir', out, wasm], { cwd: CRATES });
if (variant === 'release') {
  const bin = path.join(WEB, 'node_modules', 'binaryen', 'bin', 'wasm-opt');
  const f = path.join(out, 'web_bg.wasm');
  if (existsSync(bin)) {
    run(
      process.execPath,
      [
        bin,
        '-Oz',
        '--enable-bulk-memory',
        '--enable-nontrapping-float-to-int',
        '--enable-sign-ext',
        '--enable-mutable-globals',
        f,
        '-o',
        f,
      ],
      { cwd: WEB },
    );
  } else {
    console.warn('wasm-opt (binaryen in web/node_modules) not found: run `npm ci` in web/; the package is unoptimised');
  }
}
console.log(
  `wasm ${variant}: ${path.relative(WEB, out)} (${statSync(path.join(out, 'web_bg.wasm')).size} B)${isWin ? '' : ''}`,
);
