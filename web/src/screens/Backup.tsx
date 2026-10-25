// S8 Backup (spec 3 S8, 6.3): download the local store as one file, import one back (checked in Rust and
// against the chain before anything is kept: store/backup.ts), and see per round what the store (or the
// imported file) holds. The quick wallet's key is never in the file (it has its own export on the wallet
// sheet, S7).
import { useCallback, useEffect, useId, useState, type ChangeEvent } from 'react';
import type { BackupRound } from '@wasm/web.js';
import { useSession } from '@/app/context';
import { FigureRow } from '@/components/FigureRow';
import { Button } from '@/components/ui/button';
import { saveText } from '@/lib/download';
import { describe, PageError, unwrap } from '@/lib/errors';
import {
  BACKUP_FILE_MAX,
  backupFileName,
  importBackup,
  writeBackup,
  type ImportReport,
  type RoundOutcome,
} from '@/store/backup';
import { storeOf } from '@/store/session';

const stageName = { open: 'Open', closed: 'Closed', drawn: 'Drawn' } as const;
const outcomeName: Record<RoundOutcome, string> = {
  kept: 'Kept',
  open: 'Not kept',
  gone: 'Gone',
  absent: 'No copy',
};

function RoundRows({
  rows,
  outcomes,
  testId,
}: {
  rows: BackupRound[];
  outcomes?: Record<string, RoundOutcome>;
  testId: string;
}) {
  return (
    <ul data-testid={testId} className="m-0 flex list-none flex-col gap-2 p-0">
      {rows.map((r) => (
        <li
          key={r.round}
          data-testid={`${testId}-round-${r.round}`}
          className="flex flex-col border-t border-rule-soft pt-2"
        >
          <span className="text-[15px] font-semibold">Round {r.round}</span>
          <FigureRow name="Copy" testId={`${testId}-${r.round}-stage`}>
            {r.stage ? stageName[r.stage] : 'None'}
          </FigureRow>
          <FigureRow name="Tickets" testId={`${testId}-${r.round}-tickets`}>
            {r.tickets}
          </FigureRow>
          <FigureRow name="Combinations" testId={`${testId}-${r.round}-combinations`}>
            {r.combinations}
          </FigureRow>
          <FigureRow name="Purchases" testId={`${testId}-${r.round}-purchases`}>
            {r.purchases}
          </FigureRow>
          {r.winning && (
            <FigureRow name="Winning" testId={`${testId}-${r.round}-winning`}>
              {`${r.winning.main.join(' ')} + ${r.winning.bonus}`}
            </FigureRow>
          )}
          {outcomes?.[r.round] && (
            <FigureRow name="Import" testId={`${testId}-${r.round}-outcome`}>
              <span data-testid={`${testId}-${r.round}-outcome-value`} data-value={outcomes[r.round]}>
                {outcomeName[outcomes[r.round] as RoundOutcome]}
              </span>
            </FigureRow>
          )}
        </li>
      ))}
    </ul>
  );
}

type Status =
  | { kind: 'idle' }
  | { kind: 'busy' }
  | { kind: 'error'; code: string | null; message: string }
  | { kind: 'imported'; report: ImportReport };

export function Backup() {
  const ctx = useSession();
  const fileId = useId();
  const [rows, setRows] = useState<BackupRound[] | null>(null);
  const [status, setStatus] = useState<Status>({ kind: 'idle' });

  const summarise = useCallback(async () => {
    try {
      const store = await storeOf(ctx);
      const text = await writeBackup(ctx, store);
      const c = unwrap(await ctx.lottery.call('backupRead', { covenantId: ctx.config.covenantId, text }));
      setRows(c.summary);
    } catch (e) {
      const d = describe(e);
      setStatus({ kind: 'error', code: d.code, message: d.message });
    }
  }, [ctx]);

  useEffect(() => void summarise(), [summarise]);

  const download = async () => {
    try {
      const store = await storeOf(ctx);
      saveText(backupFileName(ctx.config.network, ctx.config.covenantId), await writeBackup(ctx, store));
    } catch (e) {
      const d = describe(e);
      setStatus({ kind: 'error', code: d.code, message: d.message });
    }
  };

  const onFile = async (ev: ChangeEvent<HTMLInputElement>) => {
    const f = ev.target.files?.[0];
    ev.target.value = '';
    if (!f) return;
    setStatus({ kind: 'busy' });
    try {
      if (f.size > BACKUP_FILE_MAX) throw new PageError('backup_invalid', `${f.size} bytes`);
      const store = await storeOf(ctx);
      const report = await importBackup(ctx, store, await f.text());
      setStatus({ kind: 'imported', report });
      void summarise();
    } catch (e) {
      const d = describe(e);
      if (d.detail) console.warn(`backup import: ${d.code ?? 'error'}: ${d.detail}`);
      setStatus({ kind: 'error', code: d.code, message: d.message });
    }
  };

  return (
    <section data-testid="backup" aria-labelledby="backup-title" className="flex flex-col gap-3.5">
      <h1 id="backup-title" className="m-0 text-[17px] font-semibold">
        Backup
      </h1>
      <div className="flex flex-col gap-2 rounded-[14px] border border-rule bg-surface p-4">
        <Button data-testid="backup-download" onClick={() => void download()}>
          Download backup
        </Button>
        <label
          htmlFor={fileId}
          data-testid="backup-import"
          className="relative inline-flex min-h-11 cursor-pointer items-center justify-center rounded-[10px] border border-control-line bg-surface px-4 text-[15px] font-medium text-ink focus-within:outline-2 focus-within:outline-focus"
        >
          Import
          <input
            id={fileId}
            type="file"
            accept="application/json,.json"
            data-testid="backup-file"
            aria-label="Import backup file"
            className="absolute -inset-px cursor-pointer opacity-0"
            disabled={status.kind === 'busy'}
            onChange={(e) => void onFile(e)}
          />
        </label>
        <div
          role="status"
          aria-live="polite"
          data-testid="backup-status"
          data-state={status.kind}
          className="text-[13px]"
        >
          {status.kind === 'busy' && <span className="font-mono text-graphite">Checking</span>}
          {status.kind === 'imported' && (
            <FigureRow name="Tickets kept" testId="backup-imported-row">
              <span data-testid="backup-imported" data-value={status.report.ticketsKept}>
                {status.report.ticketsKept}
              </span>
            </FigureRow>
          )}
        </div>
        {status.kind === 'error' && (
          <p role="alert" data-testid="backup-error" data-code={status.code ?? ''} className="m-0 text-amber">
            {status.message}
          </p>
        )}
      </div>
      {status.kind === 'imported' && (
        <section
          aria-label="Imported file"
          className="flex flex-col gap-1 rounded-[14px] border border-rule bg-surface p-4"
        >
          <h2 className="m-0 text-[15px] font-semibold">Imported file</h2>
          <RoundRows rows={status.report.summary} outcomes={status.report.rounds} testId="backup-file-rounds" />
        </section>
      )}
      <section
        aria-label="On this device"
        className="flex flex-col gap-1 rounded-[14px] border border-rule bg-surface p-4"
      >
        <h2 className="m-0 text-[15px] font-semibold">On this device</h2>
        {rows === null ? (
          <span className="h-10 w-full animate-pulse rounded bg-rule-soft" aria-busy="true" />
        ) : rows.length ? (
          <RoundRows rows={rows} testId="backup-rounds" />
        ) : (
          <FigureRow name="Rounds" testId="backup-empty">
            0
          </FigureRow>
        )}
      </section>
    </section>
  );
}
