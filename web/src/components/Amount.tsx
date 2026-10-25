// Amount (spec 4): KAS with 2 decimals; tap for the exact sompi. Figures arrive as decimal strings from
// chain state or the wasm and are only formatted here.
import { useState } from 'react';
import { cn } from '@/lib/cn';
import { kas2, sompiExact } from '@/lib/format';

export interface AmountProps {
  /** Sompi, a decimal string. */
  sompi: string;
  className?: string;
  testId?: string;
}

export function Amount({ sompi, className, testId = 'amount' }: AmountProps) {
  const [exact, setExact] = useState(false);
  const text = exact ? `${sompiExact(sompi)} sompi` : `${kas2(sompi)} KAS`;
  return (
    <button
      type="button"
      data-testid={testId}
      data-sompi={sompi}
      aria-pressed={exact}
      onClick={() => setExact((x) => !x)}
      className={cn('min-h-11 shrink-0 cursor-pointer text-left font-mono whitespace-nowrap tabular-nums', className)}
    >
      {text}
    </button>
  );
}
