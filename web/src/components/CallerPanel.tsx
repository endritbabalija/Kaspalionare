// Caller panel (spec 3 S4, 4 "Caller panel": not due, due with reward, reward below fee; 7; design 12 liveness
// note): what is due on this round and from when (clock and DAA), and, once built with the connected wallet,
// the rows of the very transaction a press signs (`callerFigures`, bound to it): the reward C_close or C_draw,
// the reserves the caller posts and when they come back (the marker's with draw(); the new pools' from their
// claim deadline, when a later draw absorbs them; a settlement record's with the settlements), the expired pools
// it absorbs, any reserve refunded now, the network fee and the total leaving the wallet. A fee above the reward
// is a warning row, not a refusal. Then the progress.
import type { CallerFigures } from '@wasm/web.js';
import { messageOf } from '@/lib/errors';
import type { CallerAction, Due } from '@/rounds/caller';
import { Amount } from './Amount';
import { FigureRow } from './FigureRow';
import { HashOrAddress } from './HashOrAddress';
import { Time } from './Time';
import { TxProgress, type TxStep } from './TxProgress';
import { Button } from './ui/button';
import { callerPanelState } from './callerPanelState';

export interface CallerTx {
  step: TxStep;
  failed?: string;
  checking?: boolean;
  id?: string;
}

export interface CallerPanelProps {
  round: string;
  due: Due;
  /** The reward the chain objects give before anything is built (the round's facts). */
  reward?: string;
  figures?: CallerFigures;
  estimating?: boolean;
  error?: string;
  tx?: CallerTx;
  walletConnected: boolean;
  disabled?: boolean;
  onPress: () => void;
  clock: { virtualDaa: string; observedAt: number };
  explorer?: string;
  testId?: string;
}

const NAME: Record<CallerAction, string> = { close: 'close()', draw: 'draw()', void: 'Void' };
const PRESS: Record<CallerAction, string> = { close: 'Close', draw: 'Draw', void: 'Void' };

