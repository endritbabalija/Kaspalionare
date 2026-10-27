// S3 round detail's state (spec 3 S3, S4; J4, J6; F5): the round read and checked every 10 s; Verify on a press
// (the worker's `verifyRound`); the caller panel: with a wallet connected, the due transition is built at once
// so its rows show the reward, the reserves, the fee and the total before anything is signed; a press signs
// exactly that build (one transition at a time, one wallet request at a time). A transition whose outcome the
// page does not learn (submit_unknown) keeps the press disabled until the chain shows the round moved on. A build
// belongs to one (round, transition, wallet, live objects): when the due transition changes or the chain moves the
// round's objects, it is dropped at once and the due one rebuilt (F5 review S6), so the panel never shows, and a
// press never signs, another state's build.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { RoundVerify } from '@wasm/web.js';
import type { CallerTx } from '@/components/CallerPanel';
import type { VerifyState } from '@/components/VerifyPanel';
import { describe } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { dueOf, prepareCaller, sendCaller, type Due, type PreparedCaller } from '@/rounds/caller';
import { loadDetail, type DetailData } from '@/rounds/load';
import { verifyRound } from '@/rounds/verify';
import type { WalletSession } from '@/wallet/context';

export const DETAIL_REFRESH_MS = 10_000;

export type DetailLoad =
  { kind: 'loading' } | { kind: 'error'; code: string | null; message: string } | { kind: 'ready'; data: DetailData };

export interface CallerView {
  prep?: PreparedCaller;
  /** The build key `prep` was made for (only shown while it is the current one). */
  prepFor?: string;
  estimating?: boolean;
  error?: string;
  tx?: CallerTx;
  /** The outcome of the last press is not known: no second press while the round still shows this transition due. */
  blockedKey?: string;
}

function dueKey(r: string, d: Due): string {
  return d.kind === 'due' ? `${r}:${d.action}` : `${r}:${d.kind}`;
}

/** The round's live, checked objects (what a build spends): any change means the chain moved. */
function objectsKey(d: DetailData | undefined): string {
  if (!d) return '';
  return d.checked.objects
    .map((o) => `${o.object.utxo.transactionId.toLowerCase()}:${o.object.utxo.index}`)
    .sort()
    .join(',');
}

