// F5 end to end on the private simnet (spec 8 "a void"), last of all the suites: the void window is 864,000 blocks
// (VOID_MIN, 24 hours of chain time), which the dev node takes about half an hour to mine and leaves it slow, so
// nothing runs after it. With the quick wallet funded by the dev faucet: `POST /round` (TEST-ONLY) leaves a round
// selling whose predecessor is drawn; a ticket bought in it; the
// TEST-ONLY `POST /mine?to=deadline` mines past T; the round closed from its S4 panel; `POST /mine?to=void` mines past
// D + draw_window; the panel says "Void is due"; pressed and accepted (C_draw paid, the marker's reserve back to
// its funder, this wallet); the round void on S3 (status and "No draw: void") and on the ticket's card on S2 (D96:
// no winners; the deposit back by return_deposit()).
import path from 'node:path';
import { expect, test } from '@playwright/test';
import { buyAndWait, fundedQuickWallet, shots, ticketOf } from './helpers';
import { detail, devPost, pressCaller, statusIs } from './roundsHelpers';

// On request only (owner, DECISIONS 2026-10-27): `npm run e2e:void` (E2E_VOID=1), never in a plain `npm run e2e`.
test.skip(process.env.E2E_VOID !== '1', 'the void e2e runs on request: npm run e2e:void');

test('a void: the open round closed, D + draw_window mined, voided from its panel; void on S3 and on S2', async ({
  browser,
}) => {
  test.setTimeout(100 * 60_000);
  const w = await fundedQuickWallet(browser);
  const page = w.page;
  // Whatever the suites before left (a round past its deadline, a round closing), the TEST-ONLY `POST /round` draws
  // the closing round, closes and draws the open one, and leaves the next one selling with its predecessor drawn.
  await devPost('/round?main=1,2,3,4,5&bonus=1', 15 * 60_000);
  await ticketOf(page, 1);
  const ticket = await buyAndWait(page);
  await expect(page.getByTestId('round-card')).toHaveAttribute('data-round', /^\d+$/, { timeout: 60_000 });
  const r = (await page.getByTestId('round-card').getAttribute('data-round')) ?? '';
  console.log(`round ${r} ticket: ${ticket}`);
  await devPost('/mine?to=deadline', 5 * 60_000);
  await detail(page, r);
  console.log(`close() of round ${r}: ${await pressCaller(page, 'close')}`);
  await statusIs(page, r, 'closing');
  const t0 = Date.now();
  await devPost('/mine?to=void');
  console.log(`void window mined in ${Math.round((Date.now() - t0) / 1000)} s`);
  await detail(page, r);
  await expect(page.getByTestId('caller')).toHaveAttribute('data-action', 'void', { timeout: 120_000 });
  await expect(page.getByTestId('caller-status')).toHaveText('Void is due');
  console.log(`void of round ${r}: ${await pressCaller(page, 'void')}`);
  await statusIs(page, r, 'void', 300_000);
  await expect(page.getByTestId('round-void-status')).toHaveText('No draw: void');
  await page.screenshot({ path: path.join(shots, 'f5-round-void.png'), fullPage: true });
  await page.goto('/#/rounds');
  await expect(page.getByTestId(`round-row-${r}`)).toHaveAttribute('data-status', 'void', { timeout: 120_000 });
  await expect
    .poll(
      async () => {
        await page.goto('/#/tickets');
        await expect(page.getByTestId('tickets')).toHaveAttribute('data-state', /ready|empty/, { timeout: 60_000 });
        return page.locator(`article[data-key="${ticket}:1"]`).getAttribute('data-state');
      },
      { timeout: 300_000, intervals: [3_000, 6_000] },
    )
    .toBe('void');
  await page.screenshot({ path: path.join(shots, 'f5-ticket-void.png'), fullPage: true });
  await w.ctx.close();
});
