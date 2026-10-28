// F3 part B end to end on the private simnet (spec 8), with the quick wallet funded by the dev faucet: buy
// b = 1 and b = 4, with and without a referral link, a referral below M_ref (it stays in the round), a refused
// kaspa: link, and a real shard conflict rebuilt on another shard. Each purchase is checked against the dev
// run's own indexer API (the ticket and its referral, as the chain recorded them) and the browser's local store
// (the ticket's state bytes and the round's copy). The dev instance runs s_R = 500 at p = 1 KAS: one
// combination's referral (0.05 KAS) is below M_ref = 0.1 KAS, four's (0.2 KAS) is paid.
import path from 'node:path';
import { expect, test, type APIRequestContext, type Page } from '@playwright/test';
import {
  axe,
  buyAndWait,
  devRun,
  fundedQuickWallet,
  layoutHolds,
  localStore,
  namedAndTagged,
  shots,
  ticketOf,
  untraced,
} from './helpers';

interface Bought {
  transactionId: string;
  combinations: unknown[];
  referral: string;
  round: string;
}

/** The dev run's indexer record of a purchase (it follows the chain, so poll until it has it). */
async function boughtByApi(request: APIRequestContext, payout: string, id: string): Promise<Bought> {
  const run = devRun();
  let found: Bought | undefined;
  await expect
    .poll(
      async () => {
        const r = await request.get(`${run.api}tickets?payout=${encodeURIComponent(payout)}`);
        const body = (await r.json()) as { bought: Bought[] };
        found = body.bought.find((b) => b.transactionId === id);
        return !!found;
      },
      { timeout: 60_000 },
    )
    .toBe(true);
  return found as Bought;
}

async function referralRow(page: Page) {
  return page.getByTestId('referral-to').getAttribute('data-value');
}

/** The ticket and the round's copy are in this browser's local store (webstore/2). */
async function keptLocally(page: Page, id: string, round: string) {
  const run = devRun();
  await expect
    .poll(
      async () => {
        const s = await localStore(page);
        const t = s.tickets.find((x) => (x as { key: string }).key === `${id}:1`) as
          { ticket: { state: string; utxo: { transactionId: string } } } | undefined;
        const r = s.rounds.find((x) => (x as { key: string }).key === `${run.covenantId}:${round}`) as
          { purchases: string[]; stage: string } | undefined;
        return {
          ticket: !!t && t.ticket.state.length > 0 && t.ticket.utxo.transactionId === id,
          round: !!r && r.purchases.length === 16 && r.stage === 'open',
          format: (s.meta[0] as { format?: string } | undefined)?.format,
        };
      },
      { timeout: 90_000 },
    )
    .toEqual({ ticket: true, round: true, format: 'webstore/2' });
}

test.describe.configure({ mode: 'serial' });

test('the quick wallet: created, funded by the dev faucet, exported and imported; the sheet holds its layout', async ({
  browser,
}) => {
  const w = await fundedQuickWallet(browser);
  const { page } = w;
  await expect(page.getByTestId('quick-wallet-warning')).toHaveText(
    'Testnet key stored in this browser; anyone with this device can spend it',
  );
  await expect(page.getByTestId('wallet-network')).toHaveText('simnet');
  await expect(page.getByTestId('connect-button')).toHaveAttribute('aria-label', `Wallet ${w.address}`);
  // J7: the menu's referral link names this wallet.
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.getByTestId('menu-button').click();
  await expect(page.getByTestId('menu-referral')).not.toHaveAttribute('data-disabled', '');
  await page.getByTestId('menu-referral').click();
  const link = await page.evaluate(() => navigator.clipboard.readText());
  expect(new URL(link).searchParams.get('ref')).toBe(w.address);
  const secret = await untraced(w.ctx, async () => {
    await page.getByTestId('quick-export').click();
    const s = (await page.getByTestId('quick-exported').textContent()) ?? '';
    await page.getByTestId('quick-exported-hide').click();
    return s;
  });
  expect(/^[0-9a-f]{64}$/.test(secret), 'the exported key is 64 hex digits').toBe(true);
  for (const width of [360, 430]) {
    await page.setViewportSize({ width, height: 844 });
    await layoutHolds(page, `wallet ${width}`);
  }
  await namedAndTagged(page);
  await axe(page, 'wallet dark');
  await page.screenshot({ path: path.join(shots, 'wallet-dark.png'), fullPage: true });
  await page.getByTestId('menu-button').click();
  await page.getByTestId('theme-light').click();
  await page.keyboard.press('Escape');
  await axe(page, 'wallet light');
  // The same key imported in another browser gives the same address.
  const other = await browser.newContext();
  const p2 = await other.newPage();
  await p2.goto('/#/wallet');
  await untraced(other, async () => {
    await p2.getByTestId('quick-import-input').fill(secret);
    await p2.getByTestId('quick-import').click();
  });
  await expect(p2.getByTestId('wallet-status')).toHaveText('Quick wallet');
  await expect(p2.getByTestId('wallet-address')).toHaveAttribute('data-value', w.address);
  await other.close();
  await w.ctx.close();
});

