// F5 components (spec 4 "Round row and round detail", "Caller panel"): their states, names and test ids, from the
// catalogue's sample figures shaped as the wasm returns them.
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { SAMPLE_CLOSE, SAMPLE_DRAW_LOW, SAMPLE_VERIFY } from '@/dev/roundSamples';
import { CallerPanel } from './CallerPanel';
import { callerPanelState } from './callerPanelState';
import { RoundRow } from './RoundRow';
import { VerifyPanel } from './VerifyPanel';

afterEach(cleanup);
const clock = { virtualDaa: '13000', observedAt: Date.UTC(2026, 9, 25) };

describe('RoundRow', () => {
  it('shows the chain status as a pill, an unchecked one as Unverified, and marks indexer figures', () => {
    render(
      <>
        <RoundRow round="3" testId="a" />
        <RoundRow
          round="2"
          testId="b"
          data={{ round: '2', status: 'drawn', sales: { value: '100000000', checked: false } }}
        />
        <RoundRow round="1" testId="c" data={{ round: '1', status: null, jackpot: { value: '1', checked: true } }} />
      </>,
    );
    expect(screen.getByTestId('a').dataset.state).toBe('checking');
    expect(screen.getByTestId('b').querySelector('[data-testid="round-status"]')?.textContent).toBe('Drawn');
    expect(screen.getByTestId('b-sales-unverified').textContent).toBe('Unverified');
    expect(screen.getByTestId('c-status').textContent).toBe('Unverified');
    expect(screen.queryByTestId('c-jackpot-unverified')).toBeNull();
    expect(screen.getByTestId('a-link').getAttribute('href')).toBe('#/rounds?r=3');
  });
});

describe('VerifyPanel', () => {
  it('names each check and says its result in words; a mismatch shows the one message', () => {
    render(<VerifyPanel state="done" result={SAMPLE_VERIFY} onVerify={() => undefined} />);
    const items = screen.getAllByRole('listitem');
    expect(items).toHaveLength(SAMPLE_VERIFY.items.length);
    expect(items.map((i) => i.dataset.result)).toEqual(SAMPLE_VERIFY.items.map((i) => i.result));
    expect(items[5]?.textContent).toBe('Shard 1 treeMismatch');
    expect(items[1]?.textContent).toMatch(/^Marker .*:0Not checkable$/);
    expect(screen.getByTestId('verify-mismatch')).toBeTruthy();
    expect(screen.getByTestId('verify-count-match').dataset.value).toBe('5');
    expect(screen.getByRole('button', { name: 'Verify in this browser' })).toBeTruthy();
  });
});

describe('CallerPanel', () => {
  it('not due: from when, and the reward; no button', () => {
    render(
      <CallerPanel
        round="4"
        due={{ kind: 'later', action: 'close', from: '13662' }}
        reward="2410000000"
        walletConnected
        onPress={() => undefined}
        clock={clock}
      />,
    );
    expect(screen.getByTestId('caller').dataset.state).toBe('not-due');
    expect(screen.getByTestId('caller-from').textContent).toContain('DAA 13,662');
    expect(screen.getByTestId('caller-reward').dataset.sompi).toBe('2410000000');
    expect(screen.queryByTestId('caller-button')).toBeNull();
  });

  it('due with reward: the built rows and the press', () => {
    render(
      <CallerPanel
        round="4"
        due={{ kind: 'due', action: 'close', from: '12000' }}
        figures={SAMPLE_CLOSE}
        walletConnected
        onPress={() => undefined}
        clock={clock}
      />,
    );
    expect(screen.getByTestId('caller').dataset.state).toBe('due');
    expect(screen.getByTestId('caller-status').textContent).toBe('close() is due');
    expect(screen.getByTestId('caller-back').textContent).toBe('draw()');
    expect(screen.getByTestId('caller-total').dataset.sompi).toBe(SAMPLE_CLOSE.total);
    expect(screen.queryByTestId('caller-warning')).toBeNull();
    expect(screen.getByRole('button', { name: 'Close round 4' })).toBeTruthy();
  });

  it('reward below fee: the warning row; without a wallet: Connect, no press', () => {
    render(
      <CallerPanel
        round="4"
        due={{ kind: 'due', action: 'draw', from: '12000' }}
        figures={SAMPLE_DRAW_LOW}
        walletConnected={false}
        onPress={() => undefined}
        clock={clock}
      />,
    );
    expect(screen.getByTestId('caller').dataset.state).toBe('reward-below-fee');
    expect(screen.getByTestId('caller-warning')).toBeTruthy();
    expect(screen.getByTestId('caller-refund').dataset.sompi).toBe(SAMPLE_DRAW_LOW.refund);
    // F5 review S1: the expired pools it absorbs; the new pools' reserves back from their claim deadline.
    expect(screen.getByTestId('caller-absorbed').dataset.value).toBe('3');
    expect(screen.getByTestId('caller-back-row')).toBeTruthy();
    expect(screen.queryByTestId('caller-record-back')).toBeNull();
    expect(screen.getByTestId('caller-connect')).toBeTruthy();
    expect(screen.queryByTestId('caller-button')).toBeNull();
  });

  it('states: running while sent, done once accepted, nothing when nothing is due', () => {
    const due = { kind: 'due' as const, action: 'void' as const, from: '1' };
    expect(callerPanelState({ due, tx: { step: 'signed' } })).toBe('running');
    expect(callerPanelState({ due, tx: { step: 'accepted' } })).toBe('done');
    expect(callerPanelState({ due, tx: { step: 'built', failed: 'x' } })).toBe('due');
    expect(callerPanelState({ due: { kind: 'none' } })).toBe('none');
    const { container } = render(
      <CallerPanel round="4" due={{ kind: 'none' }} walletConnected onPress={() => undefined} clock={clock} />,
    );
    expect(container.textContent).toBe('');
  });
});
