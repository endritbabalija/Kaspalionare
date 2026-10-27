/// <reference types="vitest/config" />
// Vite config for the Kaspalionare page (front end F3). Commands: web/CLAUDE.md.
//
// - `@wasm/*` is the wasm-bindgen output of crates/web (scripts/wasm.mjs): `src/wasm/dev` (features
//   simnet,node: the private simnet dev run) or `src/wasm/release` (feature node: testnet-10 only).
//   A production build always uses the release package and refuses a non-testnet configuration.
// - The strict Content-Security-Policy (spec 6.6) and the other response headers are HTTP headers from
//   vercel.json, served by Vercel and by `vite preview` (vercelHeaders); scripts/check-csp.mjs checks the page,
//   its worker and the wasm glue under them.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { defineConfig, loadEnv, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

const root = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'));

/**
 * The site's HTTP response headers (spec 6.6), from vercel.json (the one copy: Vercel serves them in
 * production, `vite preview` serves the same ones locally, scripts/check-csp.mjs checks the built page under
 * them). The CSP is a header, not a meta tag, so it also governs the worker script (a worker gets the policy
 * of its own response) and can set frame-ancestors (F2/F3 review S6). Two rules (F6 part B): the site
 * ("/((?!figures/).*)", frame-ancestors 'none') and the framed figures ("/figures/(.*)", their own CSP by hash,
 * frame-ancestors 'self'). Each source is a regular expression over the path, as Vercel reads it; the rules do
 * not overlap (src/screens/how/figure.test.ts), so the order never matters.
 */
export function headerRules(): { match: RegExp; headers: Record<string, string> }[] {
  const v = JSON.parse(readFileSync(path.join(root, 'vercel.json'), 'utf8')) as {
    headers: { source: string; headers: { key: string; value: string }[] }[];
  };
  return v.headers.map((r) => ({
    match: new RegExp(`^${r.source}$`),
    headers: Object.fromEntries(r.headers.map((h) => [h.key, h.value])),
  }));
}

/** The headers vercel.json gives a request path (every matching rule, in order). */
export function headersFor(pathname: string, rules = headerRules()): Record<string, string> {
  return Object.assign({}, ...rules.filter((r) => r.match.test(pathname)).map((r) => r.headers));
}

/** `vite preview` serves vercel.json's headers per path, as Vercel does. */
function vercelHeaders(): Plugin {
  return {
    name: 'kaspalionare-vercel-headers',
    configurePreviewServer(server) {
      const rules = headerRules();
      server.middlewares.use((req, res, next) => {
        const pathname = new URL(req.url ?? '/', 'http://localhost').pathname;
        for (const [k, v] of Object.entries(headersFor(pathname, rules))) res.setHeader(k, v);
        next();
      });
    },
  };
}

export default defineConfig(({ command, mode }) => {
  const env = loadEnv(mode, root, 'VITE_');
  const build = command === 'build';
  // TEST-ONLY: check-mainnet.mjs builds the page with the mainnet wasm package to scan what such a build ships.
  const mainnetCheck = (env.VITE_MAINNET_CHECK ?? process.env.VITE_MAINNET_CHECK) === '1';
  const wasm = build ? (mainnetCheck ? 'mainnet' : 'release') : (env.VITE_WASM ?? process.env.VITE_WASM ?? 'dev');
  if (build) {
    const network = env.VITE_NETWORK ?? process.env.VITE_NETWORK ?? 'testnet-10';
    if (network !== 'testnet-10') throw new Error(`a production build runs on testnet-10 only, not ${network}`);
    if (env.VITE_FAUCET_URL || process.env.VITE_FAUCET_URL) throw new Error('a production build has no faucet');
  }
  if (wasm !== 'dev' && wasm !== 'release' && !(build && mainnetCheck && wasm === 'mainnet'))
    throw new Error(`VITE_WASM must be dev or release, not ${wasm}`);
  return {
    plugins: [react(), tailwindcss(), vercelHeaders()],
    resolve: {
      alias: {
        '@wasm': path.join(root, 'src', 'wasm', wasm),
        '@': path.join(root, 'src'),
      },
    },
    worker: { format: 'es' },
    build: { target: 'es2022', sourcemap: false },
    server: { port: 5173, strictPort: true, host: '127.0.0.1' },
    preview: { port: 4173, strictPort: true, host: '127.0.0.1' },
    test: {
      environment: 'jsdom',
      include: ['src/**/*.test.{ts,tsx}'],
      css: false,
    },
  };
});
