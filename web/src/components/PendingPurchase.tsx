// Pending purchase (S2, F4 review note): a signed purchase whose outcome the page did not learn and that is
// no longer sent again by itself (more than RESEND_MS after its signature, tickets/load.ts). States: unknown
// (waiting for the player), sending, failed with its message. Send sends exactly the transaction that was
// signed then (same id, same inputs: it cannot buy twice); nothing is built or signed here.
import type { StoredPending } from '@wasm/web.js';
import { BallRow } from './BallRow';
import { FigureRow } from './FigureRow';
import { HashOrAddress } from './HashOrAddress';
import { Button } from './ui/button';

export type PendingPurchaseState = 'unknown' | 'sending' | 'failed';

export interface PendingPurchaseProps {
  testId: string;
  pending: StoredPending;
  state: PendingPurchaseState;
  error?: string;
  disabled?: boolean;
  onSend?: () => void;
  explorer?: string;
}

const label: Record<PendingPurchaseState, string> = { unknown: 'Unknown', sending: 'Checking', failed: 'Unknown' };

export function PendingPurchase(p: PendingPurchaseProps) {
  const t = p.testId;
  return (
    <article
      data-testid={t}
      data-state={p.state}
      data-key={p.pending.key}
      aria-label={`Round ${p.pending.round} purchase`}
      className="flex flex-col gap-2 rounded-[14px] border border-rule bg-surface p-4"
    >
      <h2 className="m-0 text-[17px] font-semibold">Round {p.pending.round}</h2>
      <FigureRow name="Status" testId={`${t}-status-row`}>
        <span data-testid={`${t}-status`} data-state={p.state}>
          {label[p.state]}
        </span>
      </FigureRow>
      <ul className="m-0 flex list-none flex-col gap-2 p-0">
        {p.pending.bundle.map((c, i) => (
          <li key={i} className="border-t border-rule-soft pt-2">
            <BallRow combination={c} testId={`${t}-balls-${i}`} />
          </li>
        ))}
      </ul>
      <FigureRow name="Transaction" testId={`${t}-tx-row`}>
        <HashOrAddress value={p.pending.key} kind="transaction" explorer={p.explorer} testId={`${t}-tx`} />
      </FigureRow>
      {p.error && (
        <p role="alert" data-testid={`${t}-error`} className="m-0 text-[13px] text-amber">
          {p.error}
        </p>
      )}
      <Button
        variant="primary"
        data-testid={`${t}-send`}
        disabled={p.disabled || p.state === 'sending'}
        onClick={p.onSend}
      >
        Send
      </Button>
    </article>
  );
}
