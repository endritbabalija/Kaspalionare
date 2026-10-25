// S1's buy card (spec 3, 7, mock-up board 2): the picker, the ticket builder, the cost panel as figure rows
// from the built transaction (price, deposit with its return date, network fee, referral with its
// destination, total), the buy button and the transaction progress. After the deadline: "Sales closed;
// waiting for close()" with a link to Rounds (S4).
import { useId } from 'react';
import type { BuyFigures } from '@wasm/web.js';
import { CostPanel, type CostRow } from '@/components/CostPanel';
import { HashOrAddress } from '@/components/HashOrAddress';
import { TicketRow } from '@/components/TicketRow';
import { Time } from '@/components/Time';
import { TxProgress } from '@/components/TxProgress';
import { Button } from '@/components/ui/button';
import { href } from '@/app/routes';
import { NumberPicker } from './NumberPicker';
import type { usePlay } from './usePlay';

type Play = ReturnType<typeof usePlay>;

const card = 'flex flex-col gap-2 rounded-[14px] border border-rule bg-surface p-4';

function rows(
  f: BuyFigures | undefined,
  clock: { virtualDaa: string; observedAt: number } | undefined,
  explorer?: string,
): CostRow[] {
  return [
    { id: 'price', name: 'Price', sompi: f?.price },
    {
      id: 'deposit',
      name: 'Deposit',
      sompi: f?.deposit,
      note:
        f && clock ? (
          <span className="inline-flex items-baseline gap-1">
            <span>back</span>
            <Time
              daa={f.depositBack}
              virtualDaa={clock.virtualDaa}
              observedAt={clock.observedAt}
              testId="deposit-back"
            />
          </span>
        ) : undefined,
    },
    { id: 'fee', name: 'Network fee', sompi: f?.fee },
    {
      id: 'referral',
      name: 'Referral',
      sompi: f?.referral.amount,
      note: f ? (
        f.referral.paid && f.referral.address ? (
          <HashOrAddress value={f.referral.address} kind="address" explorer={explorer} testId="referral-to" />
        ) : (
          <span data-testid="referral-to" data-value="round">
            Stays in round
          </span>
        )
      ) : undefined,
    },
  ];
}

export function BuyCard({
  play,
  explorer,
  walletConnected,
}: {
  play: Play;
  explorer?: string;
  walletConnected: boolean;
}) {
  const ticketTitle = useId();
  const { sales, cost, tx } = play;
  if (sales.kind === 'loading') {
    return (
      <section data-testid="buy-card" data-state="loading" aria-busy="true" aria-label="Buy" className={card}>
        <span className="h-40 w-full animate-pulse rounded bg-rule-soft" />
        <span className="sr-only">Loading</span>
      </section>
    );
  }
  if (sales.kind === 'error') {
    return (
      <section data-testid="buy-card" data-state="error" aria-label="Buy" className={card}>
        <p role="alert" data-testid="buy-error" data-code={sales.code ?? ''} className="m-0 text-amber">
          {sales.message}
        </p>
      </section>
    );
  }
  const s = sales.data;
  if (!s.open) {
    return (
      <section data-testid="buy-card" data-state="closed" aria-label="Buy" className={card}>
        <p data-testid="sales-closed-play" role="status" className="m-0 font-mono text-[15px] text-amber">
          Sales closed; waiting for close()
        </p>
        <a
          href={href('rounds')}
          data-testid="sales-closed-rounds"
          className="flex min-h-11 items-center text-[13px] font-medium"
        >
          Rounds
        </a>
      </section>
    );
  }
  const prepared = cost.kind === 'ready' ? cost.prepared : undefined;
  const clock = { virtualDaa: s.virtualDaaScore, observedAt: s.observedAt };
  const state = cost.kind === 'ready' ? 'ready' : cost.kind === 'estimating' ? 'estimating' : 'empty';
  return (
    <div data-testid="buy-card" data-state="open" data-round={s.round} className="flex flex-col gap-3.5">
      <NumberPicker
        pick={play.pick}
        m={play.m}
        ticketFull={play.ticket.length >= play.bMax}
        disabled={play.busy}
        onMain={play.onMain}
        onBonus={play.onBonus}
        onQuickPick={play.onQuickPick}
        onClear={play.onClear}
        onAdd={play.onAdd}
      />
      <section aria-labelledby={ticketTitle} data-testid="ticket" className={card}>
        <div className="flex items-center justify-between">
          <h2 id={ticketTitle} className="m-0 text-[17px] font-semibold">
            Ticket
          </h2>
          <span data-testid="ticket-count" className="font-mono text-[13px] text-graphite tabular-nums">
            {play.ticket.length}/{play.bMax}
          </span>
        </div>
        {play.ticket.length > 0 && (
          <ul className="m-0 list-none p-0">
            {play.ticket.map((c, i) => (
              <TicketRow
                key={`${i}-${c.main.join('-')}-${c.bonus}`}
                index={i}
                main={c.main}
                bonus={c.bonus}
                onRemove={play.busy ? undefined : play.onRemove}
              />
            ))}
          </ul>
        )}
      </section>
      <CostPanel state={state} rows={rows(prepared?.figures, clock, explorer)} total={prepared?.figures.total} />
      {cost.kind === 'error' && (
        <p role="alert" data-testid="cost-error" data-code={cost.code ?? ''} className="m-0 text-amber">
          {cost.message}
        </p>
      )}
      {!walletConnected ? (
        <Button asChild variant="primary" size="lg" data-testid="buy-connect">
          <a href={href('wallet')} className="no-underline">
            Connect
          </a>
        </Button>
      ) : (
        <Button
          variant="primary"
          size="lg"
          data-testid="buy-button"
          data-shard={prepared?.shardIndex ?? ''}
          disabled={!prepared || play.busy}
          onClick={() => void play.onBuy()}
        >
          Buy
        </Button>
      )}
      {tx && (
        <section aria-label="Purchase" data-testid="buy-progress" className={card}>
          <TxProgress step={tx.step} failed={tx.failed} rebuilt={tx.rebuilt} />
          {tx.failedCode && <span data-testid="buy-failed-code" data-code={tx.failedCode} hidden />}
          {tx.id && <HashOrAddress value={tx.id} kind="transaction" explorer={explorer} testId="buy-txid" />}
        </section>
      )}
    </div>
  );
}
