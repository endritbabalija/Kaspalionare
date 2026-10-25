// F3 part A smoke run against the simnet dev run (spec 8): the round card shows real chain data (checked
// against the indexer API the dev run serves), both themes, the remembered theme switch, axe-core with no
// serious or critical issue on what exists, and every interactive element named with a data-testid.
import path from 'node:path';
import { expect, test } from '@playwright/test';
import { axe, devRun, namedAndTagged, roundCardReady, shots } from './helpers';

test('the round card shows the open round from the simnet, checked against the node', async ({ page, request }) => {
  const run = devRun();
  const status = await (await request.get(`${run.api}status`)).json();
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.goto('/');
  const card = await roundCardReady(page);
  const round: string = await card.getAttribute('data-round').then((r) => r ?? '');
  expect(Number(round)).toBeGreaterThanOrEqual(Number(status.currentRound));
  await expect(page.getByTestId('round-number')).toHaveText(`Round ${round}`);
  await expect(page.getByTestId('network-badge')).toHaveText(/simnet/i);
  const row = (await (await request.get(`${run.api}rounds`)).json()).rounds.find(
    (x: { round: string }) => x.round === round,
  );
  await expect(page.getByTestId('round-status')).toHaveAttribute('data-status', row.status);
  await expect(page.getByTestId('jackpot')).toHaveAttribute('data-sompi', row.jackpot);
  const odds = Number(await page.getByTestId('jackpot-odds').getAttribute('data-odds'));
  expect(odds).toBe(142_506 * row.m);
  await expect(page.getByTestId('jackpot-odds')).toHaveText(`1 in ${odds.toLocaleString('en-US')}`);
  await expect(page.getByTestId('deadline')).toContainText('DAA');
  // Tap: exact sompi.
  await page.getByTestId('jackpot').click();
  await expect(page.getByTestId('jackpot')).toHaveText(/sompi$/);
  await page.screenshot({ path: path.join(shots, 'play-dark.png'), fullPage: true });
  await namedAndTagged(page);
});

// Owner decision 2026-10-26 (amends D139): a first-time visitor always sees dark, whatever the device's
// colour-scheme setting; "device" is a menu choice that follows prefers-color-scheme; the choice is remembered.
test('a first-time visitor sees dark under either device setting; both themes drawn from the tokens', async ({
  browser,
}) => {
  for (const scheme of ['dark', 'light'] as const) {
    const ctx = await browser.newContext({ colorScheme: scheme, viewport: { width: 390, height: 844 } });
    const page = await ctx.newPage();
    await page.goto('/');
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe('rgb(14, 16, 18)');
    await roundCardReady(page);
    await axe(page, `play dark (device ${scheme})`);
    await ctx.close();
  }
  const ctx = await browser.newContext({ colorScheme: 'dark', viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  await page.goto('/');
  await page.getByTestId('menu-button').click();
  await page.getByTestId('theme-light').click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe('rgb(246, 246, 243)');
  await roundCardReady(page);
  await axe(page, 'play light');
  await page.screenshot({ path: path.join(shots, 'play-light.png'), fullPage: true });
  await ctx.close();
});

test('the menu switch overrides the first-visit dark and is remembered; device follows the device', async ({
  browser,
}) => {
  const ctx = await browser.newContext({ colorScheme: 'light', viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByTestId('menu-button').click();
  await expect(page.getByTestId('menu')).toBeVisible();
  await expect(page.getByTestId('theme-dark')).toHaveAttribute('aria-checked', 'true');
  await axe(page, 'menu open');
  await namedAndTagged(page);
  await page.getByTestId('theme-light').click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByTestId('menu-button').click();
  await page.getByTestId('theme-device').click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByTestId('menu-button').click();
  await page.getByTestId('theme-dark').click();
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await ctx.close();
});

test('the shell: bottom navigation, menu screens and the dev catalogue', async ({ page }) => {
  await page.goto('/');
  for (const [nav, screen] of [
    ['nav-tickets', 'screen-tickets'],
    ['nav-rounds', 'screen-rounds'],
    ['nav-play', 'screen-play'],
  ] as const) {
    await page.getByTestId(nav).click();
    await expect(page.getByTestId(screen)).toBeVisible();
    await expect(page.getByTestId(nav)).toHaveAttribute('aria-current', 'page');
    await expect(page.getByTestId('network-badge')).toBeVisible();
  }
  for (const item of ['vault', 'how', 'backup', 'wallet'] as const) {
    await page.getByTestId('menu-button').click();
    await page.getByTestId(`menu-${item}`).click();
    await expect(page.getByTestId(`screen-${item}`)).toBeVisible();
    await expect(page.getByTestId('network-badge')).toBeVisible();
  }
  await page.goto('/#/dev/catalogue');
  await expect(page.getByTestId('catalogue')).toBeVisible();
  await expect(page.getByTestId('catalogue-light').getByTestId('number-ball-17')).toHaveAttribute(
    'data-state',
    'matched',
  );
  await axe(page, 'catalogue');
  await namedAndTagged(page);
  await page.screenshot({ path: path.join(shots, 'catalogue.png'), fullPage: true });
});
