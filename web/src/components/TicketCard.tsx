// Ticket card (spec 3 S2, 4; mock-up board 3). States: open, drawn (win, no win, partly claimed, fully
// claimed), void, deposit returnable, deposit returned; and incomplete (a winning tier whose pool the copy does
// not show, F4 review M2). One row per combination with its balls (matched ones marked, also for screen
// readers), its tier or "No win", its status when a won tier pays nothing now (Expired, No pool, Unknown),
// and a claim row for each winning one; the payout row when the ticket pays another address than the
// connected wallet (F4 review S2); then the
// ticket's figures: what it holds (its deposits and accrued prizes), when that comes back (with the last
// claim, or the date return_deposit() is valid: the real date for this ticket after the draw), the sales
// deadline while open, and Return deposit when it is valid. Every figure is the wasm's (ticketCard,
// claimFigures, returnFigures) or the chain's; nothing is computed here.
import type {
  ClaimFigures,
  ComboCard,
  ReturnFigures,
  RoundStatus,
  StoredTicket,
  TicketCard as Card,
} from '@wasm/web.js';
import { Amount } from './Amount';
import { BallRow } from './BallRow';
import { ClaimRow, type ClaimTx } from './ClaimRow';
import { FigureRow } from './FigureRow';
import { HashOrAddress } from './HashOrAddress';
import { RoundStatusPill } from './RoundStatusPill';
import { Time } from './Time';
import { TxProgress } from './TxProgress';
import { ticketCardState, type TicketCardState } from './ticketStates';
import { Button } from './ui/button';

export interface ActionView<F> {
  figures?: F;
  estimating?: boolean;
  tx?: ClaimTx;
  error?: string;
}

export interface TicketCardProps {
  testId: string;
  stored: StoredTicket;
  /** The round's status as the chain shows it (the pill). */
  status: RoundStatus;
  card?: Card;
  /** The ticket's entry is not on the node any more (it ended outside this browser). */
  spent?: boolean;
  clock: { virtualDaa: string; observedAt: number };
  claims?: Record<number, ActionView<ClaimFigures>>;
  ret?: ActionView<ReturnFigures>;
  busy?: boolean;
  onClaim?: (index: number) => void;
  onReturn?: () => void;
  explorer?: string;
  /** The connected wallet's address (the payout row shows when the ticket pays elsewhere). */
  wallet?: string;
}

const comboStatus: Partial<Record<ComboCard['status'], string>> = {
  expired: 'Expired',
  noPool: 'No pool',
  unknown: 'Unknown',
};

const label: Record<TicketCardState, string> = {
  open: 'Open',
  win: 'Win',
  'no-win': 'No win',
  'partly-claimed': 'Partly claimed',
  claimed: 'Claimed',
  void: 'No draw: void',
  returnable: 'Deposit returnable',
  returned: 'Deposit returned',
  spent: 'Spent',
  incomplete: 'Incomplete',
};

