/* global document, self */
// F2/F3 review S6: the production page under its HTTP headers (vercel.json, the copy Vercel serves), checked in
// a real browser before any deploy. It builds the page (release wasm, testnet-10) into dist-csp-check/ with a
// wss:// node URL nobody listens on, serves it with `vite preview` (which applies vercel.json's headers), opens it in
// Playwright's Chromium and checks:
//   - the page and the worker script are served with the CSP, frame-ancestors 'none' and X-Frame-Options DENY;
//   - the CSP is enforced in the page (an inline script is refused) and in the worker (its own response's
//     policy: a ws:// socket is refused there);
//   - the wasm glue runs under it: the worker instantiates the wasm (needs only 'wasm-unsafe-eval'), runs
//     errorTable and start, and reaches the node client, which fails as a node error (node_unavailable), not
//     as a stopped worker; no CSP report other than the two probes' own;
//   - a page on another origin cannot frame it;
//   - F6 part C: connect-src is narrowed to the indexer and the resolver's domains (scripts/csp.mjs): no
//     scheme-wide source; in the page and in the worker an https fetch and a wss socket to an unlisted host are
//     refused (connect-src), while a fetch to a listed resolver host and to the indexer origin is not refused by
//     the CSP (the resolver request is a real read-only GET to the public testnet-10 resolver);
//   - F6 part B: How it works frames the draw-cabinet figure (public/figures/): the figure file is served with its
//     own CSP (vercel.json "/figures/(.*)": its inline script and styles by hash, frame-ancestors 'self') and
//     X-Frame-Options SAMEORIGIN; in the page's sandboxed frame its script runs (the tour fills its callout, the
//     frame is sized by the figure's message) with no CSP report; an inline script added to it is refused; a page
//     on another origin cannot frame the figure either.
// Needs src/wasm/release (npm run wasm:release or npm run build) and Playwright's Chromium (npm run e2e:install).
// Ports: 4173 (preview) and 4174 (the framing page). Run: npm run check:csp.
import { createServer } from 'node:http';
import { readFileSync, rmSync } from 'node:fs';
import path from 'node:path';
import { WEB, run, start } from './lib.mjs';
import { broadSources, directives } from './csp.mjs';

process.env.PLAYWRIGHT_BROWSERS_PATH ??= path.join(WEB, '.playwright-browsers');
const { chromium } = await import('@playwright/test');

const out = path.join(WEB, 'dist-csp-check');
const vite = path.join(WEB, 'node_modules', 'vite', 'bin', 'vite.js');
// A wss:// URL nobody answers (a production build takes wss:// node URLs only: config.ts). F6 part C: inside the
// narrowed connect-src (a made-up name under a resolver domain, port 443), so the node client's failure is the
// network's, not the CSP's.
const NODE_URL = 'wss://kl-csp-check-unused.kaspa.blue/kaspa/testnet-10/wrpc/borsh';
const vercel = JSON.parse(readFileSync(path.join(WEB, 'vercel.json'), 'utf8')).headers;
const ruleOf = (source) =>
  Object.fromEntries(vercel.find((h) => h.source === source).headers.map((h) => [h.key, h.value]));
const CSP = ruleOf('/((?!figures/).*)')['Content-Security-Policy'];
const FIGURE_CSP = ruleOf('/figures/(.*)')['Content-Security-Policy'];
const FIGURE = '/figures/draw-cabinet-tour.html';
// F6 part C: the indexer origin vercel.json allows (the one https source without a wildcard); the check page is
// built against it, so the probe of the allowed indexer origin tests the policy that ships.
const CONNECT = directives(CSP).find(([n]) => n === 'connect-src')?.[1] ?? [];
const INDEXER = CONNECT.find((x) => x.startsWith('https://') && !x.includes('*')) ?? 'https://indexer.invalid';
const RESOLVER = CONNECT.includes('https://*.kaspa.stream');

