// S2 My tickets' state (spec 3 S2, 7; J2, J3): the cards (tickets/load.ts), re-read every 15 s; with a wallet
// connected every claimable combination's claim and every valid deposit return is built at once, so its
// row shows the fee before anything is signed (spec 7: "the fee, from the estimator"); a press signs exactly
// that build. One claim or return at a time (one wallet request at a time, spec 6.2); after each the cards
// are read again (another index's mode can change, e.g. to FINAL).
import { useCallback, useEffect, useRef, useState } from 'react';
import type { ClaimFigures, ReturnFigures } from '@wasm/web.js';
import type { ActionView } from '@/components/TicketCard';
import { describe } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { storeOf } from '@/store/session';
import {
  prepareClaim,
  prepareReturn,
  sendClaim,
  sendReturn,
  type PreparedClaim,
  type PreparedReturn,
} from '@/tickets/claimFlow';
import { loadTickets, type CardData, type TicketsData } from '@/tickets/load';
import type { SendEvent } from '@/tickets/send';
import type { WalletSession } from '@/wallet/context';

export const TICKETS_REFRESH_MS = 15_000;

export type TicketsLoad =
  { kind: 'loading' } | { kind: 'error'; code: string | null; message: string } | { kind: 'ready'; data: TicketsData };

type Prepared = PreparedClaim | PreparedReturn;
type View = ActionView<ClaimFigures | ReturnFigures> & { prep?: Prepared };

function claimKey(ticket: string, index: number): string {
  return `${ticket}#${index}`;
}

function returnKey(ticket: string): string {
  return `${ticket}#return`;
}

export function useTickets(ctx: Ctx, wallet: WalletSession) {
  const [load, setLoad] = useState<TicketsLoad>({ kind: 'loading' });
  // Each row's view holds the very build its figures come from (`prep`), so a press signs what is shown.
  const [views, setViews] = useState<Record<string, View>>({});
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const gen = useRef(0);
  const address = wallet.address;
  const adapter = wallet.adapter;

  const reload = useCallback(async () => {
    if (busyRef.current) return;
    try {
      const store = await storeOf(ctx);
      const data = await loadTickets(ctx, store, address);
      setLoad({ kind: 'ready', data });
    } catch (e) {
      const d = describe(e);
      if (d.detail) console.warn(`tickets: ${d.code ?? 'error'}: ${d.detail}`);
      setLoad((l) => (l.kind === 'ready' ? l : { kind: 'error', code: d.code, message: d.message }));
    }
  }, [ctx, address]);

  useEffect(() => {
    void reload();
    const t = setInterval(() => void reload(), TICKETS_REFRESH_MS);
    return () => clearInterval(t);
  }, [reload]);

  const data = load.kind === 'ready' ? load.data : undefined;

  // Build every claim and return the cards allow, one after another (the worker runs one call at a time).
  useEffect(() => {
    if (!data || !adapter || !address || busyRef.current) return;
    const g = ++gen.current;
    const env = { ctx, wallet: adapter, address };
    void (async () => {
      const set = (k: string, v: View) => {
        if (g === gen.current) setViews((x) => ({ ...x, [k]: { ...x[k], ...v } }));
      };
      for (const c of data.cards) {
        if (!c.chain || !c.card) continue;
        for (const combo of c.card.combos) {
          if (combo.status !== 'claimable') continue;
          const k = claimKey(c.stored.key, combo.index);
          set(k, { estimating: true, error: undefined });
          try {
            const p = await prepareClaim(env, c.chain, combo.index);
            set(k, { figures: p.figures, prep: p, estimating: false });
          } catch (e) {
            set(k, { figures: undefined, prep: undefined, estimating: false, error: describe(e).message });
          }
        }
        if (c.card.returnable) {
          const k = returnKey(c.stored.key);
          set(k, { estimating: true, error: undefined });
          try {
            const p = await prepareReturn(env, c.chain);
            set(k, { figures: p.figures, prep: p, estimating: false });
          } catch (e) {
            set(k, { figures: undefined, prep: undefined, estimating: false, error: describe(e).message });
          }
        }
      }
    })();
  }, [data, adapter, address, ctx]);

  const act = useCallback(
    async (c: CardData, key: string) => {
      const p = views[key]?.prep;
      if (!p || !adapter || !address || !c.chain || busyRef.current) return;
      busyRef.current = true;
      setBusy(true);
      setViews((x) => ({ ...x, [key]: { ...x[key], tx: { step: 'built' }, error: undefined } }));
      const onEvent = (e: SendEvent) =>
        setViews((x) => {
          const tx = x[key]?.tx ?? { step: 'built' as const };
          return {
            ...x,
            [key]: { ...x[key], tx: e.kind === 'step' ? { ...tx, step: e.step } : { ...tx, checking: e.on } },
          };
        });
      try {
        const store = await storeOf(ctx);
        const env = { ctx, wallet: adapter, address };
        const id =
          p.kind === 'claim'
            ? await sendClaim(env, store, c.chain, p, onEvent)
            : await sendReturn(env, store, c.chain, p, onEvent);
        setViews((x) => ({ ...x, [key]: { ...x[key], tx: { step: 'accepted', id, checking: false } } }));
      } catch (e) {
        const d = describe(e);
        if (d.detail) console.warn(`${key}: ${d.code ?? 'error'}: ${d.detail}`);
        setViews((x) => ({
          ...x,
          [key]: { ...x[key], tx: { ...(x[key]?.tx ?? { step: 'built' }), checking: false, failed: d.message } },
        }));
        if (d.code === 'wallet_changed' && wallet.kind === 'quick') void wallet.openQuick();
      } finally {
        busyRef.current = false;
        setBusy(false);
        setViews((x) => ({ ...x, [key]: { ...x[key], prep: undefined, figures: undefined } }));
        void reload();
      }
    },
    [adapter, address, ctx, reload, wallet, views],
  );

  return {
    load,
    busy,
    views,
    claimKey,
    returnKey,
    reload,
    walletConnected: !!address && !!adapter,
    onClaim: (c: CardData, index: number) => void act(c, claimKey(c.stored.key, index)),
    onReturn: (c: CardData) => void act(c, returnKey(c.stored.key)),
  };
}
