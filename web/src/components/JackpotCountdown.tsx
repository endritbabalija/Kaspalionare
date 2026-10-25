// Jackpot and countdown block (spec 4): live, deadline passed, stale data. J and T are the round's
// controller state (decoded by the wasm, checked against the node); the countdown runs on the local clock
// from the node's virtual DAA score at `observedAt`, 10 DAA per second.
import { useEffect, useState } from 'react';
import { clock, countdown, daaToMs } from '@/lib/format';
import { Amount } from './Amount';
import { jackpotState } from './jackpotState';
import { Time } from './Time';

export type { JackpotState } from './jackpotState';
export interface JackpotCountdownProps {
  /** J, sompi. */
  jackpot: string;
  /** T, DAA score. */
  deadline: string;
  virtualDaa: string;
  observedAt: number;
  /** The data is older than the page accepts (the node or the indexer stopped answering). */
  stale?: boolean;
  /** Fixed time for the catalogue and tests; live otherwise. */
  now?: number;
  /** Where "close() is due" leads (S4). */
  runHref?: string;
}

function useNow(fixed: number | undefined): number {
  const [now, setNow] = useState(() => fixed ?? Date.now());
  useEffect(() => {
    if (fixed !== undefined) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [fixed]);
  return fixed ?? now;
}

export function JackpotCountdown(p: JackpotCountdownProps) {
  const now = useNow(p.now);
  const deadlineMs = daaToMs(p.deadline, p.virtualDaa, p.observedAt);
  const state = jackpotState(deadlineMs, now, p.stale ?? false);
  return (
    <div data-testid="jackpot-block" data-state={state} className="flex flex-col gap-3">
      <div className="flex flex-col gap-0.5">
        <span className="text-[13px] text-graphite">Jackpot</span>
        <Amount sompi={p.jackpot} testId="jackpot" className="text-[34px] leading-tight font-medium tracking-tight" />
      </div>
      <div className="flex flex-col gap-1 border-t border-rule-soft pt-2.5">
        {state === 'passed' ? (
          <>
            <div className="flex min-h-11 items-center justify-between gap-3">
              <span className="text-[13px] text-graphite">Sales closed</span>
              <span data-testid="sales-closed" className="font-mono text-[15px] text-amber" role="status">
                Waiting for close()
              </span>
            </div>
            {p.runHref && (
              <a
                href={p.runHref}
                data-testid="run-round-link"
                className="flex min-h-11 items-center text-[13px] font-medium"
              >
                Run the round
              </a>
            )}
          </>
        ) : (
          <div className="flex min-h-11 items-center justify-between gap-3">
            <span className="text-[13px] text-graphite">Sales close in</span>
            <span data-testid="countdown" className="font-mono text-[15px] text-amber tabular-nums">
              {countdown(deadlineMs - now)}
            </span>
          </div>
        )}
        <div className="flex min-h-11 items-center justify-between gap-3">
          <span className="text-[13px] text-graphite">Deadline</span>
          <Time
            daa={p.deadline}
            virtualDaa={p.virtualDaa}
            observedAt={p.observedAt}
            now={now}
            testId="deadline"
            className="text-[13px]"
          />
        </div>
        {state === 'stale' && (
          <div className="flex min-h-11 items-center justify-between gap-3" data-testid="stale">
            <span className="text-[13px] text-amber" role="status">
              Stale
            </span>
            <span className="font-mono text-[13px] text-graphite">
              <span className="sr-only">Updated </span>
              {clock(p.observedAt, now)}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
