// F5 end to end on the private simnet (spec 8: close, draw, a void, verify on the round page: match, and mismatch
// against a doctored API), with the quick wallet funded by the dev faucet. The dev run starts with round 1 drawn,
// round 2 closing (past D: its anchor proof can be built) and round 3 open. In order:
// 1. S3: the list's statuses from the chain; round 1's detail and Verify (every check a match, X.hash bound to the
//    node's block); round 2's provisional numbers; the layout and AX checks in both themes.
// 2. S4 draw(): round 2 drawn from its panel (the reward, the fee and the total shown first), then verified.
// 3. S4 close(): a ticket bought in round 3; the dev faucet's TEST-ONLY `POST /mine?to=deadline` mines past T; round
//    3 closed from its panel; `POST /mine?to=anchor` mines past its D.
// 4. A doctored API (the TEST-ONLY `indexer-api-doctored` on the dev run's store, one per mode, the page's requests
//    to the indexer routed to it in the browser): Verify on round 1 shows the lie as a mismatch of its own item;
//    with doctored purchases the panel of round 3 refuses to build its draw.
// The void is `void.spec.ts`, last of all (its 864,000 blocks leave the dev node slow for anything after it).
import { spawn, type ChildProcess } from 'node:child_process';
import path from 'node:path';
import { expect, request, test, type BrowserContext } from '@playwright/test';
import { buyAndWait, fundedQuickWallet, shots, ticketOf, type QuickWallet } from './helpers';
import { detail, devPost, pressCaller, quality, results, run, statusIs, verify } from './roundsHelpers';

