// S1's round card (spec 3): round number, status, jackpot J, time until the deadline T, the jackpot odds
// 1 in N × m (computed by the wasm) and a link to the round's page. Loading, error, stale and live.
import { useId } from 'react';
import { JackpotCountdown } from '@/components/JackpotCountdown';
import { RoundStatusPill } from '@/components/RoundStatusPill';
import { Button } from '@/components/ui/button';
import { groupDigits } from '@/lib/format';
import type { RoundCardData } from '@/net/round';
import type { RoundCardLoad } from './useRoundCard';

const card = 'flex flex-col gap-3 rounded-[14px] border border-rule bg-surface p-4';

export function RoundCardView({ load, onRetry, now }: { load: RoundCardLoad; onRetry: () => void; now?: number }) {
  if (load.kind === 'loading') {
    return (
      <section data-testid="round-card" data-state="loading" aria-busy="true" aria-label="Round" className={card}>
        <span className="h-5 w-28 animate-pulse rounded bg-rule-soft" />
        <span className="h-10 w-48 animate-pulse rounded bg-rule-soft" />
        <span className="h-5 w-full animate-pulse rounded bg-rule-soft" />
        <span className="sr-only">Loading</span>
      </section>
    );
  }
  if (load.kind === 'error') {
    return (
      <section data-testid="round-card" data-state="error" aria-label="Round" className={card}>
        <p role="alert" data-testid="round-card-error" data-code={load.code ?? ''} className="m-0 text-amber">
          {load.message}
        </p>
        <Button variant="secondary" onClick={onRetry} data-testid="round-card-retry">
          Retry
        </Button>
      </section>
    );
  }
  return <ReadyCard data={load.data} stale={load.stale} now={now} />;
}

function ReadyCard({ data, stale, now }: { data: RoundCardData; stale: boolean; now?: number }) {
  const title = useId();
  return (
    <section
      data-testid="round-card"
      data-state={stale ? 'stale' : 'ready'}
      data-round={data.round}
      aria-labelledby={title}
      className={card}
    >
      <div className="flex items-center gap-2">
        <h2 id={title} data-testid="round-number" className="m-0 text-[15px] font-semibold">
          Round {data.round}
        </h2>
        <RoundStatusPill status={data.status} />
      </div>
      <JackpotCountdown
        jackpot={data.jackpot}
        deadline={data.deadline}
        virtualDaa={data.virtualDaaScore}
        observedAt={data.observedAt}
        stale={stale}
        now={now}
        runHref="#/rounds"
      />
      <div className="flex min-h-11 items-center justify-between gap-3">
        <span className="text-[13px] text-graphite">Jackpot odds</span>
        <span data-testid="jackpot-odds" data-odds={data.odds} className="font-mono text-[13px] tabular-nums">
          1 in {groupDigits(data.odds)}
        </span>
      </div>
      <a
        href={`#/rounds?r=${encodeURIComponent(data.round)}`}
        data-testid="round-link"
        className="flex min-h-11 items-center text-[13px] font-medium"
      >
        Round {data.round} details
      </a>
    </section>
  );
}
