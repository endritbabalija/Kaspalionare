// Ticket builder row (spec 4): a combination and its remove button. The five main numbers and the bonus are
// shown as figures; the row's accessible name says them in words for screen readers.
import { Button } from './ui/button';

export interface TicketRowProps {
  index: number;
  main: readonly number[];
  bonus: number;
  onRemove?: (index: number) => void;
}

export function TicketRow({ index, main, bonus, onRemove }: TicketRowProps) {
  const label = `${main.join(' ')} bonus ${bonus}`;
  return (
    <li
      data-testid={`ticket-row-${index}`}
      data-combination={`${main.join(',')}+${bonus}`}
      aria-label={label}
      className="flex min-h-11 items-center justify-between gap-2 border-b border-rule-soft py-1 last:border-b-0"
    >
      <span className="flex flex-wrap items-center gap-1.5 font-mono text-[15px] tabular-nums" aria-hidden="true">
        {main.map((n) => (
          <span
            key={n}
            className="inline-flex h-8 min-w-8 items-center justify-center rounded-full bg-ink px-1 text-on-ink"
          >
            {n.toString().padStart(2, '0')}
          </span>
        ))}
        <span className="inline-flex h-8 min-w-8 items-center justify-center rounded-[8px] bg-teal px-1 text-on-teal">
          {bonus}
        </span>
      </span>
      {onRemove && (
        <Button
          variant="ghost"
          size="icon"
          aria-label={`Remove ${label}`}
          data-testid={`ticket-remove-${index}`}
          onClick={() => onRemove(index)}
        >
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.8"
            aria-hidden="true"
          >
            <path d="M6 6l12 12M18 6L6 18" />
          </svg>
        </Button>
      )}
    </li>
  );
}
