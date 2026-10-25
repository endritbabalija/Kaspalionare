// Transaction progress (spec 4): built -> signed -> sent -> accepted; failed with its reason; conflict ->
// rebuilt (on another shard, up to 3 tries, spec S1); checking (the node's answer was lost and the page asks
// the chain, F2/F3 review S3). The step names are the status; the reason is the
// error's one message.
import { cn } from '@/lib/cn';

const TX_STEPS = ['built', 'signed', 'sent', 'accepted'] as const;
export type TxStep = (typeof TX_STEPS)[number];

export interface TxProgressProps {
  /** The last step reached. */
  step: TxStep;
  /** Set when the step after `step` failed: the error's message. */
  failed?: string;
  /** Rebuilds after a shard conflict so far, when any (spec S1: up to 3). */
  rebuilt?: { tries: number; max: number };
  /** The node's answer was lost: the page is asking the chain whether the transaction went through. */
  checking?: boolean;
}

const name: Record<TxStep, string> = { built: 'Built', signed: 'Signed', sent: 'Sent', accepted: 'Accepted' };

export function TxProgress({ step, failed, rebuilt, checking }: TxProgressProps) {
  const at = TX_STEPS.indexOf(step);
  return (
    <div
      data-testid="tx-progress"
      data-step={step}
      data-failed={failed ? 'true' : 'false'}
      data-checking={checking ? 'true' : 'false'}
      className="flex flex-col gap-2"
    >
      <ol className="m-0 flex list-none flex-wrap items-center gap-2 p-0 text-[13px]" aria-label="Transaction progress">
        {TX_STEPS.map((s, i) => {
          const done = i <= at;
          const current = i === at && !failed && step !== 'accepted';
          const failedHere = failed !== undefined && i === at + 1;
          return (
            <li
              key={s}
              className="flex items-center gap-2"
              data-testid={`tx-step-${s}`}
              aria-current={i === at ? 'step' : undefined}
            >
              {i > 0 && <span aria-hidden="true" className={cn('h-px w-5', done ? 'bg-teal' : 'bg-control-line')} />}
              <span
                className={cn(
                  'flex items-center gap-1.5',
                  done ? 'text-ink' : 'text-graphite',
                  current && 'font-semibold',
                  failedHere && 'text-amber',
                )}
              >
                <span
                  aria-hidden="true"
                  className={cn(
                    'box-border h-2.5 w-2.5 rounded-full',
                    done ? 'bg-teal' : failedHere ? 'border-2 border-amber' : 'border border-control-line',
                  )}
                />
                {name[s]}
                {done && <span className="sr-only">, done</span>}
                {failedHere && <span className="sr-only">, failed</span>}
              </span>
            </li>
          );
        })}
      </ol>
      <div role="status" aria-live="polite" className="flex flex-col gap-1 text-[13px]">
        {rebuilt && (
          <span data-testid="tx-rebuilt" className="font-mono text-graphite">
            Rebuilt {rebuilt.tries}/{rebuilt.max}
          </span>
        )}
        {checking && (
          <span data-testid="tx-checking" className="font-mono text-graphite">
            Checking
          </span>
        )}
        {failed && (
          <span data-testid="tx-failed" className="text-amber">
            {failed}
          </span>
        )}
      </div>
    </div>
  );
}
