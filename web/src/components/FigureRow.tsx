// A figure and its name (spec 5 rule 6: a figure row is data, not helper text): the name on the left, the
// value on the right (an Amount, a Time, a count or a status value).
import type { ReactNode } from 'react';
import { cn } from '@/lib/cn';

export function FigureRow({
  name,
  children,
  testId,
  part,
}: {
  name: string;
  children: ReactNode;
  testId: string;
  /** A fact about the row before it (indented). */
  part?: boolean;
}) {
  return (
    <div
      data-testid={testId}
      className={cn(
        'flex min-h-11 items-center justify-between gap-3 text-sm',
        part && 'ml-3 border-l border-rule pl-3 text-[13px] text-graphite',
      )}
    >
      <span>{name}</span>
      <span className="flex min-w-0 items-center justify-end text-right font-mono text-[13px]">{children}</span>
    </div>
  );
}
