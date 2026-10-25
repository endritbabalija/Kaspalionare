// S1's buy card state (spec 3, 4, 7): the open round's live shards (re-read every 10 s), the pick, the ticket
// (1..B_MAX combinations), the cost panel (a real build of the purchase as soon as the ticket and a wallet are
// there: its rows are the built transaction's, `buyFigures`), and the purchase itself with its progress.
import { useCallback, useEffect, useRef, useState } from 'react';
import type { Combination } from '@wasm/web.js';
import type { TxStep } from '@/components/TxProgress';
import type { DevHooks } from '@/dev/hooks';
import { describe } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { buy, loadSales, prepareBuy, type Prepared, type Sales } from '@/play/buyFlow';
import { EMPTY, combination, quickPick, toggleBonus, toggleMain, type Pick } from '@/play/picker';
import { referrerFor } from '@/referral/referral';
import { storeOf } from '@/store/session';
import type { WalletSession } from '@/wallet/context';

export const SALES_REFRESH_MS = 10_000;

type Failure = { message: string; code: string | null };

export type SalesLoad = { kind: 'loading' } | ({ kind: 'error' } & Failure) | { kind: 'ready'; data: Sales };
export type CostLoad =
  { kind: 'empty' } | { kind: 'estimating' } | { kind: 'ready'; prepared: Prepared } | ({ kind: 'error' } & Failure);
export interface TxState {
  step: TxStep;
  failed?: string;
  failedCode?: string | null;
  rebuilt?: { tries: number; max: number };
  id?: string;
}

function failure(e: unknown, what: string): Failure {
  const d = describe(e);
  if (d.detail) console.warn(`${what}: ${d.code ?? 'error'}: ${d.detail}`);
  return { message: d.message, code: d.code };
}

export function usePlay(ctx: Ctx, wallet: WalletSession, dev?: DevHooks) {
  const [sales, setSales] = useState<SalesLoad>({ kind: 'loading' });
  const [pick, setPick] = useState<Pick>(EMPTY);
  const [ticket, setTicket] = useState<Combination[]>([]);
  const [cost, setCost] = useState<CostLoad>({ kind: 'empty' });
  const [tx, setTx] = useState<TxState | null>(null);
  const [busy, setBusy] = useState(false);
  const gen = useRef(0);

  const reloadSales = useCallback(async () => {
    try {
      setSales({ kind: 'ready', data: await loadSales(ctx) });
    } catch (e) {
      setSales((s) => (s.kind === 'ready' ? s : { kind: 'error', ...failure(e, 'sales') }));
    }
  }, [ctx]);

  useEffect(() => {
    void reloadSales();
    const t = setInterval(() => void reloadSales(), SALES_REFRESH_MS);
    return () => clearInterval(t);
  }, [reloadSales]);

  const data = sales.kind === 'ready' ? sales.data : undefined;
  const round = data?.round;
  const open = data?.open ?? false;
  const address = wallet.address;
  const adapter = wallet.adapter;

  // A build of the purchase for the cost panel: the ticket, the wallet and the round fix it.
  useEffect(() => {
    const g = ++gen.current;
    if (busy) return;
    if (!ticket.length || !data || !open || !address || !adapter) {
      setCost({ kind: 'empty' });
      return;
    }
    setCost({ kind: 'estimating' });
    const t = setTimeout(() => {
      void (async () => {
        try {
          const referrer = await referrerFor(ctx.lottery, ctx.config.referralAddress);
          const prepared = await prepareBuy({ ctx, wallet: adapter, address, dev }, data, ticket, referrer);
          if (g === gen.current) setCost({ kind: 'ready', prepared });
        } catch (e) {
          if (g === gen.current) setCost({ kind: 'error', ...failure(e, 'cost') });
        }
      })();
    }, 250);
    return () => clearTimeout(t);
    // `data` changes on every refresh; the build is redone when the round, its openness, the ticket or the
    // wallet change (a stale shard is caught at send time: a conflict, rebuilt).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ticket, round, open, address, adapter, ctx, dev, busy]);

  const onBuy = useCallback(async () => {
    if (cost.kind !== 'ready' || !adapter || !address) return;
    setBusy(true);
    setTx({ step: 'built' });
    try {
      const store = await storeOf(ctx);
      const referrer = cost.prepared.input.referrer ?? undefined;
      const r = await buy({ ctx, wallet: adapter, address, store, dev }, cost.prepared, referrer, (e) => {
        if (e.kind === 'step') setTx((t) => ({ ...(t ?? {}), step: e.step }));
        else if (e.kind === 'rebuilt')
          setTx((t) => ({ ...(t ?? { step: 'built' }), rebuilt: { tries: e.tries, max: e.max } }));
        else setCost({ kind: 'ready', prepared: e.prepared });
      });
      setTx((t) => ({ ...(t ?? {}), step: 'accepted', id: r.id }));
      setTicket([]);
      setPick(EMPTY);
      setCost({ kind: 'empty' });
      void reloadSales();
    } catch (e) {
      const f = failure(e, 'buy');
      setTx((t) => ({ ...(t ?? { step: 'built' }), failed: f.message, failedCode: f.code }));
    } finally {
      setBusy(false);
    }
  }, [cost, adapter, address, ctx, dev, reloadSales]);

  const m = data?.m ?? 0;
  const bMax = data?.bMax ?? 0;
  return {
    sales,
    pick,
    ticket,
    cost,
    tx,
    busy,
    m,
    bMax,
    onMain: (n: number) => setPick((p) => toggleMain(p, n)),
    onBonus: (n: number) => setPick((p) => toggleBonus(p, n, m)),
    onQuickPick: () => m > 0 && setPick(quickPick(m)),
    onClear: () => setPick(EMPTY),
    onAdd: () => {
      const c = combination(pick);
      if (!c || ticket.length >= bMax) return;
      setTicket((t) => [...t, c]);
      setPick(EMPTY);
      setTx(null);
    },
    onRemove: (i: number) => setTicket((t) => t.filter((_, j) => j !== i)),
    onBuy,
  };
}
