// A share count (vault shares, D66): grouped digits, mono.
import { groupDigits } from '@/lib/format';

export function Shares({ value, testId }: { value: string; testId: string }) {
  return (
    <span data-testid={testId} data-value={value} className="font-mono tabular-nums">
      {groupDigits(value)} shares
    </span>
  );
}
