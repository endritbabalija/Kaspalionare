// F6 components (spec 4 "Vault position card": position, pending request, settle due; 3 S5): their states, names
// and test ids, from the catalogue's sample figures shaped as the wasm returns them; every control named and tagged.
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import {
  SAMPLE_DEPOSIT,
  SAMPLE_PAY,
  SAMPLE_POSITION,
  SAMPLE_REQUESTS,
  SAMPLE_SETTLE,
  SAMPLE_VAULT,
  SAMPLE_WITHDRAW,
} from '@/dev/vaultSamples';
import { DepositPanel } from './DepositPanel';
import { SettleRows } from './SettleRows';
import { VaultFigures } from './VaultFigures';
import { VaultPositionCard } from './VaultPositionCard';
import { WithdrawPanel } from './WithdrawPanel';

afterEach(cleanup);
const clock = { virtualDaa: '13000', observedAt: Date.UTC(2026, 9, 25) };
const none = () => undefined;

function namedAndTagged() {
  for (const el of Array.from(document.querySelectorAll<HTMLElement>('button, a[href], input'))) {
    const labels = (el as HTMLInputElement).labels;
    const name =
      el.getAttribute('aria-label') ??
      (labels && labels.length
        ? Array.from(labels)
            .map((l) => l.textContent ?? '')
            .join(' ')
        : el.textContent);
    expect(el.dataset.testid, el.outerHTML).toBeTruthy();
    expect(name?.trim(), el.outerHTML).toBeTruthy();
  }
}

describe('VaultFigures', () => {
  it('shows each figure of the vault, the next request as now or a time, and stale data as such', () => {
    render(
      <>
        <VaultFigures vault={{ ...SAMPLE_VAULT, requestOpen: true }} clock={clock} testId="a" />
        <VaultFigures vault={SAMPLE_VAULT} clock={clock} stale testId="b" />
      </>,
    );
    expect(screen.getByTestId('a-value').dataset.sompi).toBe(SAMPLE_VAULT.totalAssets);
    expect(screen.getByTestId('a-price').textContent).toBe('0.99999999 sompi');
    expect(screen.getByTestId('a-liability').dataset.sompi).toBe(SAMPLE_VAULT.liability);
    expect(screen.getByTestId('a-withdrawals').textContent).toBe('1,200,000,000 shares');
    expect(screen.getByTestId('a-draw').textContent).toBe('Round 12');
    expect(screen.getByTestId('a-request').textContent).toBe('Now');
    expect(screen.getByTestId('b').dataset.state).toBe('stale');
    expect(screen.getByTestId('b-request').dataset.open).toBe('false');
    expect(screen.getByTestId('b-request-time').textContent).toContain('DAA 13,300');
    namedAndTagged();
  });
});

describe('VaultPositionCard', () => {
  it('position · pending request · settle due · unknown', () => {
    render(
      <>
        <VaultPositionCard position={SAMPLE_POSITION} testId="p" />
        <VaultPositionCard position={{ ...SAMPLE_POSITION, ownerKey: undefined }} testId="e" />
        <VaultPositionCard request={SAMPLE_REQUESTS.pending} testId="q" />
        <VaultPositionCard request={SAMPLE_REQUESTS.due} testId="d" />
        <VaultPositionCard request={SAMPLE_REQUESTS.unknown} testId="u" />
      </>,
    );
    expect(screen.getByTestId('p').dataset.state).toBe('position');
    expect(screen.getByTestId('p-shares').textContent).toBe('2,500,000,000 shares');
    expect(screen.queryByTestId('p-owner-type')).toBeNull();
    expect(screen.getByTestId('e-owner-type').textContent).toMatch(/owner_type|Schnorr/);
    expect(screen.getByTestId('q').dataset.state).toBe('pending');
    expect(screen.getByTestId('q-status').textContent).toBe('Pending');
    expect(screen.getByTestId('q-price').textContent).toBe('Round 12 draw');
    expect(screen.getByTestId('d').dataset.state).toBe('settle-due');
    expect(screen.getByTestId('d-status').textContent).toBe('Settle due');
    expect(screen.getByTestId('u').dataset.state).toBe('unknown');
    expect(screen.getByTestId('u-amount').textContent).toBe('1,200,000,000 shares');
    namedAndTagged();
  });
});

