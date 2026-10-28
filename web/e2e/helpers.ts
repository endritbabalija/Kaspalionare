// Shared by the e2e suites (spec 8): the dev run's facts, axe-core, the AX check (every interactive element has
// a data-testid and an accessible name), layout checks (no horizontal scroll, touch targets), and the quick
// wallet set up and funded from the dev faucet.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import AxeBuilder from '@axe-core/playwright';
import { expect, type Browser, type BrowserContext, type Page } from '@playwright/test';

const web = path.dirname(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')));
export const shots = path.join(web, 'test-results', 'shots');

export interface DevRun {
  api: string;
  covenantId: string;
  network: string;
  referralAddress?: string;
}

export function devRun(): DevRun {
  const p = path.join(web, '..', 'crates', 'target', 'simnet-dev', 'current.json');
  const text = readFileSync(p, 'utf8');
  return JSON.parse(text) as DevRun;
}

export async function roundCardReady(page: Page) {
  const card = page.getByTestId('round-card');
  await expect(card).toHaveAttribute('data-state', /ready|stale/);
  return card;
}

export async function axe(page: Page, label: string) {
  // Colour transitions (a theme switch) must have finished, or axe reads blended colours.
  // (Transitions only: skeletons pulse forever.)
  await page.evaluate(() =>
    Promise.all(
      document
        .getAnimations()
        .filter((a) => a instanceof CSSTransition)
        .map((a) => a.finished.catch(() => undefined)),
    ),
  );
  const r = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
  const bad = r.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical');
  for (const v of r.violations) console.log(`axe ${label}: ${v.impact} ${v.id} (${v.nodes.length})`);
  expect(
    bad.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(' ')).join(', ')}`),
    label,
  ).toEqual([]);
  return r.violations.length;
}

/** Spec 8 AX check: every interactive element has a data-testid and an accessible name. */
export async function namedAndTagged(page: Page) {
  const missing = await page.evaluate(() => {
    const sel = 'button, a[href], input, select, textarea, [role="menuitem"], [role="menuitemradio"], [role="tab"]';
    const out: string[] = [];
    for (const el of Array.from(document.querySelectorAll<HTMLElement>(sel))) {
      const labelled = el.getAttribute('aria-labelledby');
      const labels = (el as HTMLInputElement).labels;
      const name =
        el.getAttribute('aria-label') ??
        (labelled
          ? labelled
              .split(' ')
              .map((id) => document.getElementById(id)?.textContent ?? '')
              .join(' ')
          : labels && labels.length
            ? Array.from(labels)
                .map((l) => l.textContent ?? '')
                .join(' ')
            : el.textContent);
      if (!el.dataset.testid) out.push(`no data-testid: ${el.outerHTML.slice(0, 120)}`);
      if (!name || !name.trim()) out.push(`no name: ${el.outerHTML.slice(0, 120)}`);
    }
    return out;
  });
  expect(missing).toEqual([]);
}

/** Spec 8 quality: no horizontal scroll, and every visible control in the page at least 44 x 44 px. */
export async function layoutHolds(page: Page, label: string) {
  const r = await page.evaluate(() => {
    const small: string[] = [];
    const sel = 'main button, main a[href], main input, header button, header a[href], nav a[href]';
    for (const el of Array.from(document.querySelectorAll<HTMLElement>(sel))) {
      const b = el.getBoundingClientRect();
      if (b.width === 0 && b.height === 0) continue;
      if (b.height < 44 || b.width < 44) {
        small.push(`${el.dataset.testid ?? el.tagName} ${Math.round(b.width)}x${Math.round(b.height)}`);
      }
    }
    return { scroll: document.documentElement.scrollWidth, width: window.innerWidth, small };
  });
  expect(r.scroll, `${label}: horizontal scroll`).toBeLessThanOrEqual(r.width);
  expect(r.small, `${label}: touch targets below 44 px`).toEqual([]);
}

export interface QuickWallet {
  ctx: BrowserContext;
  page: Page;
  address: string;
}

/** A fresh browser context with a quick wallet funded by the dev faucet (100 KAS), on the wallet sheet. */
export async function fundedQuickWallet(browser: Browser, search = ''): Promise<QuickWallet> {
  const ctx = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  page.on('console', (m) => {
    if (m.type() === 'warning' || m.type() === 'error') console.log(`[page ${m.type()}] ${m.text()}`);
  });
  await page.goto(`/${search}#/wallet`);
  await page.getByTestId('quick-create').click();
  await expect(page.getByTestId('wallet-status')).toHaveText('Quick wallet');
  const address = (await page.getByTestId('wallet-address').getAttribute('data-value')) ?? '';
  expect(address).toMatch(/^kaspasim:/);
  // The dev faucet pays one request at a time from coins of its own that must be confirmed: retry while busy.
  for (let i = 0; ; i++) {
    await page.getByTestId('dev-faucet').click();
    await expect(page.getByTestId('dev-faucet')).toHaveAttribute('data-state', /paid|failed/);
    if ((await page.getByTestId('dev-faucet').getAttribute('data-state')) === 'paid') break;
    expect(i, 'the dev faucet keeps failing').toBeLessThan(12);
    await page.waitForTimeout(5_000);
  }
  await expect(page.getByTestId('wallet-balance')).toHaveAttribute('data-sompi', /^[1-9]\d{9,}$/, { timeout: 60_000 });
  return { ctx, page, address };
}

