// F3 part A smoke run against the simnet dev run (spec 8): the round card shows real chain data (checked
// against the indexer API the dev run serves), both themes, the remembered theme switch, axe-core with no
// serious or critical issue on what exists, and every interactive element named with a data-testid.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const web = path.dirname(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')));
const shots = path.join(web, 'test-results', 'shots');

interface DevRun {
  api: string;
  covenantId: string;
  network: string;
}

function devRun(): DevRun {
  const p = path.join(web, '..', 'crates', 'target', 'simnet-dev', 'current.json');
  const text = readFileSync(p, 'utf8');
  return JSON.parse(text) as DevRun;
}

async function roundCardReady(page: Page) {
  const card = page.getByTestId('round-card');
  await expect(card).toHaveAttribute('data-state', /ready|stale/);
  return card;
}

async function axe(page: Page, label: string) {
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
async function namedAndTagged(page: Page) {
  const missing = await page.evaluate(() => {
    const sel = 'button, a[href], input, select, textarea, [role="menuitem"], [role="menuitemradio"], [role="tab"]';
    const out: string[] = [];
    for (const el of Array.from(document.querySelectorAll<HTMLElement>(sel))) {
      const labelled = el.getAttribute('aria-labelledby');
      const name =
        el.getAttribute('aria-label') ??
        (labelled
          ? labelled
              .split(' ')
              .map((id) => document.getElementById(id)?.textContent ?? '')
              .join(' ')
          : el.textContent);
      if (!el.dataset.testid) out.push(`no data-testid: ${el.outerHTML.slice(0, 120)}`);
      if (!name || !name.trim()) out.push(`no name: ${el.outerHTML.slice(0, 120)}`);
    }
    return out;
  });
  expect(missing).toEqual([]);
}

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

// Chromium answers `prefers-color-scheme: light` when no preference is set (checked: emulated no-preference
// matches light), so "no preference shows dark" (D139) is covered by the unit test of resolveTheme and by
// theme-boot.js's rule (dark unless the device asks for light); here the device's two answers.
test('the device setting picks the theme, both drawn from the tokens', async ({ browser }) => {
  for (const [scheme, theme, ground] of [
    ['dark', 'dark', 'rgb(14, 16, 18)'],
    ['light', 'light', 'rgb(246, 246, 243)'],
  ] as const) {
    const ctx = await browser.newContext({ colorScheme: scheme, viewport: { width: 390, height: 844 } });
    const page = await ctx.newPage();
    await page.goto('/');
    await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
    expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe(ground);
    await roundCardReady(page);
    await axe(page, `play ${scheme}`);
    if (scheme === 'light') await page.screenshot({ path: path.join(shots, 'play-light.png'), fullPage: true });
    await ctx.close();
  }
});

test('the menu switch overrides the device setting and is remembered', async ({ browser }) => {
  const ctx = await browser.newContext({ colorScheme: 'light', viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByTestId('menu-button').click();
  await expect(page.getByTestId('menu')).toBeVisible();
  await axe(page, 'menu open');
  await namedAndTagged(page);
  await page.getByTestId('theme-dark').click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByTestId('menu-button').click();
  await page.getByTestId('theme-device').click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
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
