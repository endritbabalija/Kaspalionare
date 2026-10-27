// F6 part C: the checks on the deploy artifact itself, the files Vercel will serve (F1/F5 reviews: "W4 on the
// deployed artifact"). Run after `npm run build` (and again after `vercel build`, on .vercel/output):
//
//   node scripts/check-deploy.mjs            check web/dist
//   node scripts/check-deploy.mjs --vercel   also check .vercel/output (vercel build's output): its static files
//                                            are dist's byte for byte and its config.json carries vercel.json's
//                                            headers for both rules
//
// What it checks (exits non-zero on any failure):
//   1. the testnet-10 configuration is complete and deployable (web/.env.production and the environment, as the
//      build read it): an https indexer URL that is not a placeholder, the covenant ID and the bundle hash (64 hex),
//      a kaspatest: referral address, no faucet, testnet-10 only, https explorer, wss node if any, no kaspa: value;
//   2. dist was built with that configuration (the indexer URL and covenant ID are in the shipped JS);
//   3. vercel.json's connect-src is exactly the one this configuration needs (scripts/csp.mjs), nothing broader;
//   4. the bundle scans (scripts/check-bundle.mjs on dist: no dev code, the served wasm is the release package,
//      no fixture code or key);
//   5. W4 on the served wasm: dist's .wasm (after wasm-opt), loaded in Node with the release glue, starts on
//      testnet-10 and refuses start("simnet"), start("mainnet"), a kaspasim: address and a kaspasim: change
//      address (address_network); the quick wallet is there and the fixtures' export is not;
//   6. with --vercel: .vercel/output/static equals dist file for file, and .vercel/output/config.json has a route
//      with each vercel.json header rule's headers.
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { WEB, run } from './lib.mjs';
import { files } from './check-bundle.mjs';
import { broadSources, connectSources, directives, productionEnv, readVercel, siteCsp } from './csp.mjs';

