import { useCallback, useEffect, useRef, useState } from 'react';
import { describe } from '@/lib/errors';
import { loadRoundCard, type Ctx, type RoundCardData } from '@/net/round';

/** How often the card re-reads the round, and when its last good read counts as stale. */
export const REFRESH_MS = 10_000;
export const STALE_MS = 30_000;

export type RoundCardLoad =
  | { kind: 'loading' }
  | { kind: 'error'; message: string; code: string | null; detail: string }
  | { kind: 'ready'; data: RoundCardData; stale: boolean; error?: string };

export function useRoundCard(ctx: Ctx): { load: RoundCardLoad; retry: () => void } {
  const [load, setLoad] = useState<RoundCardLoad>({ kind: 'loading' });
  const last = useRef<RoundCardData | null>(null);
  const [tick, setTick] = useState(0);

  const run = useCallback(async () => {
    try {
      const data = await loadRoundCard(ctx);
      last.current = data;
      setLoad({ kind: 'ready', data, stale: false });
    } catch (e) {
      const d = describe(e);
      if (d.detail) console.warn(`round card: ${d.code ?? 'error'}: ${d.detail}`);
      const prev = last.current;
      if (prev)
        setLoad({ kind: 'ready', data: prev, stale: Date.now() - prev.observedAt > STALE_MS, error: d.message });
      else setLoad({ kind: 'error', message: d.message, code: d.code, detail: d.detail });
    }
  }, [ctx]);

  useEffect(() => {
    let alive = true;
    const go = () => alive && void run();
    go();
    const t = setInterval(go, REFRESH_MS);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, [run, tick]);

  const retry = useCallback(() => {
    // A worker that stopped too often is tried again on this press (no reload needed).
    ctx.lottery.recover();
    setLoad({ kind: 'loading' });
    setTick((x) => x + 1);
  }, [ctx]);
  return { load, retry };
}
