// F4 end to end on the private simnet (spec 8 "Local copy" and the claim list), with the quick wallet funded by
// the dev faucet. Two tickets bought through the page: T1 = [B, A], T2 = [C, A], where A is a combination of
// this run (random, so no other buyer holds it), C has A's main numbers and another bonus (the second prize),
// and B four of A's numbers with A's bonus (the 4 + bonus tier: a prize below P_MIN). The dev run's `/round`
// (simnet only, TEST-ONLY) draws the closing round, closes this one and grinds its anchor so that A is the
// jackpot, held twice (a shared win, h = 2), and draws it. Then, from My tickets:
// - T2: the second prize (a win, PAY) and A (the shared win and the last claim, paid with the deposit);
// - with the indexer API stopped (every request to it refused): T1's B, below P_MIN (it accrues), built from
//   the local copy and accepted;
// - the backup downloaded; J3 in another browser (the wallet's key imported, tickets found by payout through
//   the API); a webstore/1 store migrated in place; a cleared browser refuses a corrupted backup, imports the
//   real one and, with the API stopped, makes T1's last claim (A, shared, with the deposit), accepted.
// return_deposit() cannot be reached on the simnet (its earliest lock time is 6.9 million DAA after a draw,
// genesis floors): the handoff says how it is covered instead.
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { expect, test, type BrowserContext, type Locator, type Page } from '@playwright/test';
import { axe, buyAndWait, devRun, fundedQuickWallet, layoutHolds, localStore, namedAndTagged, shots } from './helpers';

interface Combo {
  main: number[];
  bonus: number;
}

/** A of this run (random), C (A's numbers, bonus 2) and B (four of A's numbers, A's bonus). */
function combinations(): { A: Combo; B: Combo; C: Combo } {
  const all = Array.from({ length: 30 }, (_, i) => i + 1);
  const pick: number[] = [];
  while (pick.length < 5) {
    const n = all[Math.floor(Math.random() * all.length)] as number;
    if (!pick.includes(n)) pick.push(n);
  }
  const main = [...pick].sort((a, b) => a - b);
  const other = all.find((n) => !main.includes(n)) as number;
  const b = [...main.slice(0, 4), other].sort((x, y) => x - y);
  return { A: { main, bonus: 1 }, C: { main, bonus: 2 }, B: { main: b, bonus: 1 } };
}

async function buyTicket(page: Page, combos: Combo[]): Promise<string> {
  await page.goto('/#/play');
  await expect(page.getByTestId('buy-card')).toHaveAttribute('data-state', 'open');
  for (const c of combos) {
    for (const n of c.main) await page.getByTestId(`number-ball-${n}`).click();
    await page.getByTestId(`bonus-tile-${c.bonus}`).click();
    await page.getByTestId('add-combination').click();
  }
  await expect(page.getByTestId('cost-panel')).toHaveAttribute('data-state', 'ready', { timeout: 60_000 });
  return buyAndWait(page);
}

function faucetBase(): string {
  const p = path.join(
    path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')),
    '..',
    '..',
    'crates',
    'target',
    'simnet-dev',
    'current.json',
  );
  const info = JSON.parse(readFileSync(p, 'utf8')) as { faucet: string };
  return info.faucet.replace(/\/faucet$/, '');
}

/** The indexer API stopped, as the page sees it: every request to it is refused. */
async function stopApi(ctx: BrowserContext) {
  const api = new URL(devRun().api);
  await ctx.route(`${api.origin}/**`, (r) => r.abort('connectionrefused'));
}

function card(page: Page, txid: string): Locator {
  return page.locator(`article[data-key="${txid}:1"]`);
}

/** My tickets, read again until `f` holds (the screen also re-reads every 15 s). */
async function ticketsUntil(page: Page, f: () => Promise<boolean>, timeout = 120_000) {
  await expect
    .poll(
      async () => {
        await page.goto('/#/tickets');
        await expect(page.getByTestId('tickets')).toHaveAttribute('data-state', /ready|empty/, { timeout: 60_000 });
        return f();
      },
      { timeout, intervals: [2_000, 4_000, 8_000] },
    )
    .toBe(true);
}

async function claim(page: Page, txid: string, index: number) {
  const c = card(page, txid);
  const button = c.getByTestId(new RegExp(`-claim-${index}-button$`));
  await expect(button).toBeEnabled({ timeout: 90_000 });
  await button.click();
  // Accepted: the row's progress says so, or (a last claim) the card has ended as claimed.
  const progress = c.getByTestId(new RegExp(`-claim-${index}-progress$`)).getByTestId('tx-progress');
  await expect
    .poll(
      async () =>
        (await c.getAttribute('data-state')) === 'claimed' ||
        ((await progress.count()) > 0 && (await progress.getAttribute('data-step')) === 'accepted'),
      { timeout: 150_000 },
    )
    .toBe(true);
}

