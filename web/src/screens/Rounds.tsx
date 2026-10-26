// S3 Rounds (spec 3 S3, 4; F5): the list, one row per round (`#/rounds`), or one round's detail with Verify and
// Run the round (`#/rounds?r=<n>`). Loading, empty, error states.
import { useEffect, useState } from 'react';
import { useSession } from '@/app/context';
import { FigureRow } from '@/components/FigureRow';
import { RoundRow } from '@/components/RoundRow';
import { Button } from '@/components/ui/button';
import { RoundDetail } from './rounds/RoundDetail';
import { useRounds } from './rounds/useRounds';

/** The round named in the hash (`#/rounds?r=3`), if any. */
function roundOfHash(hash: string): string | null {
  const q = hash.split('?')[1] ?? '';
  const r = new URLSearchParams(q).get('r');
  return r && /^[1-9]\d{0,18}$/.test(r) ? r : null;
}

export function Rounds() {
  const [r, setR] = useState(() => roundOfHash(window.location.hash));
  useEffect(() => {
    const on = () => setR(roundOfHash(window.location.hash));
    window.addEventListener('hashchange', on);
    return () => window.removeEventListener('hashchange', on);
  }, []);
  return r ? <RoundDetail key={r} r={r} /> : <RoundList />;
}

function RoundList() {
  const ctx = useSession();
  const t = useRounds(ctx);
  if (t.load.kind === 'loading') {
    return (
      <section
        data-testid="rounds"
        data-state="loading"
        aria-busy="true"
        aria-label="Rounds"
        className="flex flex-col gap-3"
      >
        <h1 className="m-0 text-[17px] font-semibold">Rounds</h1>
        <span className="h-28 w-full animate-pulse rounded-[14px] bg-rule-soft" />
        <span className="sr-only">Loading</span>
      </section>
    );
  }
  if (t.load.kind === 'error') {
    return (
      <section data-testid="rounds" data-state="error" aria-label="Rounds" className="flex flex-col gap-3">
        <h1 className="m-0 text-[17px] font-semibold">Rounds</h1>
        <p role="alert" data-testid="rounds-error" data-code={t.load.code ?? ''} className="m-0 text-amber">
          {t.load.message}
        </p>
      </section>
    );
  }
  const rows = t.load.rows;
  return (
    <section
      data-testid="rounds"
      data-state={rows.length ? 'ready' : 'empty'}
      aria-labelledby="rounds-title"
      className="flex flex-col gap-3"
    >
      <h1 id="rounds-title" className="m-0 text-[17px] font-semibold">
        Rounds
      </h1>
      {!rows.length && (
        <div className="rounded-[14px] border border-rule bg-surface p-4">
          <FigureRow name="Rounds" testId="rounds-empty-count">
            0
          </FigureRow>
        </div>
      )}
      {rows.slice(0, t.shown).map((row) => (
        <RoundRow key={row.round} round={row.round} data={t.checked[row.round]} testId={`round-row-${row.round}`} />
      ))}
      {rows.length > t.shown && (
        <Button variant="secondary" data-testid="rounds-more" onClick={t.more}>
          More
        </Button>
      )}
    </section>
  );
}
