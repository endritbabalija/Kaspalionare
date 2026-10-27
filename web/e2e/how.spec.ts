// F6 part B: S6 How it works against the dev run (spec 3 S6, 8 quality, D138). The draw-cabinet figure loads in its
// sandboxed frame only when S6 opens, runs its tour, sizes the frame and follows the page theme; the page holds at
// 360-430 px and on desktop, axe-core finds no serious or critical issue (the figure's frame included) in both
// themes, every control is named and tagged; with reduced motion the figure holds still. Screenshots for the owner
// in test-results/shots/how-*.png.
import path from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import { axe, layoutHolds, namedAndTagged, shots } from './helpers';

const FIGURE = /\/figures\/draw-cabinet-tour(-light)?\.html$/;

async function openHow(page: Page) {
  await page.goto('/#/how');
  await expect(page.getByTestId('how')).toBeVisible();
  const frame = page.getByTestId('how-figure-frame');
  await frame.scrollIntoViewIfNeeded();
  await expect(frame).toHaveAttribute('data-state', 'sized', { timeout: 30_000 });
  const fig = page.frames().find((f) => FIGURE.test(f.url()));
  if (!fig) throw new Error('no figure frame');
  await expect(fig.locator('[data-callout] [data-callout-title]')).not.toHaveText('', { timeout: 15_000 });
  return fig;
}

async function theme(page: Page, t: 'dark' | 'light') {
  await page.getByTestId('menu-button').click();
  await page.getByTestId(`theme-${t}`).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', t);
}

test('the figure loads only on How it works, in its sandboxed frame', async ({ page }) => {
  const asked: string[] = [];
  page.on('request', (r) => {
    if (FIGURE.test(r.url())) asked.push(r.url());
  });
  await page.goto('/');
  await expect(page.getByTestId('round-card')).toBeVisible({ timeout: 90_000 });
  expect(asked, 'no figure before S6 opens').toEqual([]);
  await page.getByTestId('menu-button').click();
  await page.getByTestId('menu-how').click();
  await openHow(page);
  expect(asked.length).toBeGreaterThan(0);
  await expect(page.getByTestId('how-figure-frame')).toHaveAttribute('sandbox', 'allow-scripts');
  await expect(page.getByTestId('how-open-item')).toContainText('throw the block away');
  await expect(page.getByTestId('how')).not.toContainText(/provably fair|guarantee/i);
});

for (const t of ['dark', 'light'] as const) {
  test(`How it works holds in the ${t} theme at 360, 390, 430 px and on desktop`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: t });
    await page.goto('/#/how');
    await theme(page, t);
    const fig = await openHow(page);
    expect(fig.url()).toMatch(t === 'light' ? /-light\.html$/ : /tour\.html$/);
    for (const width of [360, 390, 430, 1280]) {
      await page.setViewportSize({ width, height: 844 });
      await expect(page.getByTestId('how-figure-frame')).toHaveAttribute('data-state', 'sized');
      await layoutHolds(page, `how ${t} ${width}`);
      if (width === 390 || width === 1280) {
        await page.waitForTimeout(600);
        await page.screenshot({ path: path.join(shots, `how-${t}-${width}.png`), fullPage: true });
        await page.getByTestId('how-figure').screenshot({ path: path.join(shots, `how-${t}-${width}-figure.png`) });
      }
    }
    await page.setViewportSize({ width: 390, height: 844 });
    await axe(page, `how ${t}`);
    await namedAndTagged(page);
  });
}

test('with reduced motion the figure holds still on its first stop', async ({ browser }) => {
  const ctx = await browser.newContext({ reducedMotion: 'reduce', viewport: { width: 390, height: 844 } });
  const page = await ctx.newPage();
  const fig = await openHow(page);
  const read = () => fig.locator('.dc-stage > svg.iso-svg').getAttribute('viewBox');
  const a = await read();
  await page.waitForTimeout(2500);
  expect(await read()).toBe(a);
  await ctx.close();
});