async function quality(page: Page, label: string) {
  for (const width of [360, 430]) {
    await page.setViewportSize({ width, height: 844 });
    await layoutHolds(page, `${label} ${width}`);
  }
  await page.setViewportSize({ width: 390, height: 844 });
  await namedAndTagged(page);
  await axe(page, `${label} dark`);
  await page.screenshot({ path: path.join(shots, `${label}-dark.png`), fullPage: true });
  await page.getByTestId('menu-button').click();
  await page.getByTestId('theme-light').click();
  await page.keyboard.press('Escape');
  await axe(page, `${label} light`);
  await page.screenshot({ path: path.join(shots, `${label}-light.png`), fullPage: true });
  await page.getByTestId('menu-button').click();
  await page.getByTestId('theme-dark').click();
  await page.keyboard.press('Escape');
}

test.describe.configure({ mode: 'serial' });

test('F4: a win, a shared win, an accrual below P_MIN and the last claims; the local copy without the API; backup and import', async ({
  browser,
  request,
}) => {
  test.setTimeout(40 * 60_000);
  const w = await fundedQuickWallet(browser);
  const { page } = w;
  await page.getByTestId('quick-export').click();
  const secret = (await page.getByTestId('quick-exported').textContent()) ?? '';
  await page.getByTestId('quick-exported-hide').click();
  const { A, B, C } = combinations();
  const t1 = await buyTicket(page, [B, A]);
  const t2 = await buyTicket(page, [C, A]);
  console.log(`A ${A.main.join(' ')} + ${A.bonus}; T1 ${t1}; T2 ${t2}`);

  // Before the draw: two open cards, the copies taken while open (incomplete).
  await ticketsUntil(page, async () => (await card(page, t1).count()) === 1 && (await card(page, t2).count()) === 1);
  await expect(card(page, t1)).toHaveAttribute('data-state', 'open');
  await expect(card(page, t1).getByTestId(/-deadline$/)).toContainText('DAA');
  await expect(page.getByTestId('local-copy-status')).toHaveAttribute('data-state', 'incomplete');
  await quality(page, 'tickets-open');

  // The dev round: A wins the jackpot (TEST-ONLY grind on the private simnet).
  const r = await request.post(`${faucetBase()}/round?main=${A.main.join(',')}&bonus=${A.bonus}`, { timeout: 900_000 });
  const body = await r.text();
  expect(r.ok(), body).toBe(true);
  console.log(`dev round: ${body}`);
  const drawn = JSON.parse(body) as { round: string; winning: Combo };
  expect(drawn.winning).toEqual(A);

  // Drawn: the copy completed after close() and the result kept (complete and verified).
  await ticketsUntil(page, async () => (await card(page, t2).getAttribute('data-state')) === 'win');
  await expect(page.getByTestId('local-copy-status')).toHaveAttribute('data-state', 'complete');
  const c2 = card(page, t2);
  await expect(c2.getByTestId(/-tier-0$/)).toHaveText('5');
  await expect(c2.getByTestId(/-tier-1$/)).toHaveText('5 + bonus');
  await expect(c2.getByTestId(/-balls-1-main-0$/)).toHaveAttribute('data-matched', 'true');
  await expect(c2.getByTestId(/-balls-0-bonus$/)).toHaveAttribute('data-matched', 'false');
  await expect(c2.getByTestId(/-claim-1-shared$/)).toHaveAttribute('data-value', '1');
  await expect(c2.getByTestId(/-claim-0-paid$/)).toHaveAttribute('data-value', 'pay');
  await expect(c2.getByTestId(/-claim-0-fee$/)).toHaveAttribute('data-sompi', /^\d+$/, { timeout: 60_000 });
  await expect(c2.getByTestId(/-claim-0-deadline$/)).toContainText('DAA');
  await expect(card(page, t1).getByTestId(/-claim-0-paid$/)).toHaveAttribute('data-value', 'accrue');
  await quality(page, 'tickets-drawn');

  // A win (PAY), then the shared win as the ticket's last claim (FINAL: with the deposit).
  await claim(page, t2, 0);
  await ticketsUntil(page, async () => (await card(page, t2).getAttribute('data-state')) === 'partly-claimed');
  await expect(card(page, t2).getByTestId(/-claim-1-paid$/)).toHaveAttribute('data-value', 'final');
  await expect(card(page, t2).getByTestId(/-claim-1-deposit$/)).toHaveAttribute('data-sompi', /^\d+$/, {
    timeout: 60_000,
  });
  await claim(page, t2, 1);
  await ticketsUntil(page, async () => (await card(page, t2).getAttribute('data-state')) === 'claimed');

  // With the indexer API stopped: T1's B (below P_MIN, it accrues), from the local copy and a node only.
  await stopApi(w.ctx);
  await ticketsUntil(page, async () => (await page.getByTestId('tickets').getAttribute('data-api')) === 'down');
  await expect(page.getByTestId('local-copy-status')).toHaveAttribute('data-state', 'complete');
  await claim(page, t1, 0);
  await ticketsUntil(page, async () => (await card(page, t1).getAttribute('data-state')) === 'partly-claimed');
  await page.screenshot({ path: path.join(shots, 'tickets-api-down.png'), fullPage: true });

  // The backup file.
  const [dl] = await Promise.all([page.waitForEvent('download'), page.getByTestId('local-copy-download').click()]);
  const file = path.join(shots, '..', 'backup.json');
  await dl.saveAs(file);
  const text = readFileSync(file, 'utf8');
  expect(text).not.toContain(secret);
  const store = await localStore(page);
  const t1Record = store.tickets.find((x) => (x as { key: string }).key === `${t1}:1`);

  // J3: another browser with the same wallet finds T1 by payout through the API (the trees checked against the roots).
  const j3 = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const p3 = await j3.newPage();
  await p3.goto('/#/wallet');
  await p3.getByTestId('quick-import-input').fill(secret);
  await p3.getByTestId('quick-import').click();
  await expect(p3.getByTestId('wallet-status')).toHaveText('Quick wallet');
  await ticketsUntil(p3, async () => (await p3.locator('article[data-key]').count()) >= 1);
  const found = p3.locator('article[data-key]').first();
  await expect(found).toHaveAttribute('data-round', drawn.round);
  await expect(found.getByTestId(/-claim-1-button$/)).toBeVisible();
  await expect(p3.getByTestId('local-copy-status')).toHaveAttribute('data-state', 'complete');
  await j3.close();

  // A webstore/1 store (F3's format) migrates in place.
  const mig = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const pm = await mig.newPage();
  await pm.goto('/#/wallet');
  await pm.evaluate(
    (rec) =>
      new Promise<void>((resolve, reject) => {
        const r = indexedDB.open('kaspalionare', 1);
        r.onupgradeneeded = () => {
          for (const s of ['meta', 'tickets', 'rounds']) r.result.createObjectStore(s);
        };
        r.onerror = () => reject(r.error);
        r.onsuccess = () => {
          const tx = r.result.transaction(['meta', 'tickets'], 'readwrite');
          tx.objectStore('meta').put({ format: 'webstore/1', createdMs: '1' }, 'format');
          const t = { ...(rec as Record<string, unknown>) };
          tx.objectStore('tickets').put(t, t.key as string);
          tx.oncomplete = () => {
            r.result.close();
            resolve();
          };
        };
      }),
    t1Record,
  );
  await ticketsUntil(pm, async () => (await card(pm, t1).count()) === 1);
  const migrated = await localStore(pm);
  expect((migrated.meta[0] as { format: string }).format).toBe('webstore/2');
  await mig.close();

  // A cleared browser, the API stopped: a corrupted backup is refused; the real one imported; T1's last claim.
  const fresh = await browser.newContext({ viewport: { width: 390, height: 844 } });
  await stopApi(fresh);
  const pf = await fresh.newPage();
  await pf.goto('/#/wallet');
  await pf.getByTestId('quick-import-input').fill(secret);
  await pf.getByTestId('quick-import').click();
  await expect(pf.getByTestId('wallet-status')).toHaveText('Quick wallet');
  await pf.goto('/#/backup');
  const bad = path.join(shots, '..', 'backup-corrupted.json');
  const mid = Math.floor(text.length / 2);
  const at = text.slice(mid).search(/[0-9a-f]/) + mid;
  writeFileSync(bad, text.slice(0, at) + (text[at] === '0' ? '1' : '0') + text.slice(at + 1));
  await pf.getByTestId('backup-file').setInputFiles(bad);
  await expect(pf.getByTestId('backup-error')).toHaveAttribute('data-code', 'backup_invalid');
  await expect(pf.getByTestId('backup-empty')).toBeVisible();
  await pf.getByTestId('backup-file').setInputFiles(file);
  await expect(pf.getByTestId('backup-imported')).toBeVisible({ timeout: 120_000 });
  await expect(pf.getByTestId(`backup-file-rounds-${drawn.round}-outcome-value`)).toHaveAttribute('data-value', 'kept');
  await expect(pf.getByTestId(`backup-rounds-round-${drawn.round}`)).toBeVisible();
  await quality(pf, 'backup');
  await ticketsUntil(pf, async () => (await card(pf, t1).getAttribute('data-state')) === 'partly-claimed');
  await expect(pf.getByTestId('tickets')).toHaveAttribute('data-api', 'down');
  await expect(card(pf, t1).getByTestId(/-claim-1-shared$/)).toHaveAttribute('data-value', '1');
  await claim(pf, t1, 1);
  await ticketsUntil(pf, async () => (await card(pf, t1).getAttribute('data-state')) === 'claimed');
  await pf.screenshot({ path: path.join(shots, 'tickets-imported-claimed.png'), fullPage: true });
  await fresh.close();
  await w.ctx.close();
});
