// Time (spec 4, rule 5): a DAA score shown as the local clock and as the score itself, converted at 10 DAA
// per second from the node's virtual DAA score at a known moment.
import { clock, daaToMs, groupDigits } from '@/lib/format';
import { cn } from '@/lib/cn';

export interface TimeProps {
  daa: string;
  virtualDaa: string;
  observedAt: number;
  now?: number;
  className?: string;
  testId?: string;
}

export function Time({ daa, virtualDaa, observedAt, now = Date.now(), className, testId = 'time' }: TimeProps) {
  const ms = daaToMs(daa, virtualDaa, observedAt);
  return (
    <span data-testid={testId} className={cn('inline-flex items-baseline gap-2 font-mono tabular-nums', className)}>
      <time dateTime={new Date(ms).toISOString()}>{clock(ms, now)}</time>
      <span className="text-graphite">DAA {groupDigits(daa)}</span>
    </span>
  );
}
