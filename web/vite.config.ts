/// <reference types="vitest/config" />
// Vite config for the Kaspalionare page (front end F3). Commands: web/CLAUDE.md.
//
// - `@wasm/*` is the wasm-bindgen output of crates/web (scripts/wasm.mjs): `src/wasm/dev` (features
//   simnet,node: the private simnet dev run) or `src/wasm/release` (feature node: testnet-10 only).
//   A production build always uses the release package and refuses a non-testnet configuration.
// - The strict Content-Security-Policy (spec 6.6) and the other response headers are HTTP headers from
//   vercel.json, served by Vercel and by `vite preview` (siteHeaders); scripts/check-csp.mjs checks the page,
//   its worker and the wasm glue under them.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

const root = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'));

/**
 * The site's HTTP response headers (spec 6.6), from vercel.json (the one copy: Vercel serves them in
 * production, `vite preview` serves the same ones locally, scripts/check-csp.mjs checks the built page under
 * them). The CSP is a header, not a meta tag, so it also governs the worker script (a worker gets the policy
 * of its own response) and can set frame-ancestors (F2/F3 review S6).
 */
export function siteHeaders(): Record<string, string> {
  const v = JSON.parse(readFileSync(path.join(root, 'vercel.json'), 'utf8')) as {
    headers: { source: string; headers: { key: string; value: string }[] }[];
  };
  const all = v.headers.find((h) => h.source === '/(.*)');
  if (!all) throw new Error('vercel.json: no headers for /(.*)');
  return Object.fromEntries(all.headers.map((h) => [h.key, h.value]));
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
    plugins: [react(), tailwindcss()],
    resolve: {
      alias: {
        '@wasm': path.join(root, 'src', 'wasm', wasm),
        '@': path.join(root, 'src'),
      },
    },
    worker: { format: 'es' },
    build: { target: 'es2022', sourcemap: false },
    server: { port: 5173, strictPort: true, host: '127.0.0.1' },
    preview: { port: 4173, strictPort: true, host: '127.0.0.1', headers: siteHeaders() },
    test: {
      environment: 'jsdom',
      include: ['src/**/*.test.{ts,tsx}'],
      css: false,
    },
  };
});
