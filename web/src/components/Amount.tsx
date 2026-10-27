// Amount (spec 4): KAS with 2 decimals; tap for the exact sompi. Figures arrive as decimal strings from
// chain state or the wasm and are only formatted here.
import { useState } from 'react';
import { cn } from '@/lib/cn';
import { kas2, kasExact, sompiExact } from '@/lib/format';

export interface AmountProps {
  /** Sompi, a decimal string. */
  sompi: string;
  /** KAS exactly, in the form the amount field reads (`kasExact`), instead of 2 decimals. */
  exact?: boolean;
  className?: string;
  testId?: string;
}

export function Amount({ sompi, exact: plain = false, className, testId = 'amount' }: AmountProps) {
  const [exact, setExact] = useState(false);
  const text = exact ? `${sompiExact(sompi)} sompi` : `${plain ? kasExact(sompi) : kas2(sompi)} KAS`;
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
