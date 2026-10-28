// F6 part A end to end on the private simnet (spec 8: "vault deposit, withdrawal and settle"), with the quick wallet
// funded by the dev faucet. In order, each step through the page's own S5 Vault (and its S4 panel for one draw):
// 1. The dev faucet's TEST-ONLY `POST /close` leaves a known state (the closing round drawn, the open one closed);
//    `POST /mine?to=vault` mines past the vault's VAULT_REQ_DELAY. S5: the vault's figures from the chain; a deposit
//    request of 25 KAS (its rows and terms bound to the built transaction) accepted; the request pending.
// 2. `POST /close` draws the request's round (its draw converts the deposit, a settlement record is written). The
//    request is settle due; Settle mints the wallet's position.
// 3. A part of the position is withdrawn (the least and most from the wasm) and a second deposit of 10 KAS made; the
//    page's own S4 panel draws their round (posting the settlement record's reserve); both are settled: the deposit
//    into a second position, the withdrawal paid to the wallet; the last settlement drains the record and its
//    reserve comes back to this wallet (it funded the record).
// The layout and AX checks run with a deposit built, a request settle due and the withdrawal form open, in both
// themes. Refund mode is not reachable here: it needs a share price above MIN_DEPOSIT sompi (10^9 x the genesis
// price) or a mint above SHARES_MAX (design 5.4, D84), neither of which a simnet instance can reach; the Rust and
// Vitest tests settle a refund-mode record (`settle refund`).
import { expect, test, type Page } from '@playwright/test';
import { fundedQuickWallet, type QuickWallet } from './helpers';
import { detail, devPost, pressCaller, quality } from './roundsHelpers';

test.describe.configure({ mode: 'serial' });

let wallet: QuickWallet | undefined;
/** The round `POST /close` left closing (its draw converts the requests made after it). */
let closing = '';

test.afterAll(async () => {
  await wallet?.ctx.close();
});

/** `POST /close`, retried while the dev wallet's coin is not confirmed yet. */
async function devClose(): Promise<string> {
  for (let i = 0; ; i++) {
    try {
      const r = (await devPost('/close', 5 * 60_000)) as { round: string };
      return r.round;
    } catch (e) {
      if (i >= 4) throw e;
      await new Promise((ok) => setTimeout(ok, 20_000));
    }
  }
}

async function vault(page: Page) {
  await page.goto('about:blank');
  await page.goto('/#/vault');
  await expect(page.getByTestId('vault')).toHaveAttribute('data-state', 'ready', { timeout: 90_000 });
}

/** Wait until the vault takes a request (VAULT_REQ_DELAY after its last spend). */
async function requestOpen(page: Page) {
  await expect
    .poll(
      async () => {
        // Asked again on every look: the dev run's index may not have seen the vault the last request moved.
        await devPost('/mine?to=vault', 5 * 60_000);
        await vault(page);
        return page.getByTestId('vault').getAttribute('data-request-open');
      },
      { timeout: 120_000, intervals: [2_000, 4_000] },
    )
    .toBe('true');
}

async function lastAccepted(page: Page, action: string): Promise<string> {
  const last = page.getByTestId('vault-last');
  await expect(last).toHaveAttribute('data-action', action, { timeout: 30_000 });
  await expect(last).toHaveAttribute('data-step', 'accepted', { timeout: 180_000 });
  return (await page.getByTestId('vault-last-txid').getAttribute('data-value')) ?? '';
}

async function deposit(page: Page, kas: string): Promise<string> {
  await page.getByTestId('deposit-amount').fill(kas);
  await expect(page.getByTestId('deposit-total')).toHaveAttribute('data-sompi', /^[1-9]\d*$/, { timeout: 90_000 });
  const amount = BigInt((await page.getByTestId('deposit-deposit').getAttribute('data-sompi')) ?? '0');
  const reserve = BigInt((await page.getByTestId('deposit-reserve').getAttribute('data-sompi')) ?? '0');
  const fee = BigInt((await page.getByTestId('deposit-fee').getAttribute('data-sompi')) ?? '0');
  const total = BigInt((await page.getByTestId('deposit-total').getAttribute('data-sompi')) ?? '0');
  expect(amount).toBe(BigInt(kas) * 100_000_000n);
  expect(amount + reserve + fee, 'the rows add up to the total').toBe(total);
  await expect(page.getByTestId('deposit-terms-revocable')).toHaveText('No');
  await expect(page.getByTestId('deposit-terms-refund')).toHaveText('Possible');
  await expect(page.getByTestId('deposit-owner')).toHaveAttribute('data-value', wallet?.address ?? '');
  console.log(`deposit ${kas} KAS: reserve ${reserve}, fee ${fee}, total ${total}`);
  await page.getByTestId('deposit-button').click();
  return lastAccepted(page, 'deposit');
}