test('buy b = 1 without a referral link: the referral is below M_ref and stays in the round', async ({
  browser,
  request,
}) => {
  const w = await fundedQuickWallet(browser);
  await ticketOf(w.page, 1);
  expect(await referralRow(w.page)).toBe('round');
  await expect(w.page.getByTestId('cost-price')).toHaveAttribute('data-sompi', '100000000');
  await expect(w.page.getByTestId('cost-referral')).toHaveAttribute('data-sompi', '5000000');
  await expect(w.page.getByTestId('deposit-back')).toContainText('DAA');
  for (const width of [360, 430]) {
    await w.page.setViewportSize({ width, height: 844 });
    await layoutHolds(w.page, `play ${width}`);
  }
  await namedAndTagged(w.page);
  await axe(w.page, 'play with a ticket, dark');
  await w.page.screenshot({ path: path.join(shots, 'play-ticket-dark.png'), fullPage: true });
  await w.page.getByTestId('menu-button').click();
  await w.page.getByTestId('theme-light').click();
  await w.page.keyboard.press('Escape');
  await axe(w.page, 'play with a ticket, light');
  await w.page.screenshot({ path: path.join(shots, 'play-ticket-light.png'), fullPage: true });
  const id = await buyAndWait(w.page);
  const b = await boughtByApi(request, w.address, id);
  expect(b.combinations).toHaveLength(1);
  expect(b.referral).toBe('0');
  await keptLocally(w.page, id, b.round);
  await axe(w.page, 'play accepted, light');
  await namedAndTagged(w.page);
  await w.page.screenshot({ path: path.join(shots, 'play-accepted.png'), fullPage: true });
  await w.ctx.close();
});

test("buy b = 4 without a referral link: the referral is paid to the site owner's address", async ({
  browser,
  request,
}) => {
  const owner = devRun().referralAddress;
  expect(owner).toMatch(/^kaspasim:/);
  const w = await fundedQuickWallet(browser);
  await ticketOf(w.page, 4);
  expect(await referralRow(w.page)).toBe(owner);
  await expect(w.page.getByTestId('cost-referral')).toHaveAttribute('data-sompi', '20000000');
  const id = await buyAndWait(w.page);
  const b = await boughtByApi(request, w.address, id);
  expect(b.combinations).toHaveLength(4);
  expect(b.referral).toBe('20000000');
  await keptLocally(w.page, id, b.round);
  await w.ctx.close();
});

test('buy with a referral link: b = 4 pays the link, b = 1 stays in the round; the link is remembered', async ({
  browser,
  request,
}) => {
  // The referrer: another quick wallet's address (no funds needed).
  const r = await browser.newContext();
  const rp = await r.newPage();
  await rp.goto('/#/wallet');
  await rp.getByTestId('quick-create').click();
  await expect(rp.getByTestId('wallet-status')).toHaveText('Quick wallet');
  const ref = (await rp.getByTestId('wallet-address').getAttribute('data-value')) ?? '';
  await r.close();

  const w = await fundedQuickWallet(browser, `?ref=${encodeURIComponent(ref)}`);
  expect(await w.page.evaluate(() => localStorage.getItem('kl.ref'))).toBe(ref);
  await ticketOf(w.page, 4);
  expect(await referralRow(w.page)).toBe(ref);
  const id4 = await buyAndWait(w.page);
  expect((await boughtByApi(request, w.address, id4)).referral).toBe('20000000');
  // Remembered without the parameter; below M_ref it stays in the round.
  await w.page.goto('/#/play');
  await ticketOf(w.page, 1);
  expect(await referralRow(w.page)).toBe('round');
  const id1 = await buyAndWait(w.page);
  expect((await boughtByApi(request, w.address, id1)).referral).toBe('0');
  await w.ctx.close();
});

