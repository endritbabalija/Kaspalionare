// Shared helpers of the web/ scripts. Every command runs with an explicit working directory:
// cargo in crates/, npm and node tools in web/ (never the home folder: see web/CLAUDE.md, npm safety).
import { spawn, spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const WEB = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const REPO = path.resolve(WEB, '..');
export const CRATES = path.join(REPO, 'crates');
export const isWin = process.platform === 'win32';

/** The env for wasm32 builds: a wasm-capable clang and ar for secp256k1-sys (CLAUDE.md "wasm build"). */
export function wasmEnv() {
  const env = { ...process.env };
  if (!env.CC_wasm32_unknown_unknown && isWin && existsSync('C:/Program Files/LLVM/bin/clang.exe')) {
    env.CC_wasm32_unknown_unknown = 'C:/Program Files/LLVM/bin/clang.exe';
    env.AR_wasm32_unknown_unknown = 'C:/Program Files/LLVM/bin/llvm-ar.exe';
  }
  return env;
}

/** Run a command to completion (inherits stdio); throws on a non-zero exit. */
export function run(cmd, args, opts) {
  if (!opts?.cwd) throw new Error(`run ${cmd}: an explicit cwd is required`);
  console.log(`> (${path.relative(REPO, opts.cwd) || '.'}) ${cmd} ${args.join(' ')}`);
  const r = spawnSync(cmd, args, { stdio: 'inherit', shell: false, ...opts });
  if (r.error) throw r.error;
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')} exited with ${r.status}`);
}

/** npm, always in web/ (npm walks up to the first package.json; web/ has its own). */
export function npm(args) {
  if (!existsSync(path.join(WEB, 'package.json'))) throw new Error('web/package.json is missing; refusing to run npm');
  // npm is a .cmd shim on Windows: run it through the shell there.
  run(isWin ? 'npm.cmd' : 'npm', args, { cwd: WEB, shell: isWin });
}

/** Start a long-running child with prefixed output. */
export function start(name, cmd, args, opts) {
  if (!opts?.cwd) throw new Error(`start ${cmd}: an explicit cwd is required`);
  console.log(`> [${name}] (${path.relative(REPO, opts.cwd) || '.'}) ${cmd} ${args.join(' ')}`);
  const p = spawn(cmd, args, { stdio: ['ignore', 'pipe', 'pipe'], shell: false, ...opts });
  const pre = (s) =>
    s
      .toString()
      .split(/\r?\n/)
      .filter((l) => l.length)
      .forEach((l) => console.log(`[${name}] ${l}`));
  p.stdout.on('data', pre);
  p.stderr.on('data', pre);
  return p;
}
