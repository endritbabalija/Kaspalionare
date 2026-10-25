// The base components' states, names and test ids (spec 1.1 AX, 4).
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { Amount } from './Amount';
import { BonusTile } from './BonusTile';
import { CostPanel } from './CostPanel';
import { JackpotCountdown } from './JackpotCountdown';
import { jackpotState } from './jackpotState';
import { NumberBall } from './NumberBall';
import { RoundStatusPill } from './RoundStatusPill';
import { TxProgress } from './TxProgress';

afterEach(cleanup);

describe('NumberBall', () => {
  it('is a named toggle in every state', () => {
    for (const s of ['free', 'picked', 'matched', 'full', 'disabled'] as const) {
      render(<NumberBall n={7} state={s} />);
      const b = screen.getByTestId('number-ball-7');
      expect(b.dataset.state).toBe(s);
      expect(b.getAttribute('aria-pressed')).toBe(s === 'picked' || s === 'matched' ? 'true' : 'false');
      expect((b as HTMLButtonElement).disabled).toBe(s === 'full' || s === 'disabled');
      expect(b.getAttribute('aria-label')).toBe(s === 'matched' ? '7, matched' : '7');
      expect(b.textContent).toBe('07');
      cleanup();
    }
  });
});

describe('BonusTile and status pill', () => {
  it('names a matched bonus and shows the status as text', () => {
    render(<BonusTile n={3} state="matched" />);
    expect(screen.getByRole('button', { name: 'Bonus 3, matched' })).toBeTruthy();
    render(<RoundStatusPill status="closing" />);
    expect(screen.getByTestId('round-status').textContent).toBe('Closing');
  });
});

describe('Amount', () => {
  it('shows KAS with 2 decimals and exact sompi on tap', () => {
    render(<Amount sompi="4820000000000" />);
    const b = screen.getByTestId('amount');
    expect(b.textContent).toBe('48,200.00 KAS');
    fireEvent.click(b);
    expect(b.textContent).toBe('4,820,000,000,000 sompi');
    expect(b.getAttribute('aria-pressed')).toBe('true');
  });
});

describe('JackpotCountdown', () => {
  it('is live before T, passed after it, stale when told', () => {
    expect(jackpotState(2000, 1000, false)).toBe('live');
    expect(jackpotState(2000, 2000, false)).toBe('passed');
    expect(jackpotState(2000, 1000, true)).toBe('stale');
    const now = 1_000_000;
    render(<JackpotCountdown jackpot="100" deadline="1600" virtualDaa="1000" observedAt={now} now={now} />);
    expect(screen.getByTestId('countdown').textContent).toBe('00:01:00');
    cleanup();
    render(
      <JackpotCountdown jackpot="100" deadline="900" virtualDaa="1000" observedAt={now} now={now} runHref="#/rounds" />,
    );
    expect(screen.getByTestId('jackpot-block').dataset.state).toBe('passed');
    expect(screen.getByTestId('run-round-link').getAttribute('href')).toBe('#/rounds');
  });
});

describe('TxProgress', () => {
  it('marks steps done, the failed step and rebuilds', () => {
    render(<TxProgress step="signed" failed="The network node refused the request." rebuilt={{ tries: 2, max: 3 }} />);
    expect(screen.getByTestId('tx-step-signed').getAttribute('aria-current')).toBe('step');
    expect(screen.getByTestId('tx-failed').textContent).toBe('The network node refused the request.');
    expect(screen.getByTestId('tx-rebuilt').textContent).toBe('Rebuilt 2/3');
    expect(screen.getByTestId('tx-step-sent').textContent).toContain('failed');
  });
});

describe('CostPanel', () => {
  it('has its four states', () => {
    const rows = [{ id: 'price', name: 'Tickets', sompi: '100000000' }];
    for (const s of ['empty', 'estimating', 'ready', 'fee-too-high'] as const) {
      render(<CostPanel state={s} rows={rows} total="113000000" warning="The fee is larger than the prize." />);
      const p = screen.getByTestId('cost-panel');
      expect(p.dataset.state).toBe(s);
      expect(p.getAttribute('aria-busy')).toBe(s === 'estimating' ? 'true' : 'false');
      expect(screen.queryByTestId('cost-warning') !== null).toBe(s === 'fee-too-high');
      if (s === 'ready') expect(screen.getByTestId('cost-total').textContent).toBe('1.13 KAS');
      cleanup();
    }
  });
});