/** Press Settle on the first settle-due request; the settlement's mode. */
async function settleOne(page: Page): Promise<{ mode: string; id: string }> {
  const card = page.locator('[data-testid^="vault-request-"][data-state="settle-due"]').first();
  const id = (await card.getAttribute('data-testid')) ?? '';
  const rows = page.getByTestId(`${id}-settle`);
  await expect(page.getByTestId(`${id}-settle-fee`)).toHaveAttribute('data-sompi', /^[1-9]\d*$/, { timeout: 90_000 });
  const mode = (await rows.getAttribute('data-mode')) ?? '';
  console.log(
    `settle ${id} (${mode}): fee ${await page.getByTestId(`${id}-settle-fee`).getAttribute('data-sompi')}, total ${await page.getByTestId(`${id}-settle-total`).getAttribute('data-sompi')}`,
  );
  await page.getByTestId(`${id}-settle-button`).click();
  return { mode, id: await lastAccepted(page, 'settle') };
}

test('S5: the vault from the chain; a deposit request accepted and pending; layout and AX', async ({ browser }) => {
  test.setTimeout(15 * 60_000);
  closing = await devClose();
  wallet = await fundedQuickWallet(browser);
  const page = wallet.page;
  await requestOpen(page);
  await expect(page.getByTestId('vault-figures-price')).toHaveAttribute('data-value', /^\d+\.\d{8}$/);
  await expect(page.getByTestId('vault-figures-draw')).toHaveAttribute('data-value', closing);
  await expect(page.getByTestId('vault-positions-count')).toHaveText('0');
  await expect(page.getByTestId('vault-requests-count')).toHaveText('0');
  await page.getByTestId('deposit-amount').fill('25');
  await expect(page.getByTestId('deposit-total')).toHaveAttribute('data-sompi', /^[1-9]\d*$/, { timeout: 90_000 });
  await quality(page, 'f6-vault-deposit');
  const id = await deposit(page, '25');
  console.log(`deposit request: ${id}`);
  await vault(page);
  const card = page.getByTestId('vault-request-0');
  await expect(card).toHaveAttribute('data-state', 'pending', { timeout: 60_000 });
  await expect(page.getByTestId('vault-request-0-price')).toHaveAttribute('data-value', closing);
  await expect(page.getByTestId('vault-request-0-amount')).toHaveAttribute('data-sompi', '2500000000');
  // The vault moved: the next request waits VAULT_REQ_DELAY.
  await expect(page.getByTestId('vault')).toHaveAttribute('data-request-open', 'false');
  await expect(page.getByTestId('vault-figures-deposits')).toHaveAttribute('data-sompi', '2500000000');
});

test('a draw converts the deposit; Settle mints the position', async () => {
  test.setTimeout(15 * 60_000);
  const page = (wallet as QuickWallet).page;
  const next = await devClose();
  console.log(`round ${closing} drawn (the deposit's), round ${next} closing`);
  closing = next;
  await vault(page);
  await expect(page.getByTestId('vault-request-0')).toHaveAttribute('data-state', 'settle-due', { timeout: 90_000 });
  await expect(page.getByTestId('vault-request-0-settle')).toHaveAttribute('data-mode', 'convert');
  await quality(page, 'f6-vault-settle-due');
  const s = await settleOne(page);
  expect(s.mode).toBe('convert');
  await vault(page);
  await expect(page.getByTestId('vault-position-0')).toHaveAttribute('data-state', 'position', { timeout: 90_000 });
  await expect(page.getByTestId('vault-position-0-worth')).toHaveAttribute('data-sompi', /^2[45]\d{8}$/);
  await expect(page.getByTestId('vault-requests-count')).toHaveText('0');
});

