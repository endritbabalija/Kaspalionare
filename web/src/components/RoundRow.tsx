// Round row (spec 3 S3, 4): one round of the list: its number (the link to its detail), its status (the chain's:
// a pill, or "Unverified" when no live object shows one), sales S and jackpot J. A figure no live object fixes is
// the indexer's and says "Unverified". States: checking, ready, error.
import type { RowData, Shown } from '@/rounds/load';
import { Amount } from './Amount';
import { FigureRow } from './FigureRow';
import { RoundStatusPill } from './RoundStatusPill';
import { Unverified } from './Unverified';

export interface RoundRowProps {
  round: string;
  /** Absent while the row is being checked against the node. */
  data?: RowData;
  testId: string;
}

function Figure({ v, testId }: { v: Shown | undefined; testId: string }) {
  if (!v) return <span className="text-graphite">–</span>;
  return (
    <span
      className="flex items-center gap-2"
      data-testid={`${testId}-value`}
      data-checked={v.checked ? 'true' : 'false'}
    >
      <Amount sompi={v.value} testId={testId} />
      {!v.checked && <Unverified testId={`${testId}-unverified`} />}
    </span>
  );
}

export function RoundRow({ round, data, testId }: RoundRowProps) {
  const state = !data ? 'checking' : data.error ? 'error' : 'ready';
  return (
    <article
      data-testid={testId}
      data-state={state}
      data-round={round}
      data-status={data?.status ?? ''}
      aria-busy={state === 'checking' ? 'true' : undefined}
      className="flex flex-col gap-0.5 rounded-[14px] border border-rule bg-surface p-4"
    >
      <div className="flex min-h-11 items-center justify-between gap-3">
        <a
          href={`#/rounds?r=${encodeURIComponent(round)}`}
          data-testid={`${testId}-link`}
          aria-label={`Round ${round} details`}
          className="flex min-h-11 min-w-11 items-center text-[15px] font-semibold"
        >
          Round {round}
        </a>
        {state === 'checking' && (
          <span className="h-6 w-16 animate-pulse rounded-full bg-rule-soft" aria-hidden="true" />
        )}
        {state === 'checking' && <span className="sr-only">Checking</span>}
        {data &&
          !data.error &&
          (data.status ? <RoundStatusPill status={data.status} /> : <Unverified testId={`${testId}-status`} />)}
      </div>
      {data?.error && (
        <p
          role="alert"
          data-testid={`${testId}-error`}
          data-code={data.code ?? ''}
          className="m-0 text-[13px] text-amber"
        >
          {data.error}
        </p>
      )}
      {data && !data.error && (
        <>
          <FigureRow name="Sales" testId={`${testId}-sales-row`}>
            <Figure v={data.sales} testId={`${testId}-sales`} />
          </FigureRow>
          <FigureRow name="Jackpot" testId={`${testId}-jackpot-row`}>
            <Figure v={data.jackpot} testId={`${testId}-jackpot`} />
          </FigureRow>
        </>
      )}
    </article>
  );
}
