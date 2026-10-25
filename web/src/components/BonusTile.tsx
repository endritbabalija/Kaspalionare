// Bonus tile (spec 4): free, picked, matched. A toggle button like the number ball, square-cornered.
import { cn } from '@/lib/cn';

export type BonusTileState = 'free' | 'picked' | 'matched';

export interface BonusTileProps {
  n: number;
  state: BonusTileState;
  onToggle?: (n: number) => void;
}

const look: Record<BonusTileState, string> = {
  free: 'border border-control-line bg-surface text-ink',
  picked: 'border border-teal bg-teal text-on-teal',
  matched: 'border border-mint bg-mint text-on-mint',
};

export function BonusTile({ n, state, onToggle }: BonusTileProps) {
  return (
    <button
      type="button"
      data-testid={`bonus-tile-${n}`}
      data-state={state}
      aria-pressed={state !== 'free'}
      aria-label={state === 'matched' ? `Bonus ${n}, matched` : `Bonus ${n}`}
      onClick={() => onToggle?.(n)}
      className={cn('h-12 min-w-11 rounded-[10px] font-mono text-base tabular-nums', look[state])}
    >
      {n}
    </button>
  );
}