export function CallerPanel(p: CallerPanelProps) {
  const id = p.testId ?? 'caller';
  const state = callerPanelState(p);
  const due = p.due;
  if (due.kind === 'none' && !p.tx) return null;
  const f = p.figures;
  const time = (daa: string, testId: string) => (
    <Time daa={daa} virtualDaa={p.clock.virtualDaa} observedAt={p.clock.observedAt} testId={testId} />
  );
  return (
    <section
      data-testid={id}
      data-state={state}
      data-due={due.kind}
      data-action={'action' in due ? due.action : ''}
      aria-labelledby={`${id}-title`}
      className="flex flex-col gap-0.5 rounded-[14px] border border-rule bg-surface p-4"
    >
      <h2 id={`${id}-title`} className="m-0 pb-1 text-[15px] font-semibold">
        Run the round
      </h2>
      {due.kind === 'waits' && (
        <FigureRow name="close() after" testId={`${id}-waits`}>
          <span data-testid={`${id}-status`}>draw()</span>
        </FigureRow>
      )}
      {due.kind === 'later' && (
        <FigureRow name={`${NAME[due.action]} from`} testId={`${id}-from-row`}>
          {time(due.from, `${id}-from`)}
        </FigureRow>
      )}
      {due.kind === 'anchor' && (
        <>
          <FigureRow name="draw() from" testId={`${id}-from-row`}>
            {time(due.from, `${id}-from`)}
          </FigureRow>
          <FigureRow name="Anchor proof" testId={`${id}-anchor-row`}>
            <span data-testid={`${id}-status`}>Unavailable</span>
          </FigureRow>
        </>
      )}
      {due.kind === 'due' && (
        <p data-testid={`${id}-status`} role="status" className="m-0 flex min-h-11 items-center font-medium">
          {NAME[due.action]} is due
        </p>
      )}
      {(due.kind === 'due' || due.kind === 'later') && (
        <FigureRow name="Reward" testId={`${id}-reward-row`}>
          {f ? (
            <Amount sompi={f.reward} testId={`${id}-reward`} />
          ) : p.reward ? (
            <Amount sompi={p.reward} testId={`${id}-reward`} />
          ) : (
            <span className="text-graphite">–</span>
          )}
        </FigureRow>
      )}
      {due.kind === 'due' && (
        <>
          <FigureRow name="Reserve" testId={`${id}-reserve-row`}>
            {f ? (
              <Amount sompi={f.reserve} testId={`${id}-reserve`} />
            ) : (
              <Pending estimating={p.estimating} testId={`${id}-reserve`} />
            )}
          </FigureRow>
          {f?.reserveBackWithDraw && (
            <FigureRow name="Back at" testId={`${id}-back-row`} part>
              <span data-testid={`${id}-back`}>draw()</span>
            </FigureRow>
          )}
          {f?.reserveBackFrom && (
            <FigureRow name="Back" testId={`${id}-back-row`} part>
              {time(f.reserveBackFrom, `${id}-back`)}
            </FigureRow>
          )}
          {f?.record && (
            <FigureRow name="Record back at" testId={`${id}-record-back-row`} part>
              <span data-testid={`${id}-record-back`}>Settlement</span>
            </FigureRow>
          )}
          {f && f.absorbed > 0 && (
            <FigureRow name="Expired pools" testId={`${id}-absorbed-row`}>
              <span data-testid={`${id}-absorbed`} data-value={f.absorbed} className="font-mono">
                {f.absorbed}
              </span>
            </FigureRow>
          )}
          {f && f.refund !== '0' && (
            <FigureRow name="Refund" testId={`${id}-refund-row`}>
              <Amount sompi={f.refund} testId={`${id}-refund`} />
            </FigureRow>
          )}
          <FigureRow name="Network fee" testId={`${id}-fee-row`}>
            {f ? (
              <Amount sompi={f.fee} testId={`${id}-fee`} />
            ) : (
              <Pending estimating={p.estimating} testId={`${id}-fee`} />
            )}
          </FigureRow>
          <FigureRow name="Total" testId={`${id}-total-row`}>
            {f ? (
              <Amount sompi={f.total} testId={`${id}-total`} />
            ) : (
              <Pending estimating={p.estimating} testId={`${id}-total`} />
            )}
          </FigureRow>
          {f?.feeAboveReward && (
            <p role="status" data-testid={`${id}-warning`} className="m-0 text-[13px] text-amber">
              {messageOf('fee_above_reward')}
            </p>
          )}
        </>
      )}
      {p.error && (
        <p role="alert" data-testid={`${id}-error`} className="m-0 text-[13px] text-amber">
          {p.error}
        </p>
      )}
      {due.kind === 'due' && !p.walletConnected && (
        <a href="#/wallet" data-testid={`${id}-connect`} className="flex min-h-11 items-center font-medium">
          Connect
        </a>
      )}
      {due.kind === 'due' && p.walletConnected && state !== 'done' && (
        <Button
          variant="primary"
          data-testid={`${id}-button`}
          aria-label={`${PRESS[due.action]} round ${p.round}`}
          disabled={!f || p.disabled || state === 'running'}
          onClick={p.onPress}
        >
          {PRESS[due.action]}
        </Button>
      )}
      {p.tx && (
        <div data-testid={`${id}-progress`} className="flex flex-col gap-1 pt-1">
          <TxProgress step={p.tx.step} failed={p.tx.failed} checking={p.tx.checking} />
          {p.tx.id && <HashOrAddress value={p.tx.id} kind="transaction" explorer={p.explorer} testId={`${id}-txid`} />}
        </div>
      )}
    </section>
  );
}

function Pending({ estimating, testId }: { estimating?: boolean; testId: string }) {
  return (
    <span data-testid={testId} aria-busy={estimating ? 'true' : undefined} className="text-graphite">
      <span aria-hidden="true">–</span>
      <span className="sr-only">{estimating ? 'Estimating' : 'none'}</span>
    </span>
  );
}
