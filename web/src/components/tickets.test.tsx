// F4 components (spec 4): the ticket card's, the claim row's and the local-copy box's states, names and test
// ids, from sample figures shaped as the wasm returns them.
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { claimView, sampleCard, sampleFigures, sampleStored, SAMPLE_TX } from '@/dev/ticketSamples';
import { BallRow } from './BallRow';
import { ClaimRow } from './ClaimRow';
import { LocalCopyBox } from './LocalCopyBox';
import { TicketCard } from './TicketCard';
import { claimRowState, ticketCardState } from './ticketStates';

afterEach(cleanup);
const clock = { virtualDaa: '13000', observedAt: Date.UTC(2026, 9, 25) };

describe('BallRow', () => {
  it('marks matched balls for screen readers, not by colour alone', () => {
    render(
      <BallRow
        combination={{ main: [3, 7, 12, 19, 28], bonus: 2 }}
        matchedMain={[true, false, false, true, true]}
        bonusMatched
      />,
    );
    expect(screen.getByTestId('ball-row-main-0').textContent).toBe('03, matched');
    expect(screen.getByTestId('ball-row-main-1').textContent).toBe('07');
    expect(screen.getByTestId('ball-row-bonus').textContent).toBe('Bonus 2, matched');
  });
});

describe('ClaimRow', () => {
  it('has the five states of spec 4', () => {
    const pay = claimView('pay', '1240000000', '2');
    expect(claimRowState({ claim: pay, figures: sampleFigures(pay, '25019700') })).toBe('claimable');
    expect(claimRowState({ claim: pay, tx: { step: 'signed' } })).toBe('claiming');
    expect(claimRowState({ claim: pay, tx: { step: 'signed', failed: 'x' } })).toBe('claimable');
    expect(claimRowState({ claimed: true })).toBe('claimed');
    expect(claimRowState({ claim: pay, tx: { step: 'accepted' } })).toBe('claimed');
    expect(claimRowState({ claim: claimView('accrue', '2240000') })).toBe('accrued');
    const fin = claimView('final', '2240000');
    expect(claimRowState({ claim: fin, figures: sampleFigures(fin, '14474100') })).toBe('fee-exceeds');
  });

  it('shows the prize, the others it is shared with, when it is paid and the fee; Claim signs only a build', () => {
    const pay = claimView('pay', '1240000000', '2');
    const { rerender } = render(<ClaimRow testId="c" claim={pay} estimating deadline="6100000" clock={clock} />);
    expect(screen.getByTestId('c-prize').dataset.sompi).toBe('1240000000');
    expect(screen.getByTestId('c-shared').dataset.value).toBe('1');
    expect(screen.getByTestId('c-paid').textContent).toBe('Now');
    expect(screen.getByTestId('c-fee').getAttribute('aria-busy')).toBe('true');
    expect((screen.getByTestId('c-button') as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByTestId('c-button').getAttribute('aria-label')).toBe('Claim 5');
    rerender(<ClaimRow testId="c" claim={pay} figures={sampleFigures(pay, '25019700')} />);
    expect((screen.getByTestId('c-button') as HTMLButtonElement).disabled).toBe(false);
    expect(screen.getByTestId('c-fee').dataset.sompi).toBe('25019700');
    expect(screen.queryByTestId('c-warning')).toBeNull();
    const acc = claimView('accrue', '2240000');
    rerender(<ClaimRow testId="c" claim={acc} figures={sampleFigures(acc, '14474100')} />);
    expect(screen.getByTestId('c-paid').textContent).toBe('With last claim');
    expect(screen.queryByTestId('c-shared')).toBeNull();
    expect(screen.getByTestId('c-warning').getAttribute('role')).toBe('alert');
    const fin = claimView('final', '1240000000');
    rerender(<ClaimRow testId="c" claim={fin} figures={sampleFigures(fin, '6172000', '26000000')} />);
    expect(screen.getByTestId('c-deposit').dataset.sompi).toBe('26000000');
  });
});

describe('TicketCard', () => {
  it('has every state of spec 4', () => {
    const s = sampleStored('12');
    expect(ticketCardState({ stored: s, card: sampleCard('open') })).toBe('open');
    expect(ticketCardState({ stored: s, card: sampleCard('win') })).toBe('win');
    expect(ticketCardState({ stored: s, card: sampleCard('no-win') })).toBe('no-win');
    expect(ticketCardState({ stored: s, card: sampleCard('partly') })).toBe('partly-claimed');
    expect(ticketCardState({ stored: s, card: { ...sampleCard('partly'), combos: sampleCard('no-win').combos } })).toBe(
      'claimed',
    );
    expect(ticketCardState({ stored: s, card: sampleCard('void') })).toBe('void');
    expect(ticketCardState({ stored: s, card: sampleCard('returnable') })).toBe('returnable');
    const ended = (kind: 'claimed' | 'returned') =>
      sampleStored('12', { kind, transactionId: SAMPLE_TX, amount: '1', endedMs: '1' });
    expect(ticketCardState({ stored: ended('claimed') })).toBe('claimed');
    expect(ticketCardState({ stored: ended('returned') })).toBe('returned');
    expect(ticketCardState({ stored: s, spent: true })).toBe('spent');
  });

  it('an open card shows the deadline; a void one says so; a returnable one offers Return deposit', () => {
    const { rerender } = render(
      <TicketCard testId="t" stored={sampleStored('12')} status="open" card={sampleCard('open')} clock={clock} />,
    );
    expect(screen.getByTestId('t-deadline').textContent).toContain('DAA 13,662');
    expect(screen.getByTestId('t-status').textContent).toBe('Open');
    rerender(
      <TicketCard testId="t" stored={sampleStored('12')} status="void" card={sampleCard('void')} clock={clock} />,
    );
    expect(screen.getByTestId('t-status').textContent).toBe('No draw: void');
    expect(screen.getByTestId('t-back').textContent).toContain('DAA 12,109,662');
    rerender(
      <TicketCard
        testId="t"
        stored={sampleStored('12')}
        status="drawn"
        card={sampleCard('returnable')}
        clock={clock}
        ret={{
          figures: {
            round: '12',
            path: 'alone',
            amount: '26000000',
            payout: 'kaspatest:q',
            fee: '5166400',
            lockTime: '1',
          },
        }}
      />,
    );
    expect(screen.getByTestId('t-return-fee').dataset.sompi).toBe('5166400');
    expect((screen.getByTestId('t-return-button') as HTMLButtonElement).disabled).toBe(false);
  });
});

describe('LocalCopyBox', () => {
  it('has the four states of spec 4 and the backup download', () => {
    for (const [s, text] of [
      ['complete', 'Complete and verified'],
      ['incomplete', 'Incomplete'],
      ['mismatch', 'Mismatch'],
      ['not-needed', 'Not needed'],
    ] as const) {
      render(<LocalCopyBox state={s} rounds={1} tickets={2} onDownload={() => undefined} />);
      expect(screen.getByTestId('local-copy-status').textContent).toBe(text);
      expect(screen.getByTestId('local-copy-download').textContent).toBe('Download backup');
      cleanup();
    }
  });
});
