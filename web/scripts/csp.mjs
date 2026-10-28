// F6 part C: the site's CSP connect-src, narrowed from "any secure host" (https: wss:) to the hosts the page
// talks to (spec 6.6):
//   - 'self' (the page's own files: the wasm, the figure model);
//   - the indexer API's origin (VITE_INDEXER_URL), each listed origin when it lists several (F6 part E); a
//     same-origin /api/v1/ entry (the Vercel proxy, INDEXER_PROXY_TARGET) is covered by 'self';
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
//   node scripts/csp.mjs --write   write it into vercel.json's site rule (the one copy of the headers), and
//                                  vercel.json's rewrites (the /api/v1/ proxy, or none)
//   node scripts/csp.mjs --check   exit non-zero when vercel.json's connect-src or rewrites are not those
//
// F6 part E, the Vercel proxy (docs/launch/site-deploy.md "The indexer through the site's own address"):
// INDEXER_PROXY_TARGET (a build-time setting in web/.env.production, never in the page) names the indexer
// server's origin, for example http://203.0.113.5 or https://api.example.org. With it, vercel.json gets one
// rewrite, /api/v1/:path* -> <target>/v1/:path*, and VITE_INDEXER_URL may list /api/v1/. Without it, no rewrite.
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { WEB } from './lib.mjs';

/** The public resolver's domains (rusty-kaspa v2.1.0 Resolvers.toml), for its https lookups and wss nodes. */
export const RESOLVER_DOMAINS = ['kaspa.stream', 'kaspa.red', 'kaspa.green', 'kaspa.blue'];
export const SITE_RULE = '/((?!figures/).*)';
/** The same-origin path the Vercel proxy serves the indexer under (F6 part E). */
export const PROXY_PATH = '/api/v1/';
export const PROXY_SOURCE = '/api/v1/:path*';

/** The production build's VITE_ variables: web/.env.production (as Vite reads it), the environment winning. */
export async function productionEnv() {
  const { loadEnv } = await import('vite');
  return { ...loadEnv('production', WEB, ['VITE_', 'INDEXER_PROXY_']), ...pickVite(process.env) };
}

function pickVite(env) {
  return Object.fromEntries(
    Object.entries(env).filter(
      ([k, v]) => (k.startsWith('VITE_') || k.startsWith('INDEXER_PROXY_')) && v !== undefined,
    ),
  );
}

/** VITE_INDEXER_URL's entries (comma-separated, trimmed, a trailing slash added; as src/net/indexers.ts reads it). */
export function indexerUrls(env) {
  return (env.VITE_INDEXER_URL ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean)
    .map((u) => (u.endsWith('/') ? u : `${u}/`));
}

/** INDEXER_PROXY_TARGET's origin (http or https, no path, no credentials), or undefined when unset. */
export function proxyTarget(env) {
  const t = env.INDEXER_PROXY_TARGET?.trim();
  if (!t) return undefined;
  const o = origin(t, ['https:', 'http:'], 'INDEXER_PROXY_TARGET');
  if (new URL(t).pathname !== '/') throw new Error(`INDEXER_PROXY_TARGET ${t}: an origin only (the rewrite adds /v1/)`);
  return o;
}

/** vercel.json's rewrites for `env`: the /api/v1/ proxy, or none. */
export function rewritesFor(env) {
  const t = proxyTarget(env);
  return t ? [{ source: PROXY_SOURCE, destination: `${t}/v1/:path*` }] : [];
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

/**
 * The connect-src sources for `env` (throws when VITE_INDEXER_URL is missing, an entry is not https, or it lists
 * /api/v1/ without INDEXER_PROXY_TARGET).
 */
export function connectSources(env) {
  const urls = indexerUrls(env);
  if (!urls.length) throw new Error('VITE_INDEXER_URL is not set (web/.env.production)');
  const out = ["'self'"];
  for (const u of urls) {
    if (u.startsWith('/') && !u.startsWith('//')) {
      if (u !== PROXY_PATH) throw new Error(`VITE_INDEXER_URL ${u}: the one same-origin entry is ${PROXY_PATH}`);
      if (!proxyTarget(env))
        throw new Error(`VITE_INDEXER_URL lists ${PROXY_PATH} but INDEXER_PROXY_TARGET is not set`);
      continue;
    }
    const o = origin(u, ['https:'], 'VITE_INDEXER_URL');
    if (!out.includes(o)) out.push(o);
  }
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
  const rewrites = rewritesFor(env);
  if (process.argv.includes('--write')) {
    h.value = withConnectSrc(h.value, want);
    if (rewrites.length) v.rewrites = rewrites;
    else delete v.rewrites;
    writeFileSync(path.join(WEB, 'vercel.json'), `${JSON.stringify(v, null, 2)}\n`);
    console.log(`csp: vercel.json connect-src = ${want.join(' ')}`);
    console.log(`csp: vercel.json rewrites = ${rewrites.length ? JSON.stringify(rewrites) : 'none'}`);
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
    if (JSON.stringify(v.rewrites ?? []) !== JSON.stringify(rewrites)) {
      console.error(
        `csp: vercel.json rewrites are ${JSON.stringify(v.rewrites ?? [])}, want ${JSON.stringify(rewrites)}; run: node scripts/csp.mjs --write`,
      );
      process.exit(1);
    }
    console.log(`csp: vercel.json connect-src and rewrites match web/.env.production (${want.length} sources)`);
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
