// F6 part C: the site's CSP connect-src, narrowed from "any secure host" (https: wss:) to the hosts the page
// talks to (spec 6.6):
//   - 'self' (the page's own files: the wasm, the figure model);
//   - the indexer API's origin (VITE_INDEXER_URL);
//   - the node: VITE_NODE_URL's origin when the build names a node, else the public resolver and its nodes.
// The resolver hosts are fixed by the wasm itself: rusty-kaspa v2.1.0 compiles its resolver list in
// (rpc/wrpc/client/Resolvers.toml: https://*.kaspa.stream, *.kaspa.red, *.kaspa.green, *.kaspa.blue; the
// page asks https://<name>.<domain>/v2/kaspa/testnet-10/tls/wrpc/borsh, rpc/wrpc/client/src/resolver.rs:126-143).
// The nodes a resolver hands out are not fixed in code: on 2026-10-27 every answer of the 16 resolvers
// named one of wss://muon-10.kaspa.blue, wss://vector-10.kaspa.green, wss://quark-10.kaspa.red, all under the
// same four domains (docs/launch/site-deploy.md, "CSP"). A node on another domain would be refused by the
// CSP (the page then shows the node as unavailable and the next resolver answer is tried on reload).
//
//   node scripts/csp.mjs           print the connect-src for web/.env.production (and the environment)
//   node scripts/csp.mjs --write   write it into vercel.json's site rule (the one copy of the headers)
//   node scripts/csp.mjs --check   exit non-zero when vercel.json's connect-src is not that one
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { WEB } from './lib.mjs';

/** The public resolver's domains (rusty-kaspa v2.1.0 Resolvers.toml), for its https lookups and wss nodes. */
export const RESOLVER_DOMAINS = ['kaspa.stream', 'kaspa.red', 'kaspa.green', 'kaspa.blue'];
export const SITE_RULE = '/((?!figures/).*)';

/** The production build's VITE_ variables: web/.env.production (as Vite reads it), the environment winning. */
export async function productionEnv() {
  const { loadEnv } = await import('vite');
  return { ...loadEnv('production', WEB, 'VITE_'), ...pickVite(process.env) };
}

function pickVite(env) {
  return Object.fromEntries(Object.entries(env).filter(([k, v]) => k.startsWith('VITE_') && v !== undefined));
}

function origin(url, schemes, what) {
  let u;
  try {
    u = new URL(url);
  } catch {
    throw new Error(`${what} ${JSON.stringify(url)} is not a URL`);
  }
  if (!schemes.includes(u.protocol)) throw new Error(`${what} ${url}: ${schemes.join(' or ')} only`);
  if (u.username || u.password) throw new Error(`${what} ${url}: no credentials in the URL`);
  return `${u.protocol}//${u.host}`;
}

/** The connect-src sources for `env` (throws when VITE_INDEXER_URL is missing or not https). */
export function connectSources(env) {
  if (!env.VITE_INDEXER_URL) throw new Error('VITE_INDEXER_URL is not set (web/.env.production)');
  const out = ["'self'", origin(env.VITE_INDEXER_URL, ['https:'], 'VITE_INDEXER_URL')];
  if (env.VITE_NODE_URL) out.push(origin(env.VITE_NODE_URL, ['wss:'], 'VITE_NODE_URL'));
  else for (const d of RESOLVER_DOMAINS) out.push(`https://*.${d}`, `wss://*.${d}`);
  return out;
}

export function readVercel() {
  return JSON.parse(readFileSync(path.join(WEB, 'vercel.json'), 'utf8'));
}

/** The site rule's CSP of a parsed vercel.json. */
export function siteCsp(v) {
  const rule = v.headers.find((h) => h.source === SITE_RULE);
  const h = rule?.headers.find((x) => x.key === 'Content-Security-Policy');
  if (!h) throw new Error(`vercel.json has no CSP for ${SITE_RULE}`);
  return h;
}

/** A CSP's directives as [name, sources[]]. */
export function directives(csp) {
  return csp
    .split(';')
    .map((d) => d.trim())
    .filter(Boolean)
    .map((d) => {
      const [name, ...src] = d.split(/\s+/);
      return [name, src];
    });
}

export function withConnectSrc(csp, sources) {
  return directives(csp)
    .map(([n, s]) => `${n} ${(n === 'connect-src' ? sources : s).join(' ')}`.trim())
    .join('; ');
}

/** Problems of a connect-src: a scheme-only or wildcard-host source allows any host. */
export function broadSources(sources) {
  return sources.filter((s) => /^[a-z][a-z0-9+.-]*:$/i.test(s) || s === '*' || /^[a-z]+:\/\/\*$/i.test(s));
}

async function main() {
  const env = await productionEnv();
  const want = connectSources(env);
  const v = readVercel();
  const h = siteCsp(v);
  const have = directives(h.value).find(([n]) => n === 'connect-src')?.[1] ?? [];
  if (process.argv.includes('--write')) {
    h.value = withConnectSrc(h.value, want);
    writeFileSync(path.join(WEB, 'vercel.json'), `${JSON.stringify(v, null, 2)}\n`);
    console.log(`csp: vercel.json connect-src = ${want.join(' ')}`);
    return;
  }
  if (process.argv.includes('--check')) {
    const broad = broadSources(have);
    if (broad.length) {
      console.error(`csp: vercel.json connect-src allows any host: ${broad.join(' ')}`);
      process.exit(1);
    }
    if (have.join(' ') !== want.join(' ')) {
      console.error(
        `csp: vercel.json connect-src is\n  ${have.join(' ')}\nbut web/.env.production needs\n  ${want.join(' ')}\nrun: node scripts/csp.mjs --write`,
      );
      process.exit(1);
    }
    console.log(`csp: vercel.json connect-src matches web/.env.production (${want.length} sources)`);
    return;
  }
  console.log(want.join(' '));
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch((e) => {
    console.error(`csp: ${e.message}`);
    process.exit(1);
  });
}
