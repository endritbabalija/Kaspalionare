// Shared by the F5 suites (rounds.spec.ts, void.spec.ts): the dev run's TEST-ONLY tools, the round page, Verify and
// the S4 panel as the e2e tests drive them.
import path from 'node:path';
import { expect, request, type Page } from '@playwright/test';
import { axe, devRun, layoutHolds, namedAndTagged, shots } from './helpers';

export interface Run {
  api: string;
  covenantId: string;
  faucet: string;
  store: string;
}

export function run(): Run {
  return devRun() as unknown as Run;
}

/** A TEST-ONLY dev-run tool (`POST /mine?to=…`, simnet binary only). */
export async function devPost(p: string, timeout = 90 * 60_000): Promise<unknown> {
  const base = run().faucet.replace(/\/faucet$/, '');
  const ctx = await request.newContext({ timeout });
  try {
    const r = await ctx.post(`${base}${p}`, { timeout });
    const body = await r.text();
    expect(r.ok(), `${p}: ${body}`).toBe(true);
    console.log(`dev ${p}: ${body}`);
    return JSON.parse(body) as unknown;
  } finally {
    await ctx.dispose();
  }
}

export async function quality(page: Page, label: string) {
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

export async function detail(page: Page, r: string) {
  await page.goto(`/#/rounds?r=${r}`);
  await expect(page.getByTestId('round-detail')).toHaveAttribute('data-state', /ready|error/, { timeout: 90_000 });
}

export async function verify(page: Page) {
  await page.getByTestId('verify-button').click();
  await expect(page.getByTestId('verify')).toHaveAttribute('data-state', /done|error/, { timeout: 120_000 });
  return page.getByTestId('verify');
}

/** The results of each kind of check, from the panel. */
export async function results(page: Page): Promise<{ kind: string; result: string; shard: string }[]> {
  return page
    .getByTestId('verify-items')
    .locator('li')
    .evaluateAll((els) =>
      els.map((e) => ({
        kind: (e as HTMLElement).dataset.kind ?? '',
        result: (e as HTMLElement).dataset.result ?? '',
        shard: (e as HTMLElement).dataset.shard ?? '',
      })),
    );
}

/** Press the panel's button once the build's rows are shown; wait for the outcome. */
export async function pressCaller(page: Page, action: string): Promise<string> {
  const panel = page.getByTestId('caller');
  await expect(panel).toHaveAttribute('data-due', 'due', { timeout: 120_000 });
  await expect(panel).toHaveAttribute('data-action', action);
  await expect(page.getByTestId('caller-fee')).toHaveAttribute('data-sompi', /^[1-9]\d*$/, { timeout: 120_000 });
  await expect(page.getByTestId('caller-total')).toHaveAttribute('data-sompi', /^-?\d+$/);
  console.log(
    `${action}: reward ${await page.getByTestId('caller-reward').getAttribute('data-sompi')}, fee ${await page
      .getByTestId('caller-fee')
      .getAttribute('data-sompi')}, total ${await page.getByTestId('caller-total').getAttribute('data-sompi')}`,
  );
  await page.getByTestId('caller-button').click();
  await expect(page.getByTestId('caller-progress').getByTestId('tx-progress')).toHaveAttribute(
    'data-step',
    'accepted',
    {
      timeout: 180_000,
    },
  );
  return (await page.getByTestId('caller-txid').getAttribute('data-value')) ?? '';
}

export async function statusIs(page: Page, r: string, status: string, timeout = 180_000) {
  await expect
    .poll(
      async () => {
        await detail(page, r);
        return page.getByTestId('round-detail').getAttribute('data-status');
      },
      { timeout, intervals: [2_000, 4_000, 8_000] },
    )
    .toBe(status);
}
