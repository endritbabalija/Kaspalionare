// Vault position card (spec 4: position, pending request, settle due; 3 S5): one of the connected wallet's share
// positions (its shares and what they are worth today), or one of its requests (deposit or withdrawal, the draw
// whose price converts it, where it is: pending, settle due, or unknown when the indexer did not list the draw's
// record). Every figure is the wasm's (`vaultView`) and links to the object's transaction (rule 1). The action
// (the withdrawal form, the settlement's rows and press) is the card's child.
import type { ReactNode } from 'react';
import type { PositionCard, RequestCard } from '@wasm/web.js';
import { messageOf } from '@/lib/errors';
import { Amount } from './Amount';
import { FigureRow } from './FigureRow';
import { Shares } from './Shares';
import { SourceLink } from './SourceLink';
import { requestCardState } from './vaultStates';

export type VaultCardProps = {
  testId: string;
  explorer?: string;
  children?: ReactNode;
} & ({ position: PositionCard; request?: undefined } | { request: RequestCard; position?: undefined });

const STATUS = { pending: 'Pending', settleDue: 'Settle due', unknown: 'Unknown' } as const;

export function VaultPositionCard(p: VaultCardProps) {
  const id = p.testId;
  const state = p.position ? 'position' : requestCardState(p.request);
  const tx = (p.position ?? p.request).source.transactionId;
  const title = p.position ? 'Position' : p.request.kind === 'deposit' ? 'Deposit request' : 'Withdrawal request';
  return (
    <article
      data-testid={id}
      data-state={state}
      aria-labelledby={`${id}-title`}
      className="flex flex-col gap-0.5 rounded-[14px] border border-rule bg-surface p-4"
    >
      <h3 id={`${id}-title`} className="m-0 flex min-h-11 items-center justify-between text-[15px] font-semibold">
        {title}
        <SourceLink tx={tx} name={title} explorer={p.explorer} testId={`${id}-source`} />
      </h3>
      {p.position && (
        <>
          <FigureRow name="Shares" testId={`${id}-shares-row`}>
            <Shares value={p.position.shares} testId={`${id}-shares`} />
          </FigureRow>
          <FigureRow name="Worth today" testId={`${id}-worth-row`}>
            <Amount sompi={p.position.worth} testId={`${id}-worth`} />
          </FigureRow>
          {!p.position.ownerKey && (
            <p role="status" data-testid={`${id}-owner-type`} className="m-0 text-[13px] text-amber">
              {messageOf('owner_type')}
            </p>
          )}
        </>
      )}
      {p.request && (
        <>
          {p.request.kind === 'deposit' ? (
            <FigureRow name="Amount" testId={`${id}-amount-row`}>
              <Amount sompi={p.request.amount} testId={`${id}-amount`} />
            </FigureRow>
          ) : (
            <>
              <FigureRow name="Shares" testId={`${id}-amount-row`}>
                <Shares value={p.request.amount} testId={`${id}-amount`} />
              </FigureRow>
              <FigureRow name="Worth today" testId={`${id}-worth-row`} part>
                <Amount sompi={p.request.worth} testId={`${id}-worth`} />
              </FigureRow>
            </>
          )}
          <FigureRow name="Price" testId={`${id}-price-row`}>
            <span data-testid={`${id}-price`} data-value={p.request.round}>
              Round {p.request.round} draw
            </span>
          </FigureRow>
          <FigureRow name="Held" testId={`${id}-held-row`}>
            <Amount sompi={p.request.value} testId={`${id}-held`} />
          </FigureRow>
          <FigureRow name="Status" testId={`${id}-status-row`}>
            <span data-testid={`${id}-status`} data-value={p.request.status}>
              {STATUS[p.request.status]}
            </span>
          </FigureRow>
        </>
      )}
      {p.children}
    </article>
  );
}
