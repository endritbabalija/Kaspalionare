// A combination shown (not picked): five main balls and the bonus tile, matched ones in the win fill and
// named "matched" for screen readers (spec 5: no meaning by colour alone). Display only: no controls.
import type { Combination } from '@wasm/web.js';
import { cn } from '@/lib/cn';

export interface BallRowProps {
  combination: Combination;
  /** After the draw: which main numbers are winning numbers. */
  matchedMain?: readonly boolean[];
  bonusMatched?: boolean;
  testId?: string;
}

export function BallRow({ combination, matchedMain, bonusMatched, testId = 'ball-row' }: BallRowProps) {
  const label = `${combination.main.join(' ')}, bonus ${combination.bonus}`;
  return (
    <ol data-testid={testId} aria-label={label} className="m-0 flex list-none flex-wrap items-center gap-1.5 p-0">
      {combination.main.map((n, i) => {
        const hit = matchedMain?.[i] === true;
        return (
          <li
            key={`${i}-${n}`}
            data-testid={`${testId}-main-${i}`}
            data-matched={hit ? 'true' : 'false'}
            className={cn(
              'flex h-8 w-8 items-center justify-center rounded-full font-mono text-[13px] tabular-nums',
              hit ? 'border border-mint bg-mint text-on-mint' : 'border border-control-line text-ink',
            )}
          >
            {n.toString().padStart(2, '0')}
            {hit && <span className="sr-only">, matched</span>}
          </li>
        );
      })}
      <li
        data-testid={`${testId}-bonus`}
        data-matched={bonusMatched ? 'true' : 'false'}
        className={cn(
          'ml-1 flex h-8 min-w-8 items-center justify-center rounded-[8px] px-1 font-mono text-[13px] tabular-nums',
          bonusMatched ? 'border border-mint bg-mint text-on-mint' : 'border border-teal text-ink',
        )}
      >
        <span className="sr-only">Bonus </span>
        {combination.bonus}
        {bonusMatched && <span className="sr-only">, matched</span>}
      </li>
    </ol>
  );
}
