// Number ball (spec 4): free, picked, matched, full (no more picks), disabled. A real toggle button; its
// name is the number, its pressed state says picked, and a matched ball also says "matched" to screen
// readers (spec 5: no meaning by colour alone).
import { cn } from '@/lib/cn';

export type NumberBallState = 'free' | 'picked' | 'matched' | 'full' | 'disabled';

export interface NumberBallProps {
  n: number;
  state: NumberBallState;
  onToggle?: (n: number) => void;
}

const look: Record<NumberBallState, string> = {
  free: 'border border-control-line bg-surface text-ink',
  picked: 'border border-ink bg-ink text-on-ink',
  matched: 'border border-mint bg-mint text-on-mint',
  full: 'border border-dashed border-control-line bg-transparent text-graphite',
  disabled: 'border border-rule bg-transparent text-graphite',
};

export function NumberBall({ n, state, onToggle }: NumberBallProps) {
  const label = n.toString().padStart(2, '0');
  const inactive = state === 'full' || state === 'disabled';
  return (
    <button
      type="button"
      data-testid={`number-ball-${n}`}
      data-state={state}
      aria-pressed={state === 'picked' || state === 'matched'}
      aria-label={state === 'matched' ? `${n}, matched` : `${n}`}
      disabled={inactive}
      onClick={() => onToggle?.(n)}
      className={cn(
        'h-12 min-w-11 rounded-full font-mono text-base tabular-nums',
        look[state],
        inactive && 'cursor-not-allowed',
      )}
    >
      {label}
    </button>
  );
}
