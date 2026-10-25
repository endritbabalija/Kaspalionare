// Round status pill (spec 4): Open, Closing, Drawn, Void. The status is the text (never colour alone).
import type { RoundStatus } from '@wasm/web.js';
import { cn } from '@/lib/cn';

const look: Record<RoundStatus, { text: string; cls: string }> = {
  open: { text: 'Open', cls: 'bg-status-open-bg text-status-open-fg' },
  closing: { text: 'Closing', cls: 'bg-status-closing-bg text-status-closing-fg' },
  drawn: { text: 'Drawn', cls: 'bg-status-drawn-bg text-status-drawn-fg' },
  void: { text: 'Void', cls: 'border border-control-line text-status-void-fg' },
};

export function RoundStatusPill({ status }: { status: RoundStatus }) {
  const l = look[status];
  return (
    <span
      data-testid="round-status"
      data-status={status}
      className={cn('inline-flex h-6 items-center rounded-full px-2.5 text-xs font-medium', l.cls)}
    >
      {l.text}
    </span>
  );
}
