// A settlement's rows and press (spec 3 S5, 7): what `settle()` of a request against its draw's record does now
// (the shares minted into a new position, the deposit refunded, the withdrawal's payout, the shares still pending,
// a payout accruing below P_MIN, the record's reserve going back to its funder when this settlement drains it), the
// network fee and the total of the very transaction the Settle button signs (`settleFigures`, bound to it). Before
// the build the rows are `vaultView`'s; the fee and total wait for the build.
import type { SettleFigures, SettleView } from '@wasm/web.js';
import { Amount } from './Amount';
import { Estimating } from './Estimating';
import { FigureRow } from './FigureRow';
import { HashOrAddress } from './HashOrAddress';
import { Shares } from './Shares';
import { TxProgress } from './TxProgress';
import { Button } from './ui/button';
import { actionState, type ActionView } from './vaultStates';

export interface SettleRowsProps extends ActionView<SettleFigures> {
  testId: string;
  /** The settlement as `vaultView` computed it (before any build). */
  view: SettleView;
  round: string;
  walletConnected: boolean;
  disabled?: boolean;
  onSettle: () => void;
  explorer?: string;
}

export function SettleRows(p: SettleRowsProps) {
  const id = p.testId;
  const s = p.figures?.settle ?? p.view;
  const f = p.figures;
  const state = actionState(p);
  return (
    <div
      data-testid={id}
      data-state={state}
      data-mode={s.mode}
      className="mt-1 flex flex-col gap-0.5 border-t border-rule-soft pt-1"
    >
      {s.mode === 'convert' && (
        <FigureRow name="Shares minted" testId={`${id}-shares-row`}>
          <Shares value={s.shares} testId={`${id}-shares`} />
        </FigureRow>
      )}
      {(s.mode === 'refund' || s.mode === 'final' || s.mode === 'pay') && (
        <FigureRow name={s.mode === 'refund' ? 'Refund' : 'Payout'} testId={`${id}-payout-row`}>
          <Amount sompi={s.payout} testId={`${id}-payout`} />
        </FigureRow>
      )}
      {s.mode === 'accrue' && (
        <FigureRow name="Accrued" testId={`${id}-accrued-row`}>
          <Amount sompi={s.accrued} testId={`${id}-accrued`} />
        </FigureRow>
      )}
      {(s.mode === 'pay' || s.mode === 'accrue') && (
        <FigureRow name="Still pending" testId={`${id}-left-row`}>
          <Shares value={s.left} testId={`${id}-left`} />
        </FigureRow>
      )}
      {s.drained && (
        <>
          <FigureRow name="Record reserve" testId={`${id}-record-row`}>
            <Amount sompi={s.recordRefund} testId={`${id}-record`} />
          </FigureRow>
          {s.recordFunder && (
            <FigureRow name="To" testId={`${id}-record-to-row`} part>
              <HashOrAddress value={s.recordFunder} kind="address" explorer={p.explorer} testId={`${id}-record-to`} />
            </FigureRow>
          )}
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
      {p.error && !p.tx && (
        <p role="alert" data-testid={`${id}-error`} className="m-0 text-[13px] text-amber">
          {p.error}
        </p>
      )}
      {p.walletConnected && state !== 'accepted' && (
        <Button
          variant="primary"
          data-testid={`${id}-button`}
          aria-label={`Settle the round ${p.round} request`}
          disabled={!f || p.disabled || p.blocked || state === 'running'}
          onClick={p.onSettle}
        >
          Settle
        </Button>
      )}
      {p.tx && (
        <div data-testid={`${id}-progress`} className="flex flex-col gap-1 pt-1">
          <TxProgress step={p.tx.step} failed={p.tx.failed} checking={p.tx.checking} />
          {p.tx.id && <HashOrAddress value={p.tx.id} kind="transaction" explorer={p.explorer} testId={`${id}-txid`} />}
        </div>
      )}
    </div>
  );
}
