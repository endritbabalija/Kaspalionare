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
// F4 review: T3 = [A] is bought too (A held three times), and T2's A and T3's A are claimed at once from two
// browsers with the same wallet: both claims spend the jackpot pool, so one is refused (its input taken) and
// is pressed again on the pool's successor; both end accepted. A first test closes a round without drawing it
// (`POST /close`, TEST-ONLY) and checks the closed copy against the live marker with the indexer API
// hanging (no answer at all: the page's read times out).
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

/** The indexer API hanging: no request to it is ever answered (the page's read times out). */
async function hangApi(ctx: BrowserContext) {
  const api = new URL(devRun().api);
  await ctx.route(`${api.origin}/**`, () => undefined);
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

/** A claim row's progress now: its transaction ID (once signed), its step, and whether it failed. */
interface Progress {
  id: string | null;
  step: string | null;
  failed: boolean;
}

async function progressOf(page: Page, txid: string, index: number): Promise<Progress> {
  const row = card(page, txid).getByTestId(new RegExp(`-claim-${index}-progress$`));
  const p = row.getByTestId('tx-progress');
  const id = row.getByTestId(new RegExp(`-claim-${index}-txid$`));
  if (!(await p.count())) return { id: null, step: null, failed: false };
  return {
    id: (await id.count()) ? await id.getAttribute('data-value') : null,
    step: await p.getAttribute('data-step'),
    failed: (await p.getAttribute('data-failed')) === 'true',
  };
}

/**
 * Press a claim row's button and follow that press only (F5 review): its outcome is read once the row shows this
 * press's own attempt (a progress no longer the earlier press's failure, or another transaction ID), so an earlier
 * press's "failed" is never read as this one's. Accepted: this press's step is accepted, or the card has ended
 * as claimed (a last claim). The press's transaction ID is logged.
 */
async function press(page: Page, txid: string, index: number): Promise<'accepted' | 'failed'> {
  const before = await progressOf(page, txid, index);
  await card(page, txid)
    .getByTestId(new RegExp(`-claim-${index}-button$`))
    .click();
  return outcomeAfter(page, txid, index, before);
}

async function outcomeAfter(page: Page, txid: string, index: number, before: Progress): Promise<'accepted' | 'failed'> {
  const until = Date.now() + 150_000;
  let started = false;
  let id: string | null = null;
  while (Date.now() < until) {
    const c = card(page, txid);
    const now = await progressOf(page, txid, index);
    // Only an earlier press's failure can be stale: without one, any state is this press's.
    started ||= !before.failed || !now.failed || (now.id !== null && now.id !== before.id);
    if (started && now.id) id = now.id;
    if (started && now.step === 'accepted') return log('accepted');
    if ((await c.getAttribute('data-state')) === 'claimed') return log('accepted');
    if (started && now.failed) return log('failed');
    await page.waitForTimeout(500);
  }
  // Still failed 150 s after the press (a press settles in seconds): this press failed as well.
  if ((await progressOf(page, txid, index)).failed) return log('failed');
  throw new Error(`claim ${txid}#${index}: no outcome of this press (transaction ${id ?? 'not shown'})`);

  function log(o: 'accepted' | 'failed') {
    console.log(`claim ${txid.slice(0, 8)}#${index}: ${o} (transaction ${id ?? 'not shown'})`);
    return o;
  }
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

test('F4 review: a closed copy is checked against the live marker, with the indexer API hanging', async ({
  browser,
  request,
}) => {
  test.setTimeout(20 * 60_000);
  const w = await fundedQuickWallet(browser);
  const { page } = w;
  const t0 = await buyTicket(page, [combinations().A]);
  await ticketsUntil(page, async () => (await card(page, t0).count()) === 1);
  const r = await request.post(`${faucetBase()}/close`, { timeout: 900_000 });
  const body = await r.text();
  expect(r.ok(), body).toBe(true);
  console.log(`dev close: ${body}`);
  // Copied again at the closed stage: the lists rebuild the live marker's frozen roots.
  await ticketsUntil(
    page,
    async () => (await page.getByTestId('local-copy-status').getAttribute('data-state')) === 'complete',
  );
  await expect(card(page, t0)).toHaveAttribute('data-state', 'open');
  // The API hanging: once its reads time out, the page works from the copy, still checked against the marker.
  await hangApi(w.ctx);
  await ticketsUntil(
    page,
    async () => (await page.getByTestId('tickets').getAttribute('data-api')) === 'down',
    300_000,
  );
  await expect(page.getByTestId('local-copy-status')).toHaveAttribute('data-state', 'complete');
  await page.screenshot({ path: path.join(shots, 'tickets-closed-api-hanging.png'), fullPage: true });
  await w.ctx.close();
});

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
  const t3 = await buyTicket(page, [A]);
  console.log(`A ${A.main.join(' ')} + ${A.bonus}; T1 ${t1}; T2 ${t2}; T3 ${t3}`);

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
  await expect(c2.getByTestId(/-claim-1-shared$/)).toHaveAttribute('data-value', '2');
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
  // Two claimers on one pool: T2's A here and T3's A in another browser with the same wallet, pressed at once.
  const two = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const p2 = await two.newPage();
  await p2.goto('/#/wallet');
  await p2.getByTestId('quick-import-input').fill(secret);
  await p2.getByTestId('quick-import').click();
  await expect(p2.getByTestId('wallet-status')).toHaveText('Quick wallet');
  await ticketsUntil(p2, async () => (await card(p2, t3).getAttribute('data-state')) === 'win');
  const b2 = card(page, t2).getByTestId(/-claim-1-button$/);
  const b3 = card(p2, t3).getByTestId(/-claim-0-button$/);
  await expect(b2).toBeEnabled({ timeout: 90_000 });
  await expect(b3).toBeEnabled({ timeout: 90_000 });
  const [before2, before3] = await Promise.all([progressOf(page, t2, 1), progressOf(p2, t3, 0)]);
  await Promise.all([b2.click(), b3.click()]);
  const first = await Promise.all([outcomeAfter(page, t2, 1, before2), outcomeAfter(p2, t3, 0, before3)]);
  console.log(`two claimers on one pool: T2 ${first[0]}, T3 ${first[1]}`);
  expect(first, 'both claims spent the pool: one is refused').toContain('failed');
  expect(first, 'one of them is accepted').toContain('accepted');
  // The refused one, pressed again once the accepted claim has settled (its card read again from the node: the
  // pool's successor and the wallet's change are then in the node's UTXO set), is accepted. A player presses
  // again on a refusal; up to three presses (one run saw a second refusal, an orphan, when the re-press was
  // built while the other claim was still settling).
  const won = first[0] === 'accepted' ? ([page, t2, 1] as const) : ([p2, t3, 0] as const);
  await ticketsUntil(won[0], async () => (await card(won[0], won[1]).getAttribute('data-state')) === 'claimed');
  for (const [p, t, i, o] of [
    [page, t2, 1, first[0]],
    [p2, t3, 0, first[1]],
  ] as const) {
    if (o === 'accepted') continue;
    let last: 'accepted' | 'failed' = o;
    for (let n = 0; n < 3 && last !== 'accepted'; n++) {
      const button = () => card(p, t).getByTestId(new RegExp(`-claim-${i}-button$`));
      // A card that ended claimed needs no press (the last press went through after all).
      await ticketsUntil(
        p,
        async () => (await card(p, t).getAttribute('data-state')) === 'claimed' || (await button().isEnabled()),
      );
      if ((await card(p, t).getAttribute('data-state')) === 'claimed') {
        last = 'accepted';
        break;
      }
      last = await press(p, t, i);
      console.log(`two claimers: re-press ${n + 1}: ${last}`);
    }
    expect(last).toBe('accepted');
  }
  await ticketsUntil(page, async () => (await card(page, t2).getAttribute('data-state')) === 'claimed');
  await ticketsUntil(p2, async () => (await card(p2, t3).getAttribute('data-state')) === 'claimed');
  await two.close();

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
  await expect(card(pf, t1).getByTestId(/-claim-1-shared$/)).toHaveAttribute('data-value', '2');
  await claim(pf, t1, 1);
  await ticketsUntil(pf, async () => (await card(pf, t1).getAttribute('data-state')) === 'claimed');
  await pf.screenshot({ path: path.join(shots, 'tickets-imported-claimed.png'), fullPage: true });
  await fresh.close();
  await w.ctx.close();
});