let bad = 0;
const check = (ok, what) => {
  console.log(`check-deploy: ${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) bad++;
};
const HEX64 = /^[0-9a-f]{64}$/;

/** 1. The configuration's problems (an empty list: deployable). */
export function configProblems(env) {
  const p = [];
  const https = (v) => {
    try {
      const u = new URL(v);
      return u.protocol === 'https:' && !u.username && !u.password;
    } catch {
      return false;
    }
  };
  const placeholder = (v) =>
    /\.invalid(?::|\/|$)|\.example(?::|\/|$)|localhost|127\.0\.0\.1/.test(new URL(v).host + '/');
  if ((env.VITE_NETWORK ?? 'testnet-10') !== 'testnet-10') p.push(`VITE_NETWORK=${env.VITE_NETWORK}`);
  if (env.VITE_FAUCET_URL) p.push('VITE_FAUCET_URL is set (the dev faucet; never in a deploy)');
  if (!env.VITE_INDEXER_URL || !https(env.VITE_INDEXER_URL)) p.push('VITE_INDEXER_URL is not an https:// URL');
  else if (placeholder(env.VITE_INDEXER_URL)) p.push(`VITE_INDEXER_URL ${env.VITE_INDEXER_URL} is a placeholder`);
  if (!HEX64.test((env.VITE_COVENANT_ID ?? '').toLowerCase())) p.push('VITE_COVENANT_ID is not 64 hex digits');
  if (!HEX64.test((env.VITE_BUNDLE_HASH ?? '').toLowerCase()))
    p.push('VITE_BUNDLE_HASH is not 64 hex digits (pin the launch bundle)');
  if (!(env.VITE_REFERRAL_ADDRESS ?? '').startsWith('kaspatest:'))
    p.push('VITE_REFERRAL_ADDRESS is not a kaspatest: address');
  if (env.VITE_NODE_URL && !env.VITE_NODE_URL.startsWith('wss://')) p.push('VITE_NODE_URL is not wss://');
  if (env.VITE_EXPLORER_URL && !https(env.VITE_EXPLORER_URL)) p.push('VITE_EXPLORER_URL is not https://');
  if (env.VITE_FAUCET_LINK && !https(env.VITE_FAUCET_LINK)) p.push('VITE_FAUCET_LINK is not https://');
  for (const [k, v] of Object.entries(env))
    if (/\bkaspa:/.test(String(v))) p.push(`${k} holds a kaspa: (mainnet) value`);
  return p;
}

/** 5. W4 on a served wasm file, with the release glue, in this Node process. */
async function w4(wasmFile) {
  const glue = path.join(WEB, 'src', 'wasm', 'release', 'web.js');
  const m = await import(pathToFileURL(glue).href);
  m.initSync({ module: readFileSync(wasmFile) });
  const code = (r) => r?.error?.code;
  const sim = 'kaspasim:qrz9k4rhzxye8lc0fc7a792sm5kxjlwrsjtfh3x7hys0258220v7we0eu85zz';
  const test = 'kaspatest:qq3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zy09kj3t9w';
  const before = m.start('simnet');
  check(code(before) === 'address_network', `served wasm: start("simnet") refused (${code(before)})`);
  const t = m.start('testnet-10');
  check(t?.ok?.network === 'testnet-10', 'served wasm: starts on testnet-10');
  check(code(m.start('simnet')) === 'address_network', 'served wasm: start("simnet") refused after testnet-10');
  check(code(m.start('mainnet')) === 'address_network', 'served wasm: start("mainnet") refused');
  check(code(m.scriptOf(sim)) === 'address_network', 'served wasm: a kaspasim: address refused');
  check(Boolean(m.scriptOf(test)?.ok), 'served wasm: a kaspatest: address accepted (positive control)');
  const b = m.buildRequestDeposit({
    covenantId: '00'.repeat(32),
    vault: {
      utxo: {
        transactionId: '00'.repeat(32),
        index: 0,
        amount: '1',
        scriptPublicKey: '0000',
        blockDaaScore: '1',
        isCoinbase: false,
      },
      state: '',
    },
    amount: '1',
    owner: test,
    funds: { coins: [], change: sim, feeRate: 100 },
  });
  check(code(b) === 'address_network', `served wasm: a kaspasim: change address refused (${code(b)})`);
  check(typeof m.quickWalletNew === 'function', 'served wasm: the quick wallet is there (testnet build)');
  check(typeof m.devFixtureNames === 'undefined', 'served wasm: no fixtures export');
}

function hashes(dir) {
  return new Map(
    files(dir).map((f) => [
      path.relative(dir, f).split(path.sep).join('/'),
      createHash('sha256').update(readFileSync(f)).digest('hex'),
    ]),
  );
}

async function main() {
  const dist = path.join(WEB, 'dist');
  if (!existsSync(path.join(dist, 'index.html'))) {
    console.error('check-deploy: no web/dist: run npm run build first');
    process.exit(1);
  }
  const env = await productionEnv();
  // 1.
  const problems = configProblems(env);
  check(
    problems.length === 0,
    `testnet-10 configuration deployable${problems.length ? `: ${problems.join('; ')}` : ''}`,
  );
  // 2.
  const js = files(dist).filter((f) => f.endsWith('.js'));
  const has = (s) => Boolean(s) && js.some((f) => readFileSync(f, 'latin1').includes(s));
  check(has(env.VITE_INDEXER_URL), 'dist was built with this VITE_INDEXER_URL');
  check(
    has((env.VITE_COVENANT_ID ?? '').toLowerCase()) || has(env.VITE_COVENANT_ID),
    'dist was built with this VITE_COVENANT_ID',
  );
  // 3.
  try {
    const want = connectSources(env);
    const have = directives(siteCsp(readVercel()).value).find(([n]) => n === 'connect-src')?.[1] ?? [];
    check(
      broadSources(have).length === 0,
      `vercel.json connect-src allows no scheme-wide source (${have.length} sources)`,
    );
    check(have.join(' ') === want.join(' '), `vercel.json connect-src = ${want.join(' ')}`);
  } catch (e) {
    check(false, `connect-src: ${e.message}`);
  }
  // 4.
  try {
    run(process.execPath, [path.join(WEB, 'scripts', 'check-bundle.mjs')], { cwd: WEB });
    check(true, 'check-bundle on dist');
  } catch {
    check(false, 'check-bundle on dist');
  }
  // 5.
  const wasm = files(dist).filter((f) => f.endsWith('.wasm'));
  if (wasm.length !== 1) check(false, `one .wasm in dist (found ${wasm.length})`);
  else {
    const pkg = readFileSync(path.join(WEB, 'src', 'wasm', 'release', 'web_bg.wasm'));
    check(
      readFileSync(wasm[0]).equals(pkg),
      `${path.relative(dist, wasm[0])} is the release package (its glue applies)`,
    );
    await w4(wasm[0]);
    // Negative control: the dev package (simnet,node), when built, starts on the simnet, so the refusals above
    // come from the release build's code, not from a call that always fails.
    const devGlue = path.join(WEB, 'src', 'wasm', 'dev', 'web.js');
    if (existsSync(devGlue)) {
      const d = await import(pathToFileURL(devGlue).href);
      d.initSync({ module: readFileSync(path.join(WEB, 'src', 'wasm', 'dev', 'web_bg.wasm')) });
      check(d.start('simnet')?.ok?.network === 'simnet', 'control: the dev package starts on the simnet');
    } else console.log('check-deploy: (no src/wasm/dev: the dev control was not run)');
  }
  // 6.
  if (process.argv.includes('--vercel')) {
    const out = path.join(WEB, '.vercel', 'output');
    const stat = path.join(out, 'static');
    if (!existsSync(stat)) check(false, '.vercel/output/static exists (run vercel build)');
    else {
      const a = hashes(dist);
      const b = hashes(stat);
      const diff = [...new Set([...a.keys(), ...b.keys()])].filter((k) => a.get(k) !== b.get(k));
      check(
        diff.length === 0,
        `.vercel/output/static = dist (${a.size} files)${diff.length ? `; differ: ${diff.slice(0, 5).join(', ')}` : ''}`,
      );
      const cfg = JSON.parse(readFileSync(path.join(out, 'config.json'), 'utf8'));
      const routes = cfg.routes ?? [];
      const lower = (h) => Object.fromEntries(Object.entries(h ?? {}).map(([k, v]) => [k.toLowerCase(), v]));
      for (const rule of readVercel().headers) {
        const want = lower(Object.fromEntries(rule.headers.map((h) => [h.key, h.value])));
        const found = routes.some((r) => {
          const got = lower(r.headers);
          return Object.entries(want).every(([k, v]) => got[k] === v);
        });
        check(found, `.vercel/output/config.json serves the headers of vercel.json rule ${rule.source}`);
      }
    }
  }
  if (bad) {
    console.error(`check-deploy: ${bad} problem(s); do not deploy`);
    process.exit(1);
  }
  console.log('check-deploy: the deploy artifact passed every check');
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch((e) => {
    console.error(`check-deploy: ${e.stack ?? e}`);
    process.exit(1);
  });
}
