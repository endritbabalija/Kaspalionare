// S1's buy card state (spec 3, 4, 7): the open round's live shards (re-read every 10 s), the pick, the ticket
// (1..B_MAX combinations), the cost panel (a real build of the purchase as soon as the ticket and a wallet are
// there: its rows are the built transaction's, `buyFigures`), and the purchase itself with its progress.
// Buy signs exactly the purchase the panel shows: a rebuild that changes it stops for a fresh press
// (F2/F3 review M2); while a sent purchase's outcome is unknown, no second purchase can start (S3).
import { useCallback, useEffect, useRef, useState } from 'react';
import type { Combination } from '@wasm/web.js';
import type { TxStep } from '@/components/TxProgress';
import type { DevHooks } from '@/dev/hooks';
import { describe, messageOf } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { buy, loadSales, prepareWithReferral, type Prepared, type Resume, type Sales } from '@/play/buyFlow';
import { EMPTY, combination, quickPick, toggleBonus, toggleMain, type Pick } from '@/play/picker';
import { referrerFor, validReferrer } from '@/referral/referral';
import { storeOf } from '@/store/session';
import type { WalletSession } from '@/wallet/context';

export const SALES_REFRESH_MS = 10_000;

type Failure = { message: string; code: string | null };

export type SalesLoad = { kind: 'loading' } | ({ kind: 'error' } & Failure) | { kind: 'ready'; data: Sales };
export type CostLoad =
  | { kind: 'empty' }
  | { kind: 'estimating' }
  | { kind: 'ready'; prepared: Prepared; resume?: Resume }
  | ({ kind: 'error' } & Failure);
export interface TxState {
  step: TxStep;
  failed?: string;
  failedCode?: string | null;
  rebuilt?: { tries: number; max: number };
  /** The page is asking the chain whether a sent purchase went through. */
  checking?: boolean;
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
  /** Bumped to build the cost panel again (after a failed purchase). */
  const [estimate, setEstimate] = useState(0);
  const busyRef = useRef(false);
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
    if (busyRef.current) return;
    if (!ticket.length || !data || !open || !address || !adapter) {
      setCost({ kind: 'empty' });
      return;
    }
    setCost({ kind: 'estimating' });
    const t = setTimeout(() => {
      void (async () => {
        try {
          const owner =
            ctx.config.referralAddress && (await validReferrer(ctx.lottery, ctx.config.referralAddress))
              ? ctx.config.referralAddress
              : undefined;
          const referrer = await referrerFor(ctx.lottery, ctx.config.referralAddress);
          const prepared = await prepareWithReferral(
            { ctx, wallet: adapter, address, dev },
            data,
            ticket,
            referrer,
            owner,
          );
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
  }, [ticket, round, open, address, adapter, ctx, dev, estimate]);

  const onBuy = useCallback(async () => {
    if (cost.kind !== 'ready' || !adapter || !address) return;
    busyRef.current = true;
    setBusy(true);
    setTx({ step: 'built' });
    let rebuildPanel = true;
    try {
      const store = await storeOf(ctx);
      const referrer = cost.prepared.input.referrer ?? undefined;
      const r = await buy(
        { ctx, wallet: adapter, address, store, dev },
        cost.prepared,
        referrer,
        (e) => {
          if (e.kind === 'step') setTx((t) => ({ ...(t ?? {}), step: e.step }));
          else if (e.kind === 'rebuilt')
            setTx((t) => ({ ...(t ?? { step: 'built' }), rebuilt: { tries: e.tries, max: e.max } }));
          else if (e.kind === 'checking') setTx((t) => ({ ...(t ?? { step: 'signed' }), checking: e.on }));
        },
        cost.resume,
      );
      if (r.kind === 'confirm') {
        // Nothing was signed: the new figures wait for the player's press.
        rebuildPanel = false;
        setCost({ kind: 'ready', prepared: r.prepared, resume: r.resume });
        setTx((t) => ({
          ...(t ?? { step: 'built' }),
          step: 'built',
          checking: false,
          failed: messageOf('purchase_changed'),
          failedCode: 'purchase_changed',
        }));
        return;
      }
      setTx((t) => ({ ...(t ?? {}), step: 'accepted', checking: false, id: r.id }));
      setTicket([]);
      setPick(EMPTY);
      setCost({ kind: 'empty' });
      void reloadSales();
    } catch (e) {
      const f = failure(e, 'buy');
      setTx((t) => ({ ...(t ?? { step: 'built' }), checking: false, failed: f.message, failedCode: f.code }));
      if (f.code === 'sales_closed') void reloadSales();
      if (f.code === 'wallet_changed' && wallet.kind === 'quick') void wallet.openQuick();
    } finally {
      busyRef.current = false;
      setBusy(false);
      if (rebuildPanel) setEstimate((x) => x + 1);
    }
  }, [cost, adapter, address, ctx, dev, reloadSales, wallet]);

  const m = data?.m ?? 0;
  const bMax = data?.bMax ?? 0;
  const shape = data?.shape;
  return {
    sales,
    pick,
    ticket,
    cost,
    tx,
    busy,
    m,
    bMax,
    shape,
    onMain: (n: number) => shape && setPick((p) => toggleMain(p, n, shape)),
    onBonus: (n: number) => setPick((p) => toggleBonus(p, n, m)),
    onQuickPick: () => shape && m > 0 && setPick(quickPick(m, shape)),
    onClear: () => setPick(EMPTY),
    onAdd: () => {
      const c = shape ? combination(pick, shape) : null;
      if (!c || ticket.length >= bMax) return;
      setTicket((t) => [...t, c]);
      setPick(EMPTY);
      setTx(null);
    },
    onRemove: (i: number) => setTicket((t) => t.filter((_, j) => j !== i)),
    onBuy,
  };
}