export function TicketCard(p: TicketCardProps) {
  const state = ticketCardState(p);
  const { stored, card, clock } = p;
  const t = p.testId;
  return (
    <article
      data-testid={t}
      data-state={state}
      data-round={stored.round}
      data-key={stored.key}
      aria-label={`Round ${stored.round} ticket`}
      className="flex flex-col gap-2 rounded-[14px] border border-rule bg-surface p-4"
    >
      <header className="flex items-center justify-between gap-2">
        <h2 className="m-0 text-[17px] font-semibold">Round {stored.round}</h2>
        <RoundStatusPill status={p.status} />
      </header>
      <FigureRow name="Status" testId={`${t}-status-row`}>
        <span data-testid={`${t}-status`} data-state={state}>
          {label[state]}
        </span>
      </FigureRow>
      <ul className="m-0 flex list-none flex-col gap-2 p-0">
        {(
          card?.combos ??
          stored.bundle.map(
            (combination, index) =>
              ({ index, combination }) as Partial<ComboCard> & Pick<ComboCard, 'index' | 'combination'>,
          )
        ).map((c) => {
          const cc = c.status !== undefined ? (c as ComboCard) : undefined;
          const view = p.claims?.[c.index];
          return (
            <li
              key={c.index}
              data-testid={`${t}-combo-${c.index}`}
              data-status={cc?.status ?? ''}
              className="flex flex-col gap-1.5 border-t border-rule-soft pt-2"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <BallRow
                  combination={c.combination}
                  matchedMain={cc?.matchedMain}
                  bonusMatched={cc?.bonusMatched}
                  testId={`${t}-balls-${c.index}`}
                />
                {cc && cc.status !== 'open' && (
                  <span data-testid={`${t}-tier-${c.index}`} className="font-mono text-[13px] text-graphite">
                    {cc.tierName ?? 'No win'}
                  </span>
                )}
                {cc && comboStatus[cc.status] && (
                  <span data-testid={`${t}-combo-status-${c.index}`} className="font-mono text-[13px] text-amber">
                    {comboStatus[cc.status]}
                  </span>
                )}
              </div>
              {cc && (cc.status === 'claimable' || cc.status === 'claimed') && cc.tier !== undefined && (
                <ClaimRow
                  testId={`${t}-claim-${c.index}`}
                  claim={cc.claim}
                  claimed={cc.status === 'claimed'}
                  figures={view?.figures}
                  estimating={view?.estimating}
                  deadline={card?.claimDeadline}
                  clock={clock}
                  tx={view?.tx}
                  error={view?.error}
                  disabled={p.busy}
                  onClaim={() => p.onClaim?.(c.index)}
                  explorer={p.explorer}
                  withLastClaim={card?.backWithLastClaim}
                  returnFrom={card?.returnFrom}
                  wallet={p.wallet}
                />
              )}
            </li>
          );
        })}
      </ul>
      {card && !stored.ended && (
        <>
          {p.wallet && card.payout !== p.wallet && (
            <FigureRow name="Payout" testId={`${t}-payout-row`}>
              <HashOrAddress value={card.payout} kind="address" explorer={p.explorer} testId={`${t}-payout`} />
            </FigureRow>
          )}
          <FigureRow name="Held" testId={`${t}-value-row`}>
            <Amount sompi={card.value} testId={`${t}-value`} />
          </FigureRow>
          <FigureRow name="Back" testId={`${t}-back-row`} part>
            {card.backWithLastClaim ? (
              <span data-testid={`${t}-back`} data-value="last-claim">
                With last claim
              </span>
            ) : (
              <Time
                daa={card.returnFrom}
                virtualDaa={clock.virtualDaa}
                observedAt={clock.observedAt}
                testId={`${t}-back`}
              />
            )}
          </FigureRow>
          {card.stage === 'open' && (
            <FigureRow name="Deadline" testId={`${t}-deadline-row`}>
              <Time
                daa={card.deadline}
                virtualDaa={clock.virtualDaa}
                observedAt={clock.observedAt}
                testId={`${t}-deadline`}
              />
            </FigureRow>
          )}
        </>
      )}
      {card?.returnable && !stored.ended && (
        <section
          aria-label="Return deposit"
          data-testid={`${t}-return`}
          className="flex flex-col gap-0.5 border-t border-rule-soft pt-2"
        >
          <FigureRow name="Deposit back" testId={`${t}-return-amount-row`}>
            <Amount sompi={p.ret?.figures?.amount ?? card.value} testId={`${t}-return-amount`} />
          </FigureRow>
          <FigureRow name="Network fee" testId={`${t}-return-fee-row`}>
            {p.ret?.figures ? (
              <Amount sompi={p.ret.figures.fee} testId={`${t}-return-fee`} />
            ) : (
              <span data-testid={`${t}-return-fee`} className="text-graphite">
                <span aria-hidden="true">–</span>
                <span className="sr-only">{p.ret?.estimating ? 'Estimating' : 'none'}</span>
              </span>
            )}
          </FigureRow>
          {p.ret?.error && !p.ret.tx && (
            <p role="alert" data-testid={`${t}-return-error`} className="m-0 text-[13px] text-amber">
              {p.ret.error}
            </p>
          )}
          <Button
            variant="primary"
            data-testid={`${t}-return-button`}
            disabled={!p.ret?.figures || p.busy || (!!p.ret.tx && !p.ret.tx.failed)}
            onClick={p.onReturn}
          >
            Return deposit
          </Button>
          {p.ret?.tx && <TxProgress step={p.ret.tx.step} failed={p.ret.tx.failed} checking={p.ret.tx.checking} />}
        </section>
      )}
      {stored.ended && (
        <>
          <FigureRow name="Paid" testId={`${t}-ended-row`}>
            <Amount sompi={stored.ended.amount} testId={`${t}-ended-amount`} />
          </FigureRow>
          <FigureRow name="Transaction" testId={`${t}-ended-tx-row`} part>
            <HashOrAddress
              value={stored.ended.transactionId}
              kind="transaction"
              explorer={p.explorer}
              testId={`${t}-ended-tx`}
            />
          </FigureRow>
        </>
      )}
    </article>
  );
}
