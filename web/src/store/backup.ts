// S8 Backup (spec 3 S8, 6.3, 8 "Local copy"): the whole local store of this instance as one file, and the
// file read back with the same checks the store's records got when they were taken. The file is written and
// read in Rust (`backupWrite`, `backupRead`: size cap, strict format, this instance and network, a checksum,
// every record checked on its own; `backup_invalid` otherwise). Then, before anything is kept, every record
// against the chain: each ticket's state against the node's entry for its outpoint (`verifyState`), each
// closed or drawn round copy's lists against a live marker or pool found on a node by its script
// (`checkCopy`). Any disagreement refuses the whole file (`backup_mismatch`); a ticket no longer live, an open
// copy (its lists are taken again from the chain) and a round with nothing left on the chain are left out
// and reported (an ended ticket's history too: it has nothing left to do, and its end is not checkable). The quick wallet's key is never in the file (it has its own database and its own export).
import type { ApiObject, BackupContents, BackupRound, StoredRound, StoredTicket } from '@wasm/web.js';
import { PageError, unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { laterStage, type WebStore } from './webstore';

/** The file's text. */
export async function writeBackup(ctx: Ctx, store: WebStore, now: () => number = Date.now): Promise<string> {
  const [tickets, rounds] = await Promise.all([
    store.tickets(ctx.config.covenantId),
    store.rounds(ctx.config.covenantId),
  ]);
  return unwrap(
    await ctx.lottery.call('backupWrite', {
      covenantId: ctx.config.covenantId,
      createdMs: String(now()),
      tickets,
      rounds,
      pending: [],
    }),
  );
}

export function backupFileName(network: string, covenantId: string, now: Date = new Date()): string {
  const d = now.toISOString().slice(0, 10);
  return `kaspalionare-backup-${network}-${covenantId.slice(0, 8)}-${d}.json`;
}

/** What an import kept or left out, per round. */
export type RoundOutcome = 'kept' | 'open' | 'gone' | 'absent';

export interface ImportReport {
  summary: BackupRound[];
  rounds: Record<string, RoundOutcome>;
  ticketsKept: number;
  ticketsSpent: number;
}

/** The largest file the page reads (Rust caps the text again). */
export const BACKUP_FILE_MAX = 16 << 20;

function objectOf(t: StoredTicket): ApiObject {
  const u = t.ticket.utxo;
  return {
    role: 'ticket',
    transactionId: u.transactionId,
    index: u.index,
    amount: u.amount,
    blockDaaScore: u.blockDaaScore,
    scriptPublicKey: u.scriptPublicKey,
    state: t.ticket.state,
  };
}

function mismatch(detail: string): PageError {
  return new PageError('backup_mismatch', detail);
}

async function checkRound(ctx: Ctx, r: StoredRound): Promise<RoundOutcome> {
  if (r.stage === 'open') return 'open';
  const scripts = [r.against, ...(r.result?.pools ?? [])]
    .filter((o): o is NonNullable<typeof o> => !!o)
    .map((o) => o.scriptPublicKey)
    .filter((s, i, a) => a.indexOf(s) === i)
    .slice(0, 16);
  const live = unwrap(await ctx.lottery.call('nodeScriptEntries', { covenantId: ctx.config.covenantId, scripts }));
  const c = await ctx.lottery.call('checkCopy', { covenantId: ctx.config.covenantId, round: r, live });
  if ('error' in c) {
    if (c.error.code === 'copy_mismatch') throw mismatch(`round ${r.round}: ${c.error.detail}`);
    throw new PageError(c.error.code, c.error.detail);
  }
  return c.ok.status === 'verified' ? 'kept' : c.ok.status === 'gone' ? 'gone' : 'open';
}

/** Read `text`, check it against the chain, and keep what passed (nothing at all when anything disagrees). */
export async function importBackup(ctx: Ctx, store: WebStore, text: string): Promise<ImportReport> {
  const contents: BackupContents = unwrap(
    await ctx.lottery.call('backupRead', { covenantId: ctx.config.covenantId, text }),
  );
  // Every check first, nothing kept until all passed.
  const rounds: Record<string, RoundOutcome> = {};
  const keepRounds: StoredRound[] = [];
  for (const r of contents.rounds) {
    const o = await checkRound(ctx, r);
    rounds[r.round] = o;
    if (o === 'kept') keepRounds.push(r);
  }
  const keepTickets: StoredTicket[] = [];
  let spent = 0;
  const objs = contents.tickets.map(objectOf);
  const utxos = objs.length ? unwrap(await ctx.lottery.call('nodeEntries', objs)) : [];
  for (const t of contents.tickets) {
    const u = utxos.find(
      (x) =>
        x.transactionId.toLowerCase() === t.ticket.utxo.transactionId.toLowerCase() && x.index === t.ticket.utxo.index,
    );
    // A ticket that ended, or is no longer live, has nothing left to do; its history is not imported unchecked.
    if (!u || t.ended) {
      spent++;
      continue;
    }
    const v = await ctx.lottery.call('verifyState', ctx.config.covenantId, 'ticket', {
      utxo: u,
      state: t.ticket.state,
    });
    if ('error' in v) throw mismatch(`ticket ${t.key}: ${v.error.code}: ${v.error.detail}`);
    keepTickets.push({ ...t, ticket: { utxo: u, state: t.ticket.state } });
  }
  for (const s of contents.summary) if (!(s.round in rounds)) rounds[s.round] = 'absent';
  for (const r of keepRounds) {
    const prev = await store.round(ctx.config.covenantId, r.round);
    if (laterStage(prev, r)) await store.putRound(r);
  }
  for (const t of keepTickets) {
    const prev = await store.ticket(t.key);
    // A ticket this browser already follows keeps its own record.
    if (!prev) await store.putTicket(t);
  }
  return { summary: contents.summary, rounds, ticketsKept: keepTickets.length, ticketsSpent: spent };
}