test('a referral link of another network is refused: the site owner stays the referrer', async ({ browser }) => {
  const owner = devRun().referralAddress;
  const w = await fundedQuickWallet(browser);
  // Malformed stand-ins with the other networks' prefixes, built here (no real address of another network).
  const other = (prefix: string) => [prefix, 'qq'.repeat(31)].join(String.fromCharCode(58));
  for (const bad of [other('kaspa'), other('kaspatest'), '<img src=x onerror=alert(1)>']) {
    await w.page.goto(`/?ref=${encodeURIComponent(bad)}#/play`);
    await expect(w.page.getByTestId('buy-card')).toHaveAttribute('data-state', 'open');
    expect(await w.page.evaluate(() => localStorage.getItem('kl.ref'))).toBeNull();
  }
  await ticketOf(w.page, 4);
  expect(await referralRow(w.page)).toBe(owner);
  await w.ctx.close();
});

test('shard contention: a buy whose shard another buyer took first is rebuilt on another shard', async ({
  browser,
  request,
}) => {
  const a = await fundedQuickWallet(browser);
  const b = await fundedQuickWallet(browser);
  // A holds its signed, checked purchase before sending (DEV-only hook).
  await ticketOf(a.page, 1);
  await a.page.evaluate(() => {
    (window as unknown as { __klDev: { holdBeforeSubmit: boolean } }).__klDev.holdBeforeSubmit = true;
  });
  await a.page.getByTestId('buy-button').click();
  await expect
    .poll(() => a.page.evaluate(() => (window as unknown as { __klDev: { holding?: boolean } }).__klDev.holding))
    .toBe(true);
  const shard = await a.page.evaluate(
    () => (window as unknown as { __klDev: { lastShard: number } }).__klDev.lastShard,
  );
  // B buys on the same shard first.
  await b.page.goto('/#/play');
  await b.page.evaluate((s) => {
    (window as unknown as { __klDev: { forceShard: number } }).__klDev.forceShard = s;
  }, shard);
  await ticketOf(b.page, 1);
  await expect(b.page.getByTestId('buy-button')).toHaveAttribute('data-shard', String(shard));
  const idB = await buyAndWait(b.page);
  await boughtByApi(request, b.address, idB);
  // A sends: the node refuses its spent shard; the page rebuilds on another one and the purchase goes through.
  await a.page.evaluate(() => (window as unknown as { __klDev: { release: () => void } }).__klDev.release());
  await expect(a.page.getByTestId('tx-rebuilt')).toHaveText('Rebuilt 1/3', { timeout: 60_000 });
  // F2/F3 review M2: the rebuild is signed by itself only when its figures are the ones shown; otherwise
  // the new figures wait for a fresh press (purchase_changed) and nothing is signed until then.
  const outcome = async () => {
    if ((await a.page.getByTestId('tx-progress').getAttribute('data-step', { timeout: 1000 })) === 'accepted')
      return 'accepted';
    const failed = a.page.getByTestId('buy-failed-code');
    return (await failed.count())
      ? ((await failed.getAttribute('data-code', { timeout: 1000 })) ?? 'pending')
      : 'pending';
  };
  await expect.poll(outcome, { timeout: 150_000 }).not.toBe('pending');
  if ((await outcome()) === 'purchase_changed') {
    await expect(a.page.getByTestId('cost-panel')).toHaveAttribute('data-state', 'ready');
    await expect(a.page.getByTestId('buy-button')).toBeEnabled();
    await a.page.getByTestId('buy-button').click();
  }
  await expect(a.page.getByTestId('tx-progress')).toHaveAttribute('data-step', 'accepted', { timeout: 150_000 });
  const idA = (await a.page.getByTestId('buy-txid').getAttribute('data-value')) ?? '';
  expect(idA).not.toBe('');
  const after = await a.page.evaluate(
    () => (window as unknown as { __klDev: { lastShard: number } }).__klDev.lastShard,
  );
  expect(after).not.toBe(shard);
  await boughtByApi(request, a.address, idA);
  await a.page.screenshot({ path: path.join(shots, 'play-rebuilt.png'), fullPage: true });
  await a.ctx.close();
  await b.ctx.close();
});
