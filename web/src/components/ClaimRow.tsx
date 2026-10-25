// Claim row (spec 3 S2, 4, 7 "Claims"): one winning combination's claim. States: claimable, claiming,
// claimed, accrued below P_MIN, fee exceeds prize. Every figure is the wasm's: the prize floor(slice / h), the
// others it is shared with (h - 1, when h > 1), when it is paid (now; with the last claim when it accrues
// below P_MIN, D56), the deposit paid with a last claim, the fee of the built claim (shown before signing),
// the claim deadline as clock and DAA. The Claim button signs exactly the built claim these rows show.
import type { ClaimFigures, ClaimView } from '@wasm/web.js';
import { messageOf } from '@/lib/errors';
import { Amount } from './Amount';
import { FigureRow } from './FigureRow';
import { Time } from './Time';
import { TxProgress, type TxStep } from './TxProgress';
import { HashOrAddress } from './HashOrAddress';
import { claimRowState } from './ticketStates';
import { Button } from './ui/button';

export interface ClaimTx {
  step: TxStep;
  failed?: string;
  checking?: boolean;
  id?: string;
}

export interface ClaimRowProps {
  testId: string;
  /** The claim the chain would take now (ticketCard); absent once claimed. */
  claim?: ClaimView;
  claimed?: boolean;
  /** The built claim's figures (claimFigures): the fee, what is paid now. */
  figures?: ClaimFigures;
  estimating?: boolean;
  /** The claim deadline (DAA) and the clock to show it with. */
  deadline?: string;
  clock?: { virtualDaa: string; observedAt: number };
  tx?: ClaimTx;
  /** A build or send failure: its one message. */
  error?: string;
  disabled?: boolean;
  onClaim?: () => void;
  explorer?: string;
}

export function ClaimRow(p: ClaimRowProps) {
  const state = claimRowState(p);
  const c = p.claim;
  const f = p.figures;
  const shared = c && c.h !== '1' ? c.sharedWith : undefined;
  return (
    <div
      data-testid={p.testId}
      data-state={state}
      className="flex flex-col gap-0.5 rounded-[10px] border border-rule-soft p-3"
    >
      {state === 'claimed' && !c && (
        <FigureRow name="Claim" testId={`${p.testId}-status`}>
          <span data-testid={`${p.testId}-claimed`}>Claimed</span>
        </FigureRow>
      )}
      {c && (
        <>
          <FigureRow name="Prize" testId={`${p.testId}-prize-row`}>
            <Amount sompi={c.prize} testId={`${p.testId}-prize`} />
          </FigureRow>
          {shared && (
            <FigureRow name="Shared with" testId={`${p.testId}-shared-row`} part>
              <span data-testid={`${p.testId}-shared`} data-value={shared}>
                {shared}
              </span>
            </FigureRow>
          )}
          <FigureRow name="Paid" testId={`${p.testId}-paid-row`} part>
            <span data-testid={`${p.testId}-paid`} data-value={c.mode}>
              {c.mode === 'accrue' ? 'With last claim' : 'Now'}
            </span>
          </FigureRow>
          {c.mode === 'final' && f && f.valueBack !== '0' && (
            <FigureRow name="Deposit back" testId={`${p.testId}-deposit-row`}>
              <Amount sompi={f.valueBack} testId={`${p.testId}-deposit`} />
            </FigureRow>
          )}
          <FigureRow name="Network fee" testId={`${p.testId}-fee-row`}>
            {f ? (
              <Amount sompi={f.fee} testId={`${p.testId}-fee`} />
            ) : (
              <span
                data-testid={`${p.testId}-fee`}
                aria-busy={p.estimating ? 'true' : undefined}
                className="text-graphite"
              >
                <span aria-hidden="true">–</span>
                <span className="sr-only">{p.estimating ? 'Estimating' : 'none'}</span>
              </span>
            )}
          </FigureRow>
          {p.deadline && p.clock && (
            <FigureRow name="Claim by" testId={`${p.testId}-deadline-row`}>
              <Time
                daa={p.deadline}
                virtualDaa={p.clock.virtualDaa}
                observedAt={p.clock.observedAt}
                testId={`${p.testId}-deadline`}
              />
            </FigureRow>
          )}
          {state === 'fee-exceeds' && (
            <p role="alert" data-testid={`${p.testId}-warning`} className="m-0 text-[13px] text-amber">
              {messageOf('fee_above_prize')}
            </p>
          )}
          {p.error && !p.tx && (
            <p role="alert" data-testid={`${p.testId}-error`} className="m-0 text-[13px] text-amber">
              {p.error}
            </p>
          )}
          {state !== 'claimed' && (
            <Button
              variant="primary"
              data-testid={`${p.testId}-button`}
              aria-label={`Claim ${c.tierName}`}
              disabled={!f || p.disabled || state === 'claiming'}
              onClick={p.onClaim}
            >
              Claim
            </Button>
          )}
        </>
      )}
      {p.tx && (
        <div data-testid={`${p.testId}-progress`} className="flex flex-col gap-1 pt-1">
          <TxProgress step={p.tx.step} failed={p.tx.failed} checking={p.tx.checking} />
          {p.tx.id && (
            <HashOrAddress value={p.tx.id} kind="transaction" explorer={p.explorer} testId={`${p.testId}-txid`} />
          )}
        </div>
      )}
    </div>
  );
}
