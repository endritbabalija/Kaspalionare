// S6 How it works (F6 part B): the page's numbers are the model's (the Rust lottery code's), the open D100 item and
// the testnet line are stated, the honesty wording rule holds (spec 7), the figure frame follows the theme and takes
// its height only from its own frame, and every control is named and tagged.
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { FigureFrame } from '@/components/FigureFrame';
import { FIGURE_HEIGHT, figureHeight } from '@/components/figureHeight';
import { MoneySplit } from '@/components/MoneySplit';
import { HowItWorks } from '@/screens/HowItWorks';
import model from './cabinet-model.json';

afterEach(cleanup);

describe('How it works', () => {
  it('prints the example round from the model and states the open items', () => {
    render(<HowItWorks theme="dark" />);
    const text = screen.getByTestId('how').textContent ?? '';
    expect(screen.getByTestId('how-example-sales-amount').getAttribute('data-sompi')).toBe(model.sales);
    expect(screen.getByTestId('how-example-jackpot-amount').getAttribute('data-sompi')).toBe(model.vault.J);
    expect(screen.getByTestId('how-example-prize-amount').getAttribute('data-sompi')).toBe(model.prize);
    expect(screen.getByTestId('how-split-house-amount').getAttribute('data-sompi')).toBe(model.perTicket.H);
    expect(screen.getByTestId('how-split-tiers-amount').getAttribute('data-sompi')).toBe(model.perTicket.Bsum);
    expect(screen.getByTestId('how-example-numbers').textContent).toContain(
      `${model.win.map((n) => String(n).padStart(2, '0')).join(' ')} + ${String(model.bonus).padStart(2, '0')}`,
    );
    expect(screen.getByTestId('how-open-item').textContent).toMatch(/could throw the block away/);
    expect(screen.getByTestId('how-vault-indexer').textContent).toMatch(/only your key can move it/);
    expect(text).toMatch(/testnet-10\. Its coins have no value/);
    expect(text).toMatch(/at least 7 days after the draw/);
    expect(text).not.toMatch(/provably fair|guarantee/i);
    // every control named and tagged
    for (const el of Array.from(document.querySelectorAll<HTMLElement>('button, a[href], input, iframe'))) {
      expect(el.dataset.testid, el.outerHTML.slice(0, 80)).toBeTruthy();
      expect(el.getAttribute('title') ?? el.textContent, el.outerHTML.slice(0, 80)).toBeTruthy();
    }
  });

  it('frames the figure of the page theme, sandboxed to scripts only', () => {
    const { rerender } = render(<HowItWorks theme="dark" />);
    const frame = () => screen.getByTestId('how-figure-frame');
    expect(frame().getAttribute('src')).toBe('/figures/draw-cabinet-tour.html');
    expect(frame().getAttribute('sandbox')).toBe('allow-scripts');
    rerender(<HowItWorks theme="light" />);
    expect(frame().getAttribute('src')).toBe('/figures/draw-cabinet-tour-light.html');
  });
});

describe('Figure frame', () => {
  it('reads only its own figure message, bounded', () => {
    expect(figureHeight({ figure: 'draw-cabinet', height: 812.4 }, 'draw-cabinet')).toBe(812);
    expect(figureHeight({ figure: 'draw-cabinet', height: 10 }, 'draw-cabinet')).toBe(FIGURE_HEIGHT.min);
    expect(figureHeight({ figure: 'draw-cabinet', height: 1e9 }, 'draw-cabinet')).toBe(FIGURE_HEIGHT.max);
    expect(figureHeight({ figure: 'other', height: 500 }, 'draw-cabinet')).toBeNull();
    expect(figureHeight({ figure: 'draw-cabinet', height: '500' }, 'draw-cabinet')).toBeNull();
    expect(figureHeight({ figure: 'draw-cabinet', height: Number.NaN }, 'draw-cabinet')).toBeNull();
    expect(figureHeight('500', 'draw-cabinet')).toBeNull();
  });

  it('takes the height from its own frame only', () => {
    render(<FigureFrame src="/figures/x.html" figure="draw-cabinet" title="X" testId="f" />);
    const el = screen.getByTestId('f') as HTMLIFrameElement;
    expect(el.dataset.state).toBe('loading');
    act(() => {
      window.dispatchEvent(
        new MessageEvent('message', { data: { figure: 'draw-cabinet', height: 900 }, source: window }),
      );
    });
    expect(el.dataset.state).toBe('loading');
    act(() => {
      window.dispatchEvent(
        new MessageEvent('message', { data: { figure: 'draw-cabinet', height: 900 }, source: el.contentWindow }),
      );
    });
    expect(el.dataset.state).toBe('sized');
    expect(el.style.height).toBe('900px');
  });
});

describe('Money split', () => {
  it('names every part and shows its exact amount; the bar is decoration', () => {
    render(
      <MoneySplit
        testId="s"
        parts={[
          { key: 'a', name: 'House vault', sompi: '74000000', fill: 'bg-teal' },
          { key: 'b', name: 'close() caller', sompi: '500000', fill: 'bg-graphite' },
        ]}
      />,
    );
    expect(screen.getByTestId('s-a').textContent).toContain('House vault');
    expect(screen.getByTestId('s-b-amount').textContent).toBe('0.005 KAS');
    fireEvent.click(screen.getByTestId('s-b-amount'));
    expect(screen.getByTestId('s-b-amount').textContent).toBe('500,000 sompi');
  });
});
