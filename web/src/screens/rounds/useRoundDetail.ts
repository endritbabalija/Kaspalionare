// S3 round detail's state (spec 3 S3, S4; J4, J6; F5): the round read and checked every 10 s; Verify on a press
// (the worker's `verifyRound`); the caller panel: with a wallet connected, the due transition is built at once
// so its rows show the reward, the reserves, the fee and the total before anything is signed; a press signs
// exactly that build (one transition at a time, one wallet request at a time). A transition whose outcome the
// page does not learn (submit_unknown) keeps the press disabled until the chain shows the round moved on.
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
  estimating?: boolean;
  error?: string;
  tx?: CallerTx;
  /** The outcome of the last press is not known: no second press while the round still shows this transition due. */
  blockedKey?: string;
}

function dueKey(r: string, d: Due): string {
  return d.kind === 'due' ? `${r}:${d.action}` : `${r}:${d.kind}`;
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

  // Another transition due (e.g. the void after this page's close): the last one's progress no longer applies, and
  // it would hide the new transition's press. Kept until then (after a press the panel shows its outcome).
  const shownFor = useRef(key);
  useEffect(() => {
    if (shownFor.current === key || busy.current || due.kind !== 'due') return;
    shownFor.current = key;
    setCaller((c) => (c.tx ? { ...c, tx: undefined } : c));
  }, [key, due]);

  // Build the due transition once per (round, transition, wallet); again after a press.
  useEffect(() => {
    if (!data || due.kind !== 'due' || !adapter || !address || busy.current) return;
    if (builtFor.current === key) return;
    builtFor.current = key;
    const action = due.action;
    setCaller((c) => ({ ...c, estimating: true, error: undefined, prep: undefined }));
    void prepareCaller({ ctx, wallet: adapter, address }, data, action).then(
      (prep) => setCaller((c) => ({ ...c, prep, estimating: false })),
      (e: unknown) => {
        const d = describe(e);
        if (d.detail) console.warn(`round ${r} ${action}: ${d.code ?? 'error'}: ${d.detail}`);
        setCaller((c) => ({ ...c, estimating: false, error: d.message }));
      },
    );
  }, [data, due, key, adapter, address, ctx, r]);

  const press = useCallback(async () => {
    const p = caller.prep;
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
  }, [caller.prep, caller.blockedKey, key, adapter, address, ctx, r, reload, wallet]);

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
    caller,
    running,
    blocked: caller.blockedKey === key,
    verify,
    walletConnected: !!address && !!adapter,
    onPress: () => void press(),
    onVerify: () => void runVerify(),
  };
}
