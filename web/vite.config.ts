/// <reference types="vitest/config" />
// Vite config for the Kaspalionare page (front end F3). Commands: web/CLAUDE.md.
//
// - `@wasm/*` is the wasm-bindgen output of crates/web (scripts/wasm.mjs): `src/wasm/dev` (features
//   simnet,node: the private simnet dev run) or `src/wasm/release` (feature node: testnet-10 only).
//   A production build always uses the release package and refuses a non-testnet configuration.
// - Production builds carry a strict Content-Security-Policy (spec 6.6) in index.html.
import path from 'node:path';
import { defineConfig, loadEnv, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

const root = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'));

function originOf(url: string | undefined): string | null {
  if (!url) return null;
  try {
    return new URL(url).origin;
  } catch {
    return null;
  }
}

/** The production CSP (spec 6.6): no third-party scripts, the wasm allowed to compile. */
function csp(indexer: string | undefined): Plugin {
  const connect = ["'self'", originOf(indexer), 'https:', 'wss:'].filter(Boolean).join(' ');
  const policy = [
    "default-src 'none'",
    "script-src 'self' 'wasm-unsafe-eval'",
    "worker-src 'self'",
    "style-src 'self'",
    "font-src 'self'",
    "img-src 'self' data:",
    `connect-src ${connect}`,
    "manifest-src 'self'",
    "base-uri 'none'",
    "form-action 'none'",
  ].join('; ');
  return {
    name: 'kaspalionare-csp',
    apply: 'build',
    transformIndexHtml(html) {
      return html.replace('<!--csp-->', `<meta http-equiv="Content-Security-Policy" content="${policy}" />`);
    },
  };
}

export default defineConfig(({ command, mode }) => {
  const env = loadEnv(mode, root, 'VITE_');
  const build = command === 'build';
  const wasm = build ? 'release' : (env.VITE_WASM ?? process.env.VITE_WASM ?? 'dev');
  if (build) {
    const network = env.VITE_NETWORK ?? process.env.VITE_NETWORK ?? 'testnet-10';
    if (network !== 'testnet-10') throw new Error(`a production build runs on testnet-10 only, not ${network}`);
    if (env.VITE_FAUCET_URL || process.env.VITE_FAUCET_URL) throw new Error('a production build has no faucet');
  }
  if (wasm !== 'dev' && wasm !== 'release') throw new Error(`VITE_WASM must be dev or release, not ${wasm}`);
  return {
    plugins: [react(), tailwindcss(), csp(env.VITE_INDEXER_URL ?? process.env.VITE_INDEXER_URL)],
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
