import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { BuyFigures } from '@wasm/web.js';
import type { Prepared, Sales } from '@/play/buyFlow';
import { EMPTY } from '@/play/picker';
import { BuyCard } from './BuyCard';
import type { usePlay } from './usePlay';

type Play = ReturnType<typeof usePlay>;

const sales: Sales = {
  round: '3',
  open: true,
  m: 2,
  deadline: '13662',
  bMax: 4,
  shape: { mainMax: 30, mainPicks: 5, bMax: 4, coinsMax: 8 },
  shards: [],
  virtualDaaScore: '9000',
  observedAt: Date.now(),
};

function play(over: Partial<Play>): Play {
  const noop = () => undefined;
  return {
    sales: { kind: 'ready', data: sales },
    pick: EMPTY,
    ticket: [{ main: [1, 2, 3, 4, 5], bonus: 1 }],
    cost: { kind: 'empty' },
    tx: null,
    busy: false,
    m: 2,
    bMax: 4,
    shape: sales.shape,
    onMain: noop,
    onBonus: noop,
    onQuickPick: noop,
    onClear: noop,
    onAdd: noop,
    onRemove: noop,
    onBuy: () => Promise.resolve(),
    ...over,
  };
}

function figures(paid: boolean): BuyFigures {
  return {
    round: '3',
    shardIndex: 4,
    b: 1,
    price: '100000000',
    deposit: '13000000',
    depositBack: '12109662',
    referral: { amount: paid ? '20000000' : '5000000', paid, address: paid ? 'kaspasim:qqowner' : undefined },
    fee: '32000000',
    total: '145000000',
    ticket: { index: 1, state: '00', scriptPublicKey: '00', address: 'kaspasim:p' },
  };
}

function ready(paid: boolean): Play['cost'] {
  return { kind: 'ready', prepared: { figures: figures(paid), shardIndex: 4 } as unknown as Prepared };
}

afterEach(cleanup);

describe('S1 buy card (spec 3, 7)', () => {
  it('after the deadline: "Sales closed; waiting for close()" and a link to Rounds, no picker', () => {
    render(<BuyCard play={play({ sales: { kind: 'ready', data: { ...sales, open: false } } })} walletConnected />);
    expect(screen.getByTestId('sales-closed-play').textContent).toBe('Sales closed; waiting for close()');
    expect(screen.getByTestId('sales-closed-rounds').getAttribute('href')).toBe('#/rounds');
    expect(screen.queryByTestId('number-picker')).toBeNull();
  });

  it('cost rows come from the built transaction; below M_ref the referral stays in the round', () => {
    render(<BuyCard play={play({ cost: ready(false) })} walletConnected />);
    expect(screen.getByTestId('cost-price').getAttribute('data-sompi')).toBe('100000000');
    expect(screen.getByTestId('cost-deposit').getAttribute('data-sompi')).toBe('13000000');
    expect(screen.getByTestId('deposit-back').textContent).toContain('DAA 12,109,662');
    expect(screen.getByTestId('cost-fee').getAttribute('data-sompi')).toBe('32000000');
    expect(screen.getByTestId('referral-to').getAttribute('data-value')).toBe('round');
    expect(screen.getByTestId('cost-total').getAttribute('data-sompi')).toBe('145000000');
    expect((screen.getByTestId('buy-button') as HTMLButtonElement).disabled).toBe(false);
  });

  it('the top-level cost rows add up to the total; the referral is a part of the price (F2/F3 review)', () => {
    for (const paid of [false, true]) {
      cleanup();
      render(<BuyCard play={play({ cost: ready(paid) })} walletConnected />);
      const rows = screen.getAllByTestId(/^cost-row-/);
      const top = rows.filter((r) => r.getAttribute('data-part') !== 'true').map((r) => r.dataset.testid);
      expect(top).toEqual(['cost-row-price', 'cost-row-deposit', 'cost-row-fee']);
      const parts = rows.filter((r) => r.getAttribute('data-part') === 'true').map((r) => r.dataset.testid);
      expect(parts).toEqual(['cost-row-referral', 'cost-row-deposit-back']);
      const sum = ['cost-price', 'cost-deposit', 'cost-fee']
        .map((id) => BigInt(screen.getByTestId(id).getAttribute('data-sompi') ?? 'x'))
        .reduce((a, b) => a + b, 0n);
      expect(sum.toString()).toBe(screen.getByTestId('cost-total').getAttribute('data-sompi'));
      expect(screen.getByTestId('cost-referral').getAttribute('data-sompi')).toBe(paid ? '20000000' : '5000000');
    }
  });

  it('while a purchase outcome is unknown, Buy stays disabled', () => {
    render(
      <BuyCard
        play={play({
          cost: ready(false),
          tx: { step: 'signed', checking: false, failed: 'x', failedCode: 'submit_unknown' },
        })}
        walletConnected
      />,
    );
    expect((screen.getByTestId('buy-button') as HTMLButtonElement).disabled).toBe(true);
    cleanup();
    render(<BuyCard play={play({ cost: ready(false), tx: { step: 'signed', checking: true } })} walletConnected />);
    expect(screen.getByTestId('tx-checking').textContent).toBe('Checking');
  });

  it('a paid referral names its destination; without a wallet the button connects', () => {
    render(<BuyCard play={play({ cost: ready(true) })} walletConnected />);
    expect(screen.getByTestId('referral-to').getAttribute('data-value')).toBe('kaspasim:qqowner');
    render(<BuyCard play={play({})} walletConnected={false} />);
    expect(screen.getByTestId('buy-connect').getAttribute('href')).toBe('#/wallet');
  });

  it('shows the rebuilds and a failure reason', () => {
    render(
      <BuyCard
        play={play({
          tx: { step: 'signed', failed: 'The wallet changed the transaction.', rebuilt: { tries: 1, max: 3 } },
        })}
        walletConnected
      />,
    );
    expect(screen.getByTestId('tx-rebuilt').textContent).toBe('Rebuilt 1/3');
    expect(screen.getByTestId('tx-failed').textContent).toBe('The wallet changed the transaction.');
  });
});
