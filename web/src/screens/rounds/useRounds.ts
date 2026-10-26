// S3 Rounds list's state (spec 3 S3; F5): the indexer's rounds, newest first, each row then checked against the
// node one after another (`checkRow`), the newest first; re-read every 30 s. ROWS_SHOWN rows at a time; More shows
// the next.
import { useCallback, useEffect, useRef, useState } from 'react';
import type { ApiRoundRow } from '@wasm/web.js';
import { describe } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { checkRow, roundRows, type RowData } from '@/rounds/load';

export const ROUNDS_REFRESH_MS = 30_000;
export const ROWS_SHOWN = 10;

export type RoundsLoad =
  | { kind: 'loading' }
  | { kind: 'error'; code: string | null; message: string }
  | { kind: 'ready'; rows: ApiRoundRow[] };

export function useRounds(ctx: Ctx) {
  const [load, setLoad] = useState<RoundsLoad>({ kind: 'loading' });
  const [checked, setChecked] = useState<Record<string, RowData>>({});
  const [shown, setShown] = useState(ROWS_SHOWN);
  const gen = useRef(0);

  const reload = useCallback(async () => {
    const g = ++gen.current;
    let rows: ApiRoundRow[];
    try {
      rows = await roundRows(ctx);
      setLoad({ kind: 'ready', rows });
    } catch (e) {
      const d = describe(e);
      if (d.detail) console.warn(`rounds: ${d.code ?? 'error'}: ${d.detail}`);
      setLoad((l) => (l.kind === 'ready' ? l : { kind: 'error', code: d.code, message: d.message }));
      return;
    }
    for (const row of rows.slice(0, shown)) {
      if (g !== gen.current) return;
      const data = await checkRow(ctx, row);
      if (g === gen.current) setChecked((c) => ({ ...c, [row.round]: data }));
    }
  }, [ctx, shown]);

  useEffect(() => {
    const g = gen;
    void reload();
    const t = setInterval(() => void reload(), ROUNDS_REFRESH_MS);
    return () => {
      g.current++;
      clearInterval(t);
    };
  }, [reload]);

  return { load, checked, shown, more: () => setShown((n) => n + ROWS_SHOWN) };
}