test('a part withdrawn and a second deposit; the page draws their round; both settled', async () => {
  test.setTimeout(30 * 60_000);
  const page = (wallet as QuickWallet).page;
  await requestOpen(page);
  // The withdrawal: a part between the least and most the wasm allows.
  await page.getByTestId('vault-position-0-withdraw').click();
  await page.getByTestId('vault-position-0-withdraw-panel-part').click();
  const least = BigInt(
    (await page.getByTestId('vault-position-0-withdraw-panel-least').getAttribute('data-sompi')) ?? '0',
  );
  const most = BigInt(
    (await page.getByTestId('vault-position-0-withdraw-panel-most').getAttribute('data-sompi')) ?? '0',
  );
  expect(least > 0n && most >= least).toBe(true);
  // Outside the range: refused by the wasm, nothing built.
  await page.getByTestId('vault-position-0-withdraw-panel-amount').fill('1');
  await expect(page.getByTestId('vault-position-0-withdraw-panel-error')).toBeVisible({ timeout: 60_000 });
  await expect(page.getByTestId('vault-position-0-withdraw-panel-button')).toBeDisabled();
  const kas = String((least + most) / 2n / 100_000_000n);
  await page.getByTestId('vault-position-0-withdraw-panel-amount').fill(kas);
  await expect(page.getByTestId('vault-position-0-withdraw-panel-total')).toHaveAttribute('data-sompi', /^[1-9]\d*$/, {
    timeout: 90_000,
  });
  await expect(page.getByTestId('vault-position-0-withdraw-panel-left')).toBeVisible();
  await expect(page.getByTestId('vault-position-0-withdraw-panel-terms-revocable')).toHaveText('No');
  await quality(page, 'f6-vault-withdraw');
  await page.getByTestId('vault-position-0-withdraw-panel-button').click();
  console.log(`withdrawal request of ${kas} KAS: ${await lastAccepted(page, 'withdraw')}`);
  // The second deposit, once the vault takes a request again.
  await requestOpen(page);
  console.log(`deposit request: ${await deposit(page, '10')}`);
  await vault(page);
  await expect(page.locator('[data-testid^="vault-request-"][data-state="pending"]')).toHaveCount(2, {
    timeout: 90_000,
  });
  // The page's own draw of their round: it posts the settlement record's reserve.
  await devPost('/mine?to=anchor', 5 * 60_000);
  await detail(page, closing);
  await expect(page.getByTestId('caller')).toHaveAttribute('data-due', 'due', { timeout: 120_000 });
  await expect(page.getByTestId('caller-record-back')).toHaveText('Settlement', { timeout: 120_000 });
  console.log(`draw() of round ${closing}: ${await pressCaller(page, 'draw')}`);
  // Both settled: the deposit into a second position, the withdrawal paid out.
  await vault(page);
  await expect(page.locator('[data-testid^="vault-request-"][data-state="settle-due"]')).toHaveCount(2, {
    timeout: 120_000,
  });
  const modes: string[] = [];
  for (let i = 0; i < 2; i++) {
    await vault(page);
    await expect(page.locator('[data-testid^="vault-request-"][data-state="settle-due"]')).toHaveCount(2 - i, {
      timeout: 90_000,
    });
    if (i === 1) {
      // The last settlement drains the record: its reserve comes back to this wallet, which funded it.
      const card = page.locator('[data-testid^="vault-request-"][data-state="settle-due"]').first();
      const id = (await card.getAttribute('data-testid')) ?? '';
      await expect(page.getByTestId(`${id}-settle-record-to`)).toHaveAttribute('data-value', wallet?.address ?? '');
    }
    modes.push((await settleOne(page)).mode);
  }
  expect(modes.sort()).toEqual(['convert', 'final']);
  await vault(page);
  await expect(page.locator('[data-testid^="vault-position-"][data-state="position"]')).toHaveCount(2, {
    timeout: 90_000,
  });
  await expect(page.getByTestId('vault-requests-count')).toHaveText('0');
});
