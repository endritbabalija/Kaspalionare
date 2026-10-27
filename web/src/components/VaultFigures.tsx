// The vault's figures (spec 3 S5): its value, the share price (A + VA) / (S + VS), the liability L it keeps, the
// pending deposits and withdrawals, the round whose draw converts requests made now, and from when the vault takes
// the next request (VAULT_REQ_DELAY after its last spend; clock and DAA). Every figure is the wasm's (`vaultView`,
// from the live vault checked against the node) and links to the vault's transaction (rule 1).
import type { VaultSummary } from '@wasm/web.js';
import { Amount } from './Amount';
import { FigureRow } from './FigureRow';
import { Shares } from './Shares';
import { SourceLink } from './SourceLink';
import { Time } from './Time';

export interface VaultFiguresProps {
  vault: VaultSummary;
  clock: { virtualDaa: string; observedAt: number };
  explorer?: string;
  /** The figures are the last ones checked; the latest read failed. */
  stale?: boolean;
  testId?: string;
}

export function VaultFigures({ vault: v, clock, explorer, stale, testId = 'vault-figures' }: VaultFiguresProps) {
  const tx = v.source.transactionId;
  const src = (name: string, id: string) => (
    <SourceLink tx={tx} name={name} explorer={explorer} testId={`${testId}-${id}-source`} />
  );
  return (
    <section
      data-testid={testId}
      data-state={stale ? 'stale' : 'ready'}
      aria-labelledby={`${testId}-title`}
      className="flex flex-col gap-0.5 rounded-[14px] border border-rule bg-surface p-4"
    >
      <h1 id={`${testId}-title`} className="m-0 pb-1 text-[17px] font-semibold">
        Vault
      </h1>
      {stale && (
        <p role="status" data-testid={`${testId}-stale`} className="m-0 text-[13px] text-amber">
          Stale
        </p>
      )}
      <FigureRow name="Vault value" testId={`${testId}-value-row`}>
        <Amount sompi={v.totalAssets} testId={`${testId}-value`} />
        {src('Vault value', 'value')}
      </FigureRow>
      <FigureRow name="Share price" testId={`${testId}-price-row`}>
        <span data-testid={`${testId}-price`} data-value={v.sharePrice} className="font-mono tabular-nums">
          {v.sharePrice} sompi
        </span>
        {src('Share price', 'price')}
      </FigureRow>
      <FigureRow name="Liability" testId={`${testId}-liability-row`}>
        <Amount sompi={v.liability} testId={`${testId}-liability`} />
        {src('Liability', 'liability')}
      </FigureRow>
      <FigureRow name="Pending deposits" testId={`${testId}-deposits-row`}>
        <Amount sompi={v.pendingDeposits} testId={`${testId}-deposits`} />
        {src('Pending deposits', 'deposits')}
      </FigureRow>
      <FigureRow name="Pending withdrawals" testId={`${testId}-withdrawals-row`}>
        <Shares value={v.pendingWithdrawals} testId={`${testId}-withdrawals`} />
        {src('Pending withdrawals', 'withdrawals')}
      </FigureRow>
      {v.pendingWithdrawals !== '0' && (
        <FigureRow name="Worth today" testId={`${testId}-withdrawals-worth-row`} part>
          <Amount sompi={v.pendingWithdrawalsWorth} testId={`${testId}-withdrawals-worth`} />
        </FigureRow>
      )}
      <FigureRow name="Next draw" testId={`${testId}-draw-row`}>
        <span data-testid={`${testId}-draw`} data-value={v.nextDraw} className="font-mono">
          Round {v.nextDraw}
        </span>
        {src('Next draw', 'draw')}
      </FigureRow>
      <FigureRow name="Next request" testId={`${testId}-request-row`}>
        {v.requestOpen ? (
          <span data-testid={`${testId}-request`} data-open="true">
            Now
          </span>
        ) : (
          <span data-testid={`${testId}-request`} data-open="false">
            <Time
              daa={v.requestFrom}
              virtualDaa={clock.virtualDaa}
              observedAt={clock.observedAt}
              testId={`${testId}-request-time`}
            />
          </span>
        )}
        {src('Next request', 'request')}
      </FigureRow>
    </section>
  );
}
