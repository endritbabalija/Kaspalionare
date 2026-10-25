// The one-command dev run (spec 1.1 DX): `npm run dev:simnet` in web/.
//
// 1. installs web/ dependencies if missing (`npm ci`, in web/ only);
// 2. builds the dev wasm package (crates/web, features simnet,node -> src/wasm/dev);
// 3. builds `simnet` (release; RocksDB, about 10 minutes the first time) and starts `simnet dev`: a fresh
//    private node with no peers (D125), round 1 drawn, round 2 closing, round 3 open (the same scenario
//    every run), the M8 indexer, the indexer API on 127.0.0.1:8790, the node's Borsh wRPC on
//    127.0.0.1:18610 and the dev faucet on 127.0.0.1:8791;
// 4. waits for that run's crates/target/simnet-dev/current.json (written once its self-check passed);
// 5. starts the Vite dev server on http://127.0.0.1:5173/ pointed at it.
// Ctrl-C stops both. Flags: --skip-build (use the binaries and package as they are), --block-ms <n>.
import { existsSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { CRATES, WEB, isWin, npm, run, start } from './lib.mjs';

const args = process.argv.slice(2);
const skipBuild = args.includes('--skip-build');
const blockMs = args.includes('--block-ms') ? args[args.indexOf('--block-ms') + 1] : '200';
const PORTS = { api: '127.0.0.1:8790', node: '18610', faucet: '127.0.0.1:8791', web: 5173 };

if (!existsSync(path.join(WEB, 'node_modules', 'vite'))) npm(['ci', '--no-audit', '--no-fund']);
if (!skipBuild) {
  run(process.execPath, [path.join(WEB, 'scripts', 'wasm.mjs'), 'dev'], { cwd: WEB });
  run('cargo', ['build', '-j', process.env.CARGO_JOBS ?? '2', '-p', 'simnet', '--release'], { cwd: CRATES });
}
const simnetBin = path.join(CRATES, 'target', 'release', isWin ? 'simnet.exe' : 'simnet');
if (!existsSync(simnetBin)) throw new Error(`${simnetBin} is missing: run without --skip-build`);
if (!existsSync(path.join(WEB, 'src', 'wasm', 'dev', 'web.d.ts')))
  throw new Error('src/wasm/dev is missing: run without --skip-build');

const current = path.join(CRATES, 'target', 'simnet-dev', 'current.json');
const t0 = Date.now();
const children = [];
const stop = (code) => {
  for (const c of children) {
    try {
      c.kill();
    } catch {
      // gone
    }
  }
  process.exit(code);
};
process.on('SIGINT', () => stop(0));
process.on('SIGTERM', () => stop(0));

const sim = start(
  'simnet',
  simnetBin,
  ['dev', '--api-bind', PORTS.api, '--node-port', PORTS.node, '--faucet-bind', PORTS.faucet, '--block-ms', blockMs],
  { cwd: CRATES },
);
children.push(sim);
sim.on('exit', (code) => {
  console.error(`[simnet] exited with ${code}`);
  stop(code ?? 1);
});

async function waitForRun() {
  const deadline = Date.now() + 15 * 60_000;
  while (Date.now() < deadline) {
    if (existsSync(current) && statSync(current).mtimeMs >= t0) {
      try {
        const info = JSON.parse(readFileSync(current, 'utf8'));
        if (Number(info.startedMs) >= t0 - 5_000) return info;
      } catch {
        // being written
      }
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error('simnet dev did not become ready in 15 minutes');
}

const info = await waitForRun();
const env = {
  ...process.env,
  VITE_NETWORK: 'simnet',
  VITE_NODE_URL: info.wrpcBorsh,
  VITE_INDEXER_URL: info.api,
  VITE_COVENANT_ID: info.covenantId,
  VITE_FAUCET_URL: info.faucet,
  // The dev run's stand-in for the site owner's referral address (spec 6.5, 7): a kaspasim: address.
  ...(info.referralAddress ? { VITE_REFERRAL_ADDRESS: info.referralAddress } : {}),
  VITE_WASM: 'dev',
};
const vite = start(
  'vite',
  process.execPath,
  [path.join(WEB, 'node_modules', 'vite', 'bin', 'vite.js'), '--port', String(PORTS.web), '--strictPort'],
  {
    cwd: WEB,
    env,
  },
);
children.push(vite);
vite.on('exit', (code) => stop(code ?? 1));
console.log(
  `\ndev:simnet ready: http://127.0.0.1:${PORTS.web}/  (indexer ${info.api}, node ${info.wrpcBorsh}, faucet ${info.faucet}, instance ${info.covenantId})\n`,
);
