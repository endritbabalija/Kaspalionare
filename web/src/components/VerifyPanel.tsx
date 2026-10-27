// Verify in this browser (spec J6, S3, 4 "verify result: match, mismatch, not checkable"): the Verify button and,
// once run, one line per check with its result as text (never colour alone). Every check ran in the worker
// (`verifyRound`): the item names are the figures checked; a mismatch also shows the one message of
// chain_mismatch. An item's `detail` is for logs, never shown (copy rule); its code is in `data-code`. States: idle,
// running, done, error.
import type { RoundVerify, VerifyItem } from '@wasm/web.js';
import { cn } from '@/lib/cn';
import { messageOf } from '@/lib/errors';
import { shortHash } from '@/lib/format';
import { Button } from './ui/button';

export type VerifyState = 'idle' | 'running' | 'done' | 'error';

export interface VerifyPanelProps {
  state: VerifyState;
  result?: RoundVerify;
  error?: string;
  disabled?: boolean;
  onVerify: () => void;
  testId?: string;
}

const RESULT = { match: 'Match', mismatch: 'Mismatch', notCheckable: 'Not checkable' } as const;

const ROLE: Record<string, string> = {
  controller: 'Controller',
  params: 'Params',
  shard: 'Shard',
  marker: 'Marker',
  drawer: 'Drawer',
  vault: 'Vault',
  pool: 'Pool',
  record: 'Record',
  ticket: 'Ticket',
  position: 'Position',
  request: 'Request',
};

/** The item's name: the figure it checks. */
function itemName(i: VerifyItem): string {
  switch (i.kind) {
    case 'state':
      return `${ROLE[i.role ?? ''] ?? 'Object'} ${i.outpoint ? `${shortHash(i.outpoint.transactionId)}:${i.outpoint.index}` : ''}`.trim();
    case 'status':
      return 'Status';
    case 'roots':
      return 'Frozen roots';
    case 'tree':
      return `Shard ${i.shard ?? ''} tree`;
    case 'numbers':
      return 'Winning numbers';
    case 'anchor':
      return 'Anchor block';
  }
}

function Mark({ result }: { result: VerifyItem['result'] }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        'box-border inline-block h-2.5 w-2.5 shrink-0 rounded-full',
        result === 'match' ? 'bg-teal' : result === 'mismatch' ? 'border-2 border-amber' : 'border border-control-line',
      )}
    />
  );
}

export function VerifyPanel({ state, result, error, disabled, onVerify, testId = 'verify' }: VerifyPanelProps) {
  const counts = result
    ? (['match', 'mismatch', 'notCheckable'] as const).map(
        (k) => [k, result.items.filter((i) => i.result === k).length] as const,
      )
    : [];
  return (
    <section
      data-testid={testId}
      data-state={state}
      data-consistent={result ? (result.consistent ? 'true' : 'false') : ''}
      aria-labelledby={`${testId}-title`}
      className="flex flex-col gap-2 rounded-[14px] border border-rule bg-surface p-4"
    >
      <h2 id={`${testId}-title`} className="m-0 text-[15px] font-semibold">
        Verify in this browser
      </h2>
      {result && (
        <>
          <div className="flex flex-wrap gap-3 font-mono text-[13px]" data-testid={`${testId}-counts`}>
            {counts.map(([k, n]) => (
              <span key={k} data-testid={`${testId}-count-${k}`} data-value={n} className="flex items-center gap-1.5">
                <Mark result={k} />
                {RESULT[k]} {n}
              </span>
            ))}
          </div>
          {!result.consistent && (
            <p role="alert" data-testid={`${testId}-mismatch`} className="m-0 text-[13px] text-amber">
              {messageOf('chain_mismatch')}
            </p>
          )}
          <ul className="m-0 flex list-none flex-col p-0" data-testid={`${testId}-items`}>
            {result.items.map((i, n) => (
              <li
                key={`${i.kind}-${n}`}
                data-testid={`${testId}-item-${n}`}
                data-kind={i.kind}
                data-result={i.result}
                data-shard={i.shard ?? ''}
                data-code={i.code ?? ''}
                className="flex min-h-9 items-center justify-between gap-3 border-b border-rule-soft text-[13px] last:border-b-0"
              >
                <span className="min-w-0 truncate">{itemName(i)}</span>
                <span
                  className={cn(
                    'flex shrink-0 items-center gap-1.5 font-mono',
                    i.result === 'mismatch' && 'text-amber',
                  )}
                >
                  <Mark result={i.result} />
                  {RESULT[i.result]}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
      {state === 'error' && error && (
        <p role="alert" data-testid={`${testId}-error`} className="m-0 text-[13px] text-amber">
          {error}
        </p>
      )}
      <Button
        variant="secondary"
        data-testid={`${testId}-button`}
        aria-label="Verify in this browser"
        disabled={disabled || state === 'running'}
        aria-busy={state === 'running' ? 'true' : undefined}
        onClick={onVerify}
      >
        {state === 'running' ? 'Verifying' : 'Verify'}
      </Button>
    </section>
  );
}