export function useRoundDetail(ctx: Ctx, wallet: WalletSession, r: string) {
  const [load, setLoad] = useState<DetailLoad>({ kind: 'loading' });
  const [verify, setVerify] = useState<{ state: VerifyState; result?: RoundVerify; error?: string }>({ state: 'idle' });
  const [caller, setCaller] = useState<CallerView>({});
  const busy = useRef(false);
  const [running, setRunning] = useState(false);
  const builtFor = useRef<string | null>(null);
  const address = wallet.address;
  const adapter = wallet.adapter;

  const reload = useCallback(async () => {
    try {
      const data = await loadDetail(ctx, r);
      setLoad({ kind: 'ready', data });
    } catch (e) {
      const d = describe(e);
      if (d.detail) console.warn(`round ${r}: ${d.code ?? 'error'}: ${d.detail}`);
      setLoad((l) => (l.kind === 'ready' ? l : { kind: 'error', code: d.code, message: d.message }));
    }
  }, [ctx, r]);

  useEffect(() => {
    setLoad({ kind: 'loading' });
    setVerify({ state: 'idle' });
    setCaller({});
    builtFor.current = null;
    void reload();
    const t = setInterval(() => {
      if (!busy.current) void reload();
    }, DETAIL_REFRESH_MS);
    return () => clearInterval(t);
  }, [reload]);

  const data = load.kind === 'ready' ? load.data : undefined;
  const due: Due = useMemo(() => (data ? dueOf(data) : { kind: 'none' }), [data]);
  const key = `${dueKey(r, due)}:${address ?? ''}`;
  const buildKey = `${key}|${objectsKey(data)}`;

  // Another transition due (e.g. the void after this page's close): the last one's progress no longer applies, and
  // it would hide the new transition's press. Kept until then (after a press the panel shows its outcome).
  const shownFor = useRef(key);
  useEffect(() => {
    if (shownFor.current === key || busy.current || due.kind !== 'due') return;
    shownFor.current = key;
    setCaller((c) => (c.tx ? { ...c, tx: undefined } : c));
  }, [key, due]);

  // Build the due transition once per (round, transition, wallet, live objects); again after a press. Another key:
  // the old build is dropped at once (its answer, if it comes later, too).
  useEffect(() => {
    if (busy.current || builtFor.current === buildKey) return;
    builtFor.current = buildKey;
    if (!data || due.kind !== 'due' || !adapter || !address) {
      setCaller((c) =>
        c.prep || c.estimating || c.error
          ? { ...c, prep: undefined, prepFor: undefined, estimating: false, error: undefined }
          : c,
      );
      return;
    }
    const action = due.action;
    const forKey = buildKey;
    setCaller((c) => ({ ...c, estimating: true, error: undefined, prep: undefined, prepFor: undefined }));
    void prepareCaller({ ctx, wallet: adapter, address }, data, action).then(
      (prep) =>
        setCaller((c) => (builtFor.current === forKey ? { ...c, prep, prepFor: forKey, estimating: false } : c)),
      (e: unknown) => {
        const d = describe(e);
        if (d.detail) console.warn(`round ${r} ${action}: ${d.code ?? 'error'}: ${d.detail}`);
        setCaller((c) => (builtFor.current === forKey ? { ...c, estimating: false, error: d.message } : c));
      },
    );
  }, [data, due, buildKey, adapter, address, ctx, r]);

  // Only the build of the current key is shown and signed.
  const shown = caller.prepFor === buildKey ? caller.prep : undefined;

  const press = useCallback(async () => {
    const p = shown;
    if (!p || !adapter || !address || busy.current || caller.blockedKey === key) return;
    busy.current = true;
    setRunning(true);
    setCaller((c) => ({ ...c, tx: { step: 'built' }, error: undefined }));
    try {
      const id = await sendCaller({ ctx, wallet: adapter, address }, p, (e) =>
        setCaller((c) => {
          const tx = c.tx ?? { step: 'built' as const };
          return { ...c, tx: e.kind === 'step' ? { ...tx, step: e.step } : { ...tx, checking: e.on } };
        }),
      );
      setCaller((c) => ({ ...c, prep: undefined, tx: { step: 'accepted', id, checking: false } }));
    } catch (e) {
      const d = describe(e);
      if (d.detail) console.warn(`round ${r} ${p.action}: ${d.code ?? 'error'}: ${d.detail}`);
      setCaller((c) => ({
        ...c,
        prep: undefined,
        blockedKey: d.code === 'submit_unknown' ? key : undefined,
        tx: { ...(c.tx ?? { step: 'built' }), checking: false, failed: d.message },
      }));
      if (d.code === 'wallet_changed' && wallet.kind === 'quick') void wallet.openQuick();
    } finally {
      busy.current = false;
      setRunning(false);
      builtFor.current = null;
      void reload();
    }
  }, [shown, caller.blockedKey, key, adapter, address, ctx, r, reload, wallet]);

  const runVerify = useCallback(async () => {
    setVerify({ state: 'running' });
    try {
      setVerify({ state: 'done', result: await verifyRound(ctx, r) });
    } catch (e) {
      const d = describe(e);
      if (d.detail) console.warn(`verify round ${r}: ${d.code ?? 'error'}: ${d.detail}`);
      setVerify({ state: 'error', error: d.message });
    }
  }, [ctx, r]);

  return {
    load,
    due,
    caller: { ...caller, prep: shown },
    running,
    blocked: caller.blockedKey === key,
    verify,
    walletConnected: !!address && !!adapter,
    onPress: () => void press(),
    onVerify: () => void runVerify(),
  };
}
