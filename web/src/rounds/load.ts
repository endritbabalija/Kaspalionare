// S3 Rounds and round detail (spec 3 S3, 6.3; F5). Nothing the indexer says is shown as checked: each round is
// read from the indexer, every object it names is read from a public node by outpoint (spent ones, which an
// indexer a little behind may still list, are left out), and `checkIndexerRound` checks them. The status is the
// one the live objects show (F2/F3 review S4), "void" only as far as the chain supports it and this browser's own
// copy does not show the round drawn (F4 review S1). Figures come from the round's facts (Rust: T, S, J, D, the
// claim deadline, the void lock, the roots, the pools, the caller reward), each with the objects it is read from;
// a figure no live object fixes is the indexer's, marked unverified.
import type {
  AnchorBlock,
  ApiAnchorProof,
  ApiObject,
  ApiRound,
  ApiRoundRow,
  CheckedObject,
  CheckedRound,
  NodeInfo,
  RoundStatus,
  Utxo,
  WinningOut,
} from '@wasm/web.js';
import { PageError, describe, unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { storeOf } from '@/store/session';

/** A figure and whether a node-checked object fixes it (false: the indexer's, unverified). */
export interface Shown {
  value: string;
  checked: boolean;
  /** The transaction it lives in (the explorer link). */
  source?: string;
}

/** The round's status as the page shows it: a checked status, or null (no live object shows one). */
export type ShownStatus = RoundStatus | null;

export interface RowData {
  round: string;
  status: ShownStatus;
  sales?: Shown;
  jackpot?: Shown;
  /** The row's check failed (its one message). */
  error?: string;
  code?: string | null;
}

export interface Clock {
  virtualDaa: string;
  observedAt: number;
}

function sameOutpoint(u: Utxo, o: ApiObject): boolean {
  return u.transactionId.toLowerCase() === o.transactionId.toLowerCase() && u.index === o.index;
}

/** The round with only the objects the node holds, and the node's entries for them. */
export async function liveRound(ctx: Ctx, round: ApiRound): Promise<{ round: ApiRound; utxos: Utxo[] }> {
  const utxos = round.objects.length ? unwrap(await ctx.lottery.call('nodeEntries', round.objects)) : [];
  return { round: { ...round, objects: round.objects.filter((o) => utxos.some((u) => sameOutpoint(u, o))) }, utxos };
}

/** Whether this browser's copy of round `r` holds its result (then a "void" is refused, F4 review S1). */
async function copyDrawn(ctx: Ctx, r: string): Promise<boolean> {
  try {
    const s = await storeOf(ctx);
    return (await s.round(ctx.config.covenantId, r))?.stage === 'drawn';
  } catch {
    return false;
  }
}

/** The status to show: the chain's, or void as far as the chain and this browser's copy support it. */
export async function shownStatus(ctx: Ctx, c: CheckedRound): Promise<ShownStatus> {
  if (c.statusChecked) return c.status;
  if (c.voidSupported && !(await copyDrawn(ctx, c.round))) return 'void';
  return null;
}

function fromFact(f: { value: string; sources: { transactionId: string }[] } | undefined): Shown | undefined {
  return f ? { value: f.value, checked: true, source: f.sources[0]?.transactionId } : undefined;
}

/** The rounds the indexer lists, newest first (its row figures, unchecked until `checkRow`). */
export async function roundRows(ctx: Ctx): Promise<ApiRoundRow[]> {
  const list = await ctx.indexer.rounds();
  if (list.covenantId.toLowerCase() !== ctx.config.covenantId) {
    throw new PageError('chain_mismatch', `the indexer serves instance ${list.covenantId}`);
  }
  return list.rounds;
}

/** One row checked against the node. */
export async function checkRow(ctx: Ctx, row: ApiRoundRow): Promise<RowData> {
  try {
    const { round, utxos } = await liveRound(ctx, await ctx.indexer.round(row.round));
    const c = unwrap(await ctx.lottery.call('checkIndexerRound', { covenantId: ctx.config.covenantId, round, utxos }));
    const status = await shownStatus(ctx, c);
    const unverified = (v: string | undefined) => (v === undefined ? undefined : { value: v, checked: false });
    return {
      round: row.round,
      status,
      sales: fromFact(c.facts.sales) ?? unverified(row.sales),
      jackpot: fromFact(c.facts.jackpot) ?? unverified(row.jackpot),
    };
  } catch (e) {
    const d = describe(e);
    if (d.detail) console.warn(`round ${row.round}: ${d.code ?? 'error'}: ${d.detail}`);
    return { round: row.round, status: null, error: d.message, code: d.code };
  }
}

/** The round detail (S3): the indexer's copy, its live objects checked, and the node's clock. */
export interface DetailData {
  api: ApiRound;
  /** The live objects and the facts they fix (`checkIndexerRound`). */
  checked: CheckedRound;
  status: ShownStatus;
  /** Closing (once X exists): the provisional numbers, bound to the anchor block a public node reports. */
  provisional?: WinningOut;
  /** The anchor proof the indexer serves (unverified; checked before it is used). */
  proof?: ApiAnchorProof;
  /** The indexer's row of the round (S and J of a round no live object fixes them for; unverified). */
  row?: ApiRoundRow;
  node: NodeInfo;
  clock: Clock;
}

export function objectsOf(c: CheckedRound, role: CheckedObject['role']): CheckedObject[] {
  return c.objects.filter((o) => o.role === role);
}

/** X's block as a public node reports it, or undefined when the node does not have it (never the indexer's). */
export async function anchorBlockOf(ctx: Ctx, proof: ApiAnchorProof): Promise<AnchorBlock | undefined> {
  const x = unwrap(await ctx.lottery.call('anchorProofX', proof.anchorProof));
  const b = await ctx.lottery.call('nodeAnchorBlock', x.anchorHash);
  if ('ok' in b) return b.ok;
  console.info(`anchor block ${x.anchorHash}: ${b.error.code}: ${b.error.detail}`);
  return undefined;
}

export async function loadDetail(ctx: Ctx, r: string, now: () => number = Date.now): Promise<DetailData> {
  const api = await ctx.indexer.round(r);
  const { round, utxos } = await liveRound(ctx, api);
  const covenantId = ctx.config.covenantId;
  let checked = unwrap(await ctx.lottery.call('checkIndexerRound', { covenantId, round, utxos }));
  let provisional: WinningOut | undefined;
  let proof: ApiAnchorProof | undefined;
  // Closing and past D: the provisional numbers (DECISIONS 2026-10-26: shown labelled, never as verified).
  if (checked.statusChecked && checked.status === 'closing') {
    proof = (await ctx.indexer.anchorProof(r).catch(() => null)) ?? undefined;
    const block = proof ? await anchorBlockOf(ctx, proof) : undefined;
    if (proof && block) {
      const c = await ctx.lottery.call('checkIndexerRound', {
        covenantId,
        round,
        utxos,
        anchorProof: proof,
        anchorBlock: block,
      });
      if ('ok' in c && c.ok.anchorBound) {
        checked = c.ok;
        provisional = c.ok.winning;
      } else if ('error' in c) {
        console.warn(`round ${r}: the anchor proof: ${c.error.code}: ${c.error.detail}`);
      }
    }
  }
  const row = (await ctx.indexer.rounds().catch(() => null))?.rounds.find((x) => x.round === api.round);
  const node = unwrap(await ctx.lottery.call('nodeInfo'));
  return {
    api,
    row,
    checked,
    status: await shownStatus(ctx, checked),
    provisional,
    proof,
    node,
    clock: { virtualDaa: node.virtualDaaScore, observedAt: now() },
  };
}
