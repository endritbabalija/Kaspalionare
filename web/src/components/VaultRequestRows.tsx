// The rows a vault request shares (deposit and withdrawal; spec 3 S5, 7): the reserve R it posts and when it comes
// back, the network fee, the total leaving the wallet, the terms, and, while the vault is younger than
// VAULT_REQ_DELAY, from when it takes the request. Then the press and the progress.
import type { ReactNode } from 'react';
import { Amount } from './Amount';
import { Estimating } from './Estimating';
import { FigureRow } from './FigureRow';
import { HashOrAddress } from './HashOrAddress';
import { Time } from './Time';
import { TxProgress } from './TxProgress';
import { Button } from './ui/button';
import { VaultTerms } from './VaultTerms';
import { actionState, type ActionView } from './vaultStates';

export interface VaultRequestRowsProps extends ActionView<{
  reserve: string;
  fee: string;
  total: string;
  revocable: boolean;
  round: string;
}> {
  testId: string;
  /** A deposit's refund term (absent for a withdrawal). */
  refundable?: boolean;
  requestOpen: boolean;
  requestFrom: string;
  clock: { virtualDaa: string; observedAt: number };
  walletConnected: boolean;
  /** What the press does ("Request deposit"). */
  label: string;
  disabled?: boolean;
  onRequest: () => void;
  explorer?: string;
  children?: ReactNode;
}

export function VaultRequestRows(p: VaultRequestRowsProps) {
  const id = p.testId;
  const f = p.figures;
  const state = actionState(p);
  return (
    <>
      {p.children}
      {f && f.reserve !== '0' && (
        <>
          <FigureRow name="Reserve" testId={`${id}-reserve-row`}>
            <Amount sompi={f.reserve} testId={`${id}-reserve`} />
          </FigureRow>
          <FigureRow name="Back" testId={`${id}-back-row`} part>
            <span data-testid={`${id}-back`}>With settlement</span>
          </FigureRow>
        </>
      )}
      <FigureRow name="Network fee" testId={`${id}-fee-row`}>
        {f ? (
          <Amount sompi={f.fee} testId={`${id}-fee`} />
        ) : (
          <Estimating estimating={p.estimating} testId={`${id}-fee`} />
        )}
      </FigureRow>
      <FigureRow name="Total" testId={`${id}-total-row`}>
        {f ? (
          <Amount sompi={f.total} testId={`${id}-total`} />
        ) : (
          <Estimating estimating={p.estimating} testId={`${id}-total`} />
        )}
      </FigureRow>
      {f && <VaultTerms revocable={f.revocable} round={f.round} refundable={p.refundable} testId={`${id}-terms`} />}
      {!p.requestOpen && (
        <FigureRow name="From" testId={`${id}-from-row`}>
          <Time
            daa={p.requestFrom}
            virtualDaa={p.clock.virtualDaa}
            observedAt={p.clock.observedAt}
            testId={`${id}-from`}
          />
        </FigureRow>
      )}
      {p.error && !p.tx && (
        <p role="alert" data-testid={`${id}-error`} className="m-0 text-[13px] text-amber">
          {p.error}
        </p>
      )}
      {!p.walletConnected && (
        <a href="#/wallet" data-testid={`${id}-connect`} className="flex min-h-11 items-center font-medium">
          Connect
        </a>
      )}
      {p.walletConnected && state !== 'accepted' && (
        <Button
          variant="primary"
          data-testid={`${id}-button`}
          disabled={!f || !p.requestOpen || p.disabled || p.blocked || state === 'running'}
          onClick={p.onRequest}
        >
          {p.label}
        </Button>
      )}
      {p.tx && (
        <div data-testid={`${id}-progress`} className="flex flex-col gap-1 pt-1">
          <TxProgress step={p.tx.step} failed={p.tx.failed} checking={p.tx.checking} />
          {p.tx.id && <HashOrAddress value={p.tx.id} kind="transaction" explorer={p.explorer} testId={`${id}-txid`} />}
        </div>
      )}
    </>
  );
}
