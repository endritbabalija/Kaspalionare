// S1's number picker (spec 3): 5 of 1..30, then a bonus in 1..m (m from the round's live shards), quick pick
// and clear, and "Add" to put the combination on the ticket (up to B_MAX).
import { useId } from 'react';
import { BonusTile } from '@/components/BonusTile';
import { NumberBall, type NumberBallState } from '@/components/NumberBall';
import { Button } from '@/components/ui/button';
import { MAIN_MAX, MAIN_PICKS, complete, type Pick } from '@/play/picker';

export interface NumberPickerProps {
  pick: Pick;
  m: number;
  /** The ticket is full (B_MAX combinations): nothing more can be added. */
  ticketFull: boolean;
  disabled?: boolean;
  onMain: (n: number) => void;
  onBonus: (n: number) => void;
  onQuickPick: () => void;
  onClear: () => void;
  onAdd: () => void;
}

export function NumberPicker(p: NumberPickerProps) {
  const title = useId();
  const bonusTitle = useId();
  const full = p.pick.main.length >= MAIN_PICKS;
  const ballState = (n: number): NumberBallState => {
    if (p.disabled) return 'disabled';
    if (p.pick.main.includes(n)) return 'picked';
    return full ? 'full' : 'free';
  };
  return (
    <section
      data-testid="number-picker"
      aria-labelledby={title}
      className="flex flex-col gap-3 rounded-[14px] border border-rule bg-surface p-3"
    >
      <div className="flex items-center justify-between gap-2">
        <h2 id={title} className="m-0 text-[17px] font-semibold">
          Numbers
        </h2>
        <span data-testid="picked-count" className="font-mono text-[13px] text-graphite tabular-nums">
          {p.pick.main.length}/{MAIN_PICKS}
        </span>
      </div>
      <div className="grid grid-cols-6 gap-1.5" role="group" aria-labelledby={title}>
        {Array.from({ length: MAIN_MAX }, (_, i) => i + 1).map((n) => (
          <NumberBall key={n} n={n} state={ballState(n)} onToggle={p.onMain} />
        ))}
      </div>
      <h3 id={bonusTitle} className="m-0 text-[15px] font-semibold">
        Bonus
      </h3>
      <div className="flex flex-wrap gap-2" role="group" aria-labelledby={bonusTitle} data-testid="bonus-tiles">
        {Array.from({ length: p.m }, (_, i) => i + 1).map((n) => (
          <BonusTile key={n} n={n} state={p.pick.bonus === n ? 'picked' : 'free'} onToggle={p.onBonus} />
        ))}
      </div>
      <div className="grid grid-cols-3 gap-2">
        <Button
          data-testid="quick-pick"
          className="px-2 whitespace-nowrap"
          onClick={p.onQuickPick}
          disabled={p.disabled}
        >
          Quick pick
        </Button>
        <Button data-testid="clear-pick" onClick={p.onClear} disabled={p.disabled}>
          Clear
        </Button>
        <Button
          variant="primary"
          data-testid="add-combination"
          onClick={p.onAdd}
          disabled={p.disabled || p.ticketFull || !complete(p.pick, p.m)}
        >
          Add
        </Button>
      </div>
    </section>
  );
}