let bad = 0;
const check = (ok, what) => {
  console.log(`check-csp: ${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) bad++;
};

rmSync(out, { recursive: true, force: true });
run(process.execPath, [vite, 'build', '--outDir', out, '--emptyOutDir'], {
  cwd: WEB,
  env: {
    ...process.env,
    VITE_INDEXER_URL: `${INDEXER}/v1/`,
    VITE_COVENANT_ID: '00'.repeat(32),
    VITE_NODE_URL: NODE_URL,
  },
});
const preview = start('preview', process.execPath, [vite, 'preview', '--outDir', out], { cwd: WEB });
const framer = createServer((q, r) => {
  r.setHeader('content-type', 'text/html');
  const src = q.url === '/figure' ? `http://127.0.0.1:4173${FIGURE}` : 'http://127.0.0.1:4173/';
  r.end(`<!doctype html><iframe id="f" src="${src}"></iframe>`);
}).listen(4174, '127.0.0.1');
const browser = await chromium.launch();
try {
  for (let i = 0; i < 60; i++) {
    const ok = await fetch('http://127.0.0.1:4173/').then(
      (r) => r.ok,
      () => false,
    );
    if (ok) break;
    await new Promise((r) => setTimeout(r, 500));
  }
  const page = await browser.newPage();
  const reports = [];
  page.on('console', (m) => {
    if (process.env.CSP_DEBUG) console.log(`console: ${m.type()}: ${m.text().slice(0, 300)}`);
    if (/Content Security Policy|Refused to/i.test(m.text())) reports.push(m.text());
  });
  const workerUp = page.waitForEvent('worker', { timeout: 60_000 });
  const res = await page.goto('http://127.0.0.1:4173/');
  const h = res.headers();
  check(h['content-security-policy'] === CSP, "the page is served with vercel.json's CSP header");
  check(/frame-ancestors 'none'/.test(h['content-security-policy'] ?? ''), "the CSP has frame-ancestors 'none'");
  check(h['x-frame-options'] === 'DENY', 'X-Frame-Options DENY');
  const worker = await workerUp;
  const wres = await fetch(worker.url());
  check(
    wres.headers.get('content-security-policy') === CSP,
    `the worker script (${path.basename(new URL(worker.url()).pathname)}) is served with the same CSP`,
  );
  const code = await page.getByTestId('round-card-error').getAttribute('data-code', { timeout: 90_000 });
  check(
    code === 'node_unavailable',
    `the worker ran the wasm under the CSP and reached the node client (round card: ${code})`,
  );
  // The policy is enforced in both contexts (Playwright's evaluate may run eval past a CSP, CDP's
  // allowUnsafeEvalBlockedByCSP, so enforcement is shown with what it does not bypass): an inline script
  // in the page is refused, and a ws:// socket (outside connect-src) is refused inside the worker. So a
  // glue that needed eval would have stopped the worker; it reached the node client instead (above).
  const inline = await page.evaluate(
    () =>
      new Promise((resolve) => {
        document.addEventListener('securitypolicyviolation', (e) => resolve(e.effectiveDirective), { once: true });
        const el = document.createElement('script');
        el.textContent = 'window.__inline = 1';
        document.head.append(el);
        setTimeout(() => resolve('none'), 2000);
      }),
  );
  check(String(inline).startsWith('script-src'), `the page refuses an inline script (${inline})`);
  const socketIn = (where) =>
    where.evaluate(
      () =>
        new Promise((resolve) => {
          self.addEventListener('securitypolicyviolation', (e) => resolve(e.effectiveDirective), { once: true });
          try {
            new WebSocket('ws://127.0.0.1:4174/');
          } catch (e) {
            resolve(`threw ${e instanceof Error ? e.name : String(e)}`);
          }
          setTimeout(() => resolve('none'), 3000);
        }),
    );
  const inWorker = await socketIn(worker);
  check(
    inWorker === 'connect-src' || inWorker === 'threw SecurityError',
    `the worker enforces its own CSP (a ws:// socket: ${inWorker})`,
  );
  const inPage = await socketIn(page);
  check(inPage === 'connect-src' || inPage === 'threw SecurityError', `the page enforces it too (${inPage})`);
  // F6 part C: the narrowed connect-src.
  const connectSrc = CONNECT;
  check(
    connectSrc.length > 1 && broadSources(connectSrc).length === 0,
    `connect-src names its hosts, no scheme-wide source (${connectSrc.join(' ')})`,
  );
  const connectIn = (where, kind, url) =>
    where.evaluate(
      ([kind, url]) =>
        new Promise((resolve) => {
          const host = new URL(url).host;
          self.addEventListener('securitypolicyviolation', (e) => {
            if (String(e.blockedURI).includes(host)) resolve(e.effectiveDirective);
          });
          try {
            if (kind === 'ws') new WebSocket(url).close();
            else fetch(url, { mode: 'cors', credentials: 'omit' }).catch(() => undefined);
          } catch (e) {
            resolve(`threw ${e instanceof Error ? e.name : String(e)}`);
          }
          setTimeout(() => resolve('none'), 4000);
        }),
      [kind, url],
    );
  const refused = (r) => r === 'connect-src' || r === 'threw SecurityError';
  for (const [name, where] of [
    ['page', page],
    ['worker', worker],
  ]) {
    const a = await connectIn(where, 'fetch', 'https://example.com/');
    check(refused(a), `${name}: an https fetch to an unlisted host is refused (${a})`);
    const b = await connectIn(where, 'ws', 'wss://example.com/');
    check(refused(b), `${name}: a wss socket to an unlisted host is refused (${b})`);
    if (RESOLVER) {
      const c = await connectIn(where, 'fetch', 'https://eric.kaspa.stream/v2/kaspa/testnet-10/tls/wrpc/borsh');
      check(c === 'none', `${name}: a fetch to a resolver host is allowed by the CSP (${c})`);
    }
    const d = await connectIn(where, 'fetch', `${INDEXER}/v1/status`);
    check(d === 'none', `${name}: a fetch to the indexer origin is allowed by the CSP (${d})`);
  }
  // The probes' own reports: their directives, and the console's wording of a refused fetch to the probe host.
  const other = reports.filter((t) => !/connect-src|script-src|https:\/\/example\.com\//.test(t));
  check(
    other.length === 0,
    `no CSP report but the probes' own (${reports.length} report(s))${other.length ? `: ${other.join(' | ')}` : ''}`,
  );
  const fp = await browser.newPage();
  await fp.goto('http://127.0.0.1:4174/');
  await fp.waitForTimeout(1500);
  const framed = fp.frames().find((f) => f.url().startsWith('http://127.0.0.1:4173'));
  const root = framed ? await framed.$('#root').catch(() => null) : null;
  check(root === null, 'a page on another origin cannot frame the site');

  // F6 part B: the framed figure on How it works.
  const fres = await fetch(`http://127.0.0.1:4173${FIGURE}`);
  check(
    fres.headers.get('content-security-policy') === FIGURE_CSP,
    "the figure is served with vercel.json's figure CSP",
  );
  check(
    /frame-ancestors 'self'/.test(FIGURE_CSP) && /script-src 'sha256-/.test(FIGURE_CSP),
    "the figure CSP: script by hash, frame-ancestors 'self'",
  );
  check(fres.headers.get('x-frame-options') === 'SAMEORIGIN', 'the figure: X-Frame-Options SAMEORIGIN');
  const how = await browser.newPage();
  const howReports = [];
  how.on('console', (m) => {
    if (/Content Security Policy|Refused to/i.test(m.text())) howReports.push(m.text());
  });
  await how.goto('http://127.0.0.1:4173/#/how');
  const frameEl = how.getByTestId('how-figure-frame');
  await frameEl.scrollIntoViewIfNeeded();
  await how
    .waitForFunction(
      () => document.querySelector('[data-testid="how-figure-frame"]')?.getAttribute('data-state') === 'sized',
      null,
      { timeout: 30_000 },
    )
    .catch(() => undefined);
  check(
    (await frameEl.getAttribute('data-state')) === 'sized',
    'the figure ran in its sandboxed frame and posted its height',
  );
  const fig = how.frames().find((f) => f.url().endsWith(FIGURE));
  const title = fig
    ? await fig
        .locator('[data-callout] [data-callout-title]')
        .textContent({ timeout: 10_000 })
        .catch(() => '')
    : '';
  check(Boolean(title), `the figure's tour runs under its CSP (callout: ${title})`);
  const figInline = fig
    ? await fig.evaluate(
        () =>
          new Promise((resolve) => {
            document.addEventListener('securitypolicyviolation', (e) => resolve(e.effectiveDirective), { once: true });
            const el = document.createElement('script');
            el.textContent = 'window.__inline = 1';
            document.head.append(el);
            setTimeout(() => resolve('none'), 2000);
          }),
      )
    : 'no frame';
  check(String(figInline).startsWith('script-src'), `the figure refuses an inline script (${figInline})`);
  const howOther = howReports.filter((t) => !/script-src/.test(t));
  check(
    howOther.length === 0,
    `How it works and its figure: no CSP report but the probe's own${howOther.length ? `: ${howOther.join(' | ')}` : ''}`,
  );
  const fp2 = await browser.newPage();
  await fp2.goto('http://127.0.0.1:4174/figure');
  await fp2.waitForTimeout(1500);
  const framedFig = fp2.frames().find((f) => f.url().startsWith('http://127.0.0.1:4173'));
  const plate = framedFig ? await framedFig.$('.iso-plate').catch(() => null) : null;
  check(plate === null, 'a page on another origin cannot frame the figure');
} finally {
  await browser.close();
  framer.close();
  preview.kill();
  rmSync(out, { recursive: true, force: true });
}
if (bad) {
  console.error(`check-csp: ${bad} problems`);
  process.exit(1);
}
console.log('check-csp: the page, its worker and the wasm glue run under the header CSP');