const web = path.dirname(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')));
const crates = path.join(web, '..', 'crates');

test.describe.configure({ mode: 'serial' });

let wallet: QuickWallet | undefined;
const doctors: ChildProcess[] = [];

test.afterAll(async () => {
  for (const d of doctors) d.kill();
  await wallet?.ctx.close();
});

test('S3: the rounds from the chain; round 1 verified in this browser; round 2 provisional; layout and AX', async ({
  page,
}) => {
  await page.goto('/#/rounds');
  await expect(page.getByTestId('rounds')).toHaveAttribute('data-state', 'ready', { timeout: 90_000 });
  for (const [r, s] of [
    ['3', 'open'],
    ['2', 'closing'],
    ['1', 'drawn'],
  ]) {
    await expect(page.getByTestId(`round-row-${r}`)).toHaveAttribute('data-status', s as string, { timeout: 90_000 });
  }
  await expect(page.getByTestId('round-row-3-sales-value')).toHaveAttribute('data-checked', 'true');
  await quality(page, 'f5-rounds');
  await page.getByTestId('round-row-1-link').click();
  await expect(page.getByTestId('round-detail')).toHaveAttribute('data-status', 'drawn', { timeout: 90_000 });
  await expect(page.getByTestId('round-numbers')).toHaveAttribute('data-state', 'final');
  await expect(page.getByTestId('round-roots')).toHaveAttribute('data-count', '16');
  // F5 review: X bound to the node's block by the detail itself (hash and DAA score); the draw's ID bound to the
  // drawer of round 2 (only round 1's draw creates it); the close's ID no live object fixes any more: unverified.
  await expect(page.getByTestId('round-anchor')).toHaveAttribute('data-state', 'checked', { timeout: 30_000 });
  await expect(page.getByTestId('round-anchor-daa')).toHaveAttribute('data-state', 'checked');
  await expect(page.getByTestId('round-draw-tx')).toHaveAttribute('data-checked', 'true');
  await expect(page.getByTestId('round-close-tx')).toHaveAttribute('data-checked', 'false');
  await expect(page.getByTestId('round-m')).toHaveAttribute('data-checked', 'true');
  const v = await verify(page);
  await expect(v).toHaveAttribute('data-consistent', 'true');
  const items = await results(page);
  console.log(`round 1 verify: ${JSON.stringify(items.map((i) => `${i.kind}${i.shard}:${i.result}`))}`);
  expect(items.filter((i) => i.kind === 'tree' && i.result === 'match')).toHaveLength(16);
  for (const k of ['status', 'roots', 'numbers', 'anchor']) {
    expect(items.find((i) => i.kind === k)?.result, k).toBe('match');
  }
  // After Verify, X.hash shows as checked (bound to the node's block).
  await expect(page.getByTestId('round-anchor')).toHaveAttribute('data-checked', 'true');
  await quality(page, 'f5-round-1');
  await detail(page, '2');
  await expect(page.getByTestId('round-detail')).toHaveAttribute('data-status', 'closing');
  await expect(page.getByTestId('round-numbers')).toHaveAttribute('data-state', 'provisional');
  await expect(page.getByTestId('round-numbers-status')).toHaveText('Provisional');
  // Before the draw X is bound only to the node's current view: provisional, like the numbers. The close's ID is
  // the marker's transaction.
  await expect(page.getByTestId('round-anchor')).toHaveAttribute('data-state', 'provisional');
  await expect(page.getByTestId('round-anchor-provisional')).toHaveText('Provisional');
  await expect(page.getByTestId('round-close-tx')).toHaveAttribute('data-checked', 'true');
  await expect(page.getByTestId('caller')).toHaveAttribute('data-due', 'due');
  await expect(page.getByTestId('caller-connect')).toBeVisible();
  await quality(page, 'f5-round-2');
});

test('S4 draw(): the closing round drawn from its panel, then verified', async ({ browser }) => {
  wallet = await fundedQuickWallet(browser);
  const page = wallet.page;
  await detail(page, '2');
  const id = await pressCaller(page, 'draw');
  console.log(`draw() of round 2: ${id}`);
  await statusIs(page, '2', 'drawn');
  await expect(page.getByTestId('round-numbers')).toHaveAttribute('data-state', 'final');
  const v = await verify(page);
  await expect(v).toHaveAttribute('data-consistent', 'true');
  const items = await results(page);
  expect(items.find((i) => i.kind === 'numbers')?.result).toBe('match');
  expect(items.find((i) => i.kind === 'anchor')?.result).toBe('match');
});

test('S4 close(): a ticket in round 3, T passed (dev mine), round 3 closed from its panel', async ({ browser }) => {
  test.setTimeout(10 * 60_000);
  wallet ??= await fundedQuickWallet(browser);
  const page = wallet.page;
  await ticketOf(page, 1);
  console.log(`round 3 ticket: ${await buyAndWait(page)}`);
  await detail(page, '3');
  await expect(page.getByTestId('caller')).toHaveAttribute('data-due', /later|due/);
  await devPost('/mine?to=deadline', 5 * 60_000);
  await detail(page, '3');
  const id = await pressCaller(page, 'close');
  console.log(`close() of round 3: ${id}`);
  await statusIs(page, '3', 'closing');
  await devPost('/mine?to=anchor', 5 * 60_000);
});

const DOCTORED = 'indexer-api-doctored' + (process.platform === 'win32' ? '.exe' : '');

async function doctor(mode: string, port: number): Promise<string> {
  const r = run();
  const bin = path.join(crates, 'target', 'release', DOCTORED);
  const child = spawn(
    bin,
    ['--store', r.store, '--cov', r.covenantId, '--bind', `127.0.0.1:${port}`, '--doctor', mode],
    {
      stdio: 'ignore',
    },
  );
  doctors.push(child);
  const base = `http://127.0.0.1:${port}/v1/`;
  const ctx = await request.newContext();
  try {
    await expect
      .poll(async () => (await ctx.get(`${base}status`).catch(() => null))?.status() ?? 0, { timeout: 60_000 })
      .toBe(200);
  } finally {
    await ctx.dispose();
  }
  return base;
}

/** Send the page's indexer requests (those matching `only`, else all) to the doctored server at `base`. */
async function routeTo(ctx: BrowserContext, base: string, only?: RegExp) {
  const api = new URL(run().api);
  const to = new URL(base);
  await ctx.route(`${api.origin}/**`, async (route) => {
    const u = new URL(route.request().url());
    if (only && !only.test(u.pathname)) return route.continue();
    u.port = to.port;
    const resp = await route.fetch({ url: u.toString() });
    await route.fulfill({ response: resp });
  });
}

test('a doctored API: Verify names each lie as a mismatch; doctored purchases: the panel refuses to build', async ({
  browser,
}) => {
  test.setTimeout(15 * 60_000);
  const want: Record<string, string> = {
    wrong_state: 'state',
    wrong_tree: 'tree',
    missing_purchase: 'tree',
    extra_purchase: 'tree',
    wrong_winning: 'numbers',
    wrong_anchor_proof: 'numbers',
  };
  let port = 8792;
  const bases: Record<string, string> = {};
  for (const mode of Object.keys(want)) bases[mode] = await doctor(mode, port++);
  for (const [mode, kind] of Object.entries(want)) {
    const ctx = await browser.newContext({ viewport: { width: 390, height: 844 } });
    await routeTo(ctx, bases[mode] as string);
    const page = await ctx.newPage();
    await detail(page, '1');
    if (mode === 'wrong_state') {
      // The round's objects do not match the chain: the detail shows the mismatch and builds nothing.
      await expect(page.getByTestId('round-detail')).toHaveAttribute('data-state', 'error');
      await expect(page.getByTestId('round-detail-error')).toHaveAttribute('data-code', 'chain_mismatch');
      await expect(page.getByTestId('caller')).toHaveCount(0);
    }
    const v = await verify(page);
    await expect(v).toHaveAttribute('data-consistent', 'false');
    await expect(page.getByTestId('verify-mismatch')).toBeVisible();
    const items = await results(page);
    const bad = items.filter((i) => i.result === 'mismatch');
    console.log(`${mode}: mismatches ${JSON.stringify(bad.map((i) => `${i.kind}${i.shard}`))}`);
    expect(bad.length).toBeGreaterThan(0);
    expect(
      bad.every((i) => i.kind === kind),
      `${mode}: only ${kind} items`,
    ).toBe(true);
    if (mode === 'wrong_tree')
      await page.screenshot({ path: path.join(shots, 'f5-verify-mismatch.png'), fullPage: true });
    await ctx.close();
  }
  // Round 3 is closing and past D: its draw() with doctored purchases is refused before
  // anything is built or signed.
  wallet ??= await fundedQuickWallet(browser);
  const w = wallet;
  await routeTo(w.ctx, bases['wrong_tree'] as string, /\/purchases$/);
  // A fresh load, so the panel builds again through the doctored purchases (a hash change alone keeps the page).
  await w.page.goto('about:blank');
  await detail(w.page, '3');
  const panel = w.page.getByTestId('caller');
  await expect(panel).toHaveAttribute('data-due', 'due', { timeout: 180_000 });
  await expect(w.page.getByTestId('caller-error')).toBeVisible({ timeout: 120_000 });
  await expect(w.page.getByTestId('caller-error')).toHaveText("The indexer's copy does not match the chain.");
  await expect(w.page.getByTestId('caller-button')).toBeDisabled();
  await w.ctx.unroute(`${new URL(run().api).origin}/**`);
});
