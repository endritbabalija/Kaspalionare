// S2 My tickets (spec 3 S2, 4; mock-up board 3; J2, J3): one card per ticket of this instance in this
// browser (bought here, found by the connected wallet's payout, or imported from a backup), newest round
// first; the "Saved on this device" box with the backup download. Loading, empty, error and stale states.
import { useState } from 'react';
import type { ClaimFigures, ReturnFigures, RoundStatus } from '@wasm/web.js';
import { useSession } from '@/app/context';
import { href } from '@/app/routes';
import { FigureRow } from '@/components/FigureRow';
import { LocalCopyBox } from '@/components/LocalCopyBox';
import { PendingPurchase } from '@/components/PendingPurchase';
import { TicketCard, type ActionView } from '@/components/TicketCard';
import { saveText } from '@/lib/download';
import { describe, messageOf } from '@/lib/errors';
import { backupFileName, writeBackup } from '@/store/backup';
import { storeOf } from '@/store/session';
import type { CardData } from '@/tickets/load';
import { useWallet } from '@/wallet/context';
import { useTickets } from './tickets/useTickets';

/** The pill: the chain's stage of the ticket's round (void: the indexer's word that neither the chain nor the copy contradicts). */
function roundStatus(c: CardData): RoundStatus {
  if (c.void || c.card?.stage === 'void') return 'void';
  if (c.card?.stage === 'drawn' || c.stage === 'drawn') return 'drawn';
  if (c.stage === 'closed') return 'closing';
  return 'open';
}

export function MyTickets() {
  const ctx = useSession();
  const wallet = useWallet();
  const t = useTickets(ctx, wallet);
  const [saving, setSaving] = useState<string | null>(null);

  const download = async () => {
    setSaving('');
    try {
      const store = await storeOf(ctx);
      const text = await writeBackup(ctx, store);
      saveText(backupFileName(ctx.config.network, ctx.config.covenantId), text);
      setSaving(null);
    } catch (e) {
      setSaving(describe(e).message);
    }
  };

  if (t.load.kind === 'loading') {
    return (
      <section
        data-testid="tickets"
        data-state="loading"
        aria-busy="true"
        aria-label="My tickets"
        className="flex flex-col gap-3"
      >
        <h1 className="m-0 text-[17px] font-semibold">My tickets</h1>
        <span className="h-40 w-full animate-pulse rounded-[14px] bg-rule-soft" />
        <span className="sr-only">Loading</span>
      </section>
    );
  }
  if (t.load.kind === 'error') {
    return (
      <section data-testid="tickets" data-state="error" aria-label="My tickets" className="flex flex-col gap-3">
        <h1 className="m-0 text-[17px] font-semibold">My tickets</h1>
        <p role="alert" data-testid="tickets-error" data-code={t.load.code ?? ''} className="m-0 text-amber">
          {t.load.message}
        </p>
      </section>
    );
  }
  const d = t.load.data;
  const clock = { virtualDaa: d.virtualDaaScore, observedAt: d.observedAt };
  const live = d.cards.filter((c) => !c.stored.ended && !c.spent);
  const rounds = new Set(d.cards.filter((c) => c.copy !== 'missing').map((c) => c.stored.round)).size;
  return (
    <section
      data-testid="tickets"
      data-state={d.cards.length || d.stale.length ? 'ready' : 'empty'}
      data-api={d.apiDown ? 'down' : 'up'}
      aria-labelledby="tickets-title"
      className="flex flex-col gap-3.5"
    >
      <h1 id="tickets-title" className="m-0 text-[17px] font-semibold">
        My tickets
      </h1>
      {d.apiDown && (
        <p role="status" data-testid="tickets-api-down" className="m-0 font-mono text-[13px] text-amber">
          {messageOf('indexer_unavailable')}
        </p>
      )}
      {!d.cards.length && !d.stale.length && (
        <div
          data-testid="tickets-empty"
          className="flex flex-col gap-2 rounded-[14px] border border-rule bg-surface p-4"
        >
          <FigureRow name="Tickets" testId="tickets-empty-count">
            0
          </FigureRow>
          <a href={href('play')} data-testid="tickets-play" className="flex min-h-11 items-center font-medium">
            Play
          </a>
        </div>
      )}
      {d.stale.map((p, i) => (
        <PendingPurchase
          key={p.key}
          testId={`pending-${i}`}
          pending={p}
          state={t.resends[p.key]?.state ?? 'unknown'}
          error={t.resends[p.key]?.error}
          disabled={t.busy}
          onSend={() => t.onResend(p.key)}
          explorer={ctx.config.explorerUrl}
        />
      ))}
      {d.cards.map((c, i) => (
        <TicketCard
          key={c.stored.key}
          testId={`ticket-${i}`}
          stored={c.stored}
          status={roundStatus(c)}
          card={c.card}
          spent={c.spent}
          clock={clock}
          claims={Object.fromEntries(
            (c.card?.combos ?? []).map((x) => [
              x.index,
              (t.views[t.claimKey(c.stored.key, x.index)] ?? {}) as ActionView<ClaimFigures>,
            ]),
          )}
          ret={t.views[t.returnKey(c.stored.key)] as ActionView<ReturnFigures> | undefined}
          busy={t.busy || !t.walletConnected}
          onClaim={(index) => t.onClaim(c, index)}
          onReturn={() => t.onReturn(c)}
          explorer={ctx.config.explorerUrl}
          wallet={wallet.address}
        />
      ))}
      {d.cards.some((c) => c.error) && (
        <p role="alert" data-testid="tickets-card-error" className="m-0 text-amber">
          {d.cards.find((c) => c.error)?.error?.message}
        </p>
      )}
      {!t.walletConnected &&
        live.some((c) => c.card?.combos.some((x) => x.status === 'claimable') || c.card?.returnable) && (
          <a href={href('wallet')} data-testid="tickets-connect" className="flex min-h-11 items-center font-medium">
            Connect
          </a>
        )}
      <LocalCopyBox
        state={d.copyBox}
        rounds={rounds}
        tickets={live.length}
        onDownload={() => void download()}
        downloading={saving === ''}
      />
      {saving && (
        <p role="alert" data-testid="local-copy-error" className="m-0 text-amber">
          {saving}
        </p>
      )}
    </section>
  );
}