describe('SettleRows', () => {
  it('shows what the settlement does; the fee and total wait for the build; Settle only once built', () => {
    render(
      <>
        <SettleRows
          testId="s"
          view={SAMPLE_SETTLE.settle}
          round="11"
          figures={SAMPLE_SETTLE}
          walletConnected
          onSettle={none}
        />
        <SettleRows testId="t" view={SAMPLE_PAY} round="11" estimating walletConnected onSettle={none} />
      </>,
    );
    expect(screen.getByTestId('s').dataset.mode).toBe('convert');
    expect(screen.getByTestId('s-shares').textContent).toBe('2,500,000,000 shares');
    expect(screen.getByTestId('s-record').dataset.sompi).toBe('169000000');
    expect(screen.getByTestId('s-total').dataset.sompi).toBe('-165337600');
    expect((screen.getByTestId('s-button') as HTMLButtonElement).disabled).toBe(false);
    expect(screen.getByTestId('t').dataset.state).toBe('estimating');
    expect(screen.getByTestId('t-payout').dataset.sompi).toBe('499999999');
    expect(screen.getByTestId('t-left').textContent).toBe('500,000 shares');
    expect(screen.getByTestId('t-fee').getAttribute('aria-busy')).toBe('true');
    expect((screen.getByTestId('t-button') as HTMLButtonElement).disabled).toBe(true);
    namedAndTagged();
  });
});

describe('DepositPanel and WithdrawPanel', () => {
  it('rows and terms from the built transaction; the press waits for the vault to take requests', () => {
    render(
      <>
        <DepositPanel
          testId="d"
          minDeposit="1000000000"
          amount="25"
          onAmount={none}
          figures={SAMPLE_DEPOSIT}
          requestOpen={false}
          requestFrom="13300"
          clock={clock}
          walletConnected
          onRequest={none}
        />
        <WithdrawPanel
          testId="w"
          position={SAMPLE_POSITION}
          mode="part"
          onMode={none}
          amount="12"
          onAmount={none}
          figures={SAMPLE_WITHDRAW}
          requestOpen
          requestFrom="13300"
          clock={clock}
          walletConnected
          onRequest={none}
        />
        <DepositPanel
          testId="x"
          minDeposit="1000000000"
          amount=""
          onAmount={none}
          requestOpen
          requestFrom="13300"
          clock={clock}
          walletConnected={false}
          onRequest={none}
        />
      </>,
    );
    expect(screen.getByTestId('d-total').dataset.sompi).toBe(SAMPLE_DEPOSIT.total);
    expect(screen.getByTestId('d-reserve').dataset.sompi).toBe(SAMPLE_DEPOSIT.reserve);
    expect(screen.getByTestId('d-back').textContent).toBe('With settlement');
    expect(screen.getByTestId('d-terms-revocable').textContent).toBe('No');
    expect(screen.getByTestId('d-terms-price').textContent).toBe('Round 12 draw');
    expect(screen.getByTestId('d-terms-refund').textContent).toBe('Possible');
    expect(screen.getByTestId('d-from')).toBeTruthy();
    expect((screen.getByTestId('d-button') as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByTestId('w').dataset.mode).toBe('part');
    expect(screen.getByTestId('w-least').dataset.sompi).toBe(SAMPLE_POSITION.partial?.leastWorth);
    expect(screen.getByTestId('w-left').textContent).toBe('1,300,000,000 shares');
    expect(screen.queryByTestId('w-terms-refund')).toBeNull();
    expect((screen.getByTestId('w-button') as HTMLButtonElement).disabled).toBe(false);
    expect(screen.getByTestId('x-connect').getAttribute('href')).toBe('#/wallet');
    expect(screen.queryByTestId('x-button')).toBeNull();
    namedAndTagged();
  });
});
