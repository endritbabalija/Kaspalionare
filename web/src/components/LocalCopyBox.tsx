// Local-copy box (spec 3 S2, 4): "Saved on this device". States: complete and verified (every round the
// player still holds tickets in is copied and its lists rebuild a live chain object's frozen roots),
// incomplete (a round is not copied yet, or only while open), mismatch (a kept copy no longer rebuilds the
// chain's roots), not needed (no live ticket). With the backup download (S8).
import { useId } from 'react';
import { FigureRow } from './FigureRow';
import { Button } from './ui/button';

export type LocalCopyState = 'complete' | 'incomplete' | 'mismatch' | 'not-needed';

const label: Record<LocalCopyState, string> = {
  complete: 'Complete and verified',
  incomplete: 'Incomplete',
  mismatch: 'Mismatch',
  'not-needed': 'Not needed',
};

export interface LocalCopyBoxProps {
  state: LocalCopyState;
  /** Rounds kept on this device. */
  rounds: number;
  tickets: number;
  onDownload?: () => void;
  downloading?: boolean;
}

export function LocalCopyBox({ state, rounds, tickets, onDownload, downloading }: LocalCopyBoxProps) {
  const title = useId();
  return (
    <section
      data-testid="local-copy"
      data-state={state}
      aria-labelledby={title}
      className="flex flex-col gap-0.5 rounded-[14px] border border-rule bg-surface p-4"
    >
      <h2 id={title} className="m-0 mb-1 text-[17px] font-semibold">
        Saved on this device
      </h2>
      <FigureRow name="Status" testId="local-copy-status-row">
        <span
          data-testid="local-copy-status"
          data-state={state}
          className={state === 'mismatch' ? 'text-amber' : undefined}
        >
          {label[state]}
        </span>
      </FigureRow>
      <FigureRow name="Rounds" testId="local-copy-rounds-row">
        <span data-testid="local-copy-rounds">{rounds}</span>
      </FigureRow>
      <FigureRow name="Tickets" testId="local-copy-tickets-row">
        <span data-testid="local-copy-tickets">{tickets}</span>
      </FigureRow>
      <Button data-testid="local-copy-download" disabled={downloading || !onDownload} onClick={onDownload}>
        Download backup
      </Button>
    </section>
  );
}