/** On Play: put `b` quick-picked combinations on the ticket and wait for the cost panel's build. */
export async function ticketOf(page: Page, b: number) {
  await page.goto('/#/play');
  await expect(page.getByTestId('buy-card')).toHaveAttribute('data-state', 'open');
  for (let i = 0; i < b; i++) {
    await page.getByTestId('quick-pick').click();
    await page.getByTestId('add-combination').click();
  }
  await expect(page.getByTestId('ticket-count')).toHaveText(new RegExp(`^${b}/`));
  const t0 = Date.now();
  await expect(page.getByTestId('cost-panel')).toHaveAttribute('data-state', 'ready', { timeout: 60_000 });
  // Spec 8 quality (indicative, desktop Chromium on this PC): ticket to cost panel, including the 250 ms
  // debounce, the indexer and node reads, buildBuy and buyFigures in the worker.
  console.log(`cost panel ready for b = ${b} in ${Date.now() - t0} ms`);
}

/** Press Buy and wait for "accepted"; the transaction id. */
export async function buyAndWait(page: Page): Promise<string> {
  await page.getByTestId('buy-button').click();
  await expect(page.getByTestId('tx-progress')).toHaveAttribute('data-step', 'accepted', { timeout: 150_000 });
  return (await page.getByTestId('buy-txid').getAttribute('data-value')) ?? '';
}

/** The local store's records in this page's IndexedDB (database `kaspalionare`). */
export async function localStore(page: Page): Promise<{ tickets: unknown[]; rounds: unknown[]; meta: unknown[] }> {
  return page.evaluate(
    () =>
      new Promise((resolve, reject) => {
        const r = indexedDB.open('kaspalionare');
        r.onerror = () => reject(r.error);
        r.onsuccess = () => {
          const db = r.result;
          const names = ['tickets', 'rounds', 'meta'].filter((n) => db.objectStoreNames.contains(n));
          if (!names.length) return resolve({ tickets: [], rounds: [], meta: [] });
          const tx = db.transaction(names);
          const out: Record<string, unknown[]> = { tickets: [], rounds: [], meta: [] };
          let left = names.length;
          for (const n of names) {
            const g = tx.objectStore(n).getAll();
            g.onsuccess = () => {
              out[n] = g.result;
              if (--left === 0) resolve(out as { tickets: unknown[]; rounds: unknown[]; meta: unknown[] });
            };
          }
        };
      }),
  );
}

/**
 * Run `fn` with the context's trace paused (F6 security review): the steps that show or type a quick wallet's key
 * (the exported key in the DOM, `fill()` with it) are kept out of the trace Playwright keeps on a failure
 * (`trace: 'retain-on-failure'`). The chunk recorded so far is dropped and a new one starts after `fn`.
 */
export async function untraced<T>(ctx: BrowserContext, fn: () => Promise<T>): Promise<T> {
  await ctx.tracing.stopChunk().catch(() => undefined);
  try {
    return await fn();
  } finally {
    await ctx.tracing.startChunk().catch(() => undefined);
  }
}
