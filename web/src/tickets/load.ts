// S2 My tickets' data (spec 3 S2, 6.3; J2, J3): the store brought up to date, then every ticket as a card.
//
// 1. Purchases whose outcome the page did not learn (F2/F3 review S3, the `pending` store): the ticket output
//    live on the node makes it a ticket; otherwise the same signed transaction is sent again, and an input
//    spent without our ticket drops it.
// 2. J3: with a wallet connected and the indexer API up, the live tickets paying out to its address are
//    found (`/v1/tickets?payout=`), each read from the node by outpoint and checked (`checkIndexerObjects`).
// 3. Each round the store holds tickets in is copied again when the chain shows a later stage (`copyRound`:
//    the trees from the API checked against the frozen roots; nothing unchecked is kept).
// 4. Each round copy is checked again against the chain without the API (`checkCopy`: the lists against the
//    live marker or a live pool, found on a node by script), which also gives the round's live pools.
// 5. Each ticket's entry from the node by outpoint, and its card from the wasm (`ticketCard`).
// With the API down, steps 2 and 3 are skipped: the claim needs only the local copy and a node.
import type { ApiObject, CovUtxo, RoundCopyStage, StoredRound, StoredTicket, TicketCard } from '@wasm/web.js';
import { describe, messageOf, PageError, unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { copyRound } from '@/store/roundCopy';
import { ticketKey, type WebStore } from '@/store/webstore';
import { settleSubmit } from '@/play/buyFlow';
import type { TicketChain } from './claimFlow';

export type CopyState = 'verified' | 'open' | 'gone' | 'missing' | 'mismatch';

export interface CardData {
  stored: StoredTicket;
  /** The ticket as it can act now (live on the node, with its round's chain objects). */
  chain?: TicketChain;
  /** The card's figures (absent when the ticket is not live, or its card could not be built). */
  card?: TicketCard;
  /** The ticket's entry is not on the node (claimed or returned elsewhere, or not confirmed yet). */
  spent: boolean;
  copy: CopyState;
  /** The round copy's stage, when the store holds one. */
  stage?: RoundCopyStage;
  /** The round's status says void (the indexer's word: no live object shows a void; display only). */
  void: boolean;
  error?: { code: string | null; message: string };
}

/** The local-copy box (spec 4): complete and verified, incomplete, mismatch, not needed. */
export type CopyBox = 'complete' | 'incomplete' | 'mismatch' | 'not-needed';

export interface TicketsData {
  cards: CardData[];
  copyBox: CopyBox;
  /** The indexer API could not be reached: the page works from the local copy. */
  apiDown: boolean;
  virtualDaaScore: string;
  observedAt: number;
}

function apiUnavailable(e: unknown): boolean {
  return e instanceof PageError && e.code === 'indexer_unavailable';
}

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

/** Step 1: settle the purchases whose outcome is not known. */
export async function settlePending(ctx: Ctx, store: WebStore): Promise<void> {
  for (const p of await store.pending(ctx.config.covenantId)) {
    const obj: ApiObject = {
      role: 'ticket',
      transactionId: p.key,
      index: p.ticket.index,
      amount: p.buy.deposit,
      blockDaaScore: '0',
      scriptPublicKey: p.ticket.scriptPublicKey,
      state: p.ticket.state,
    };
    const live = async () => {
      const u = unwrap(await ctx.lottery.call('nodeEntries', [obj])).find((x) => x.index === p.ticket.index);
      if (u)
        unwrap(
          await ctx.lottery.call('verifyState', ctx.config.covenantId, 'ticket', { utxo: u, state: p.ticket.state }),
        );
      return u;
    };
    try {
      let u = await live();
      if (!u) {
        await settleSubmit(p.signed, async (s) => unwrap(await ctx.lottery.call('nodeSubmit', s)), {
          landed: async () => (await live()) !== undefined,
          tries: 1,
        });
        u = await live();
      }
      if (u) {
        await store.putTicket({
          key: ticketKey(p.key, p.ticket.index),
          covenantId: p.covenantId,
          network: p.network,
          round: p.round,
          shardIndex: p.shardIndex,
          ticket: { utxo: u, state: p.ticket.state },
          bundle: p.bundle,
          payout: p.payout,
          buy: p.buy,
          savedMs: String(Date.now()),
        });
        await store.delPending(p.key);
      }
    } catch (e) {
      // An input spent and our ticket not live: it did not go through.
      if (e instanceof PageError && e.code === 'node_conflict') await store.delPending(p.key);
      else console.info(`pending purchase ${p.key}: ${describe(e).code ?? 'error'}: ${describe(e).detail}`);
    }
  }
}

function sameOutpoint(a: { transactionId: string; index: number }, b: { transactionId: string; index: number }) {
  return a.transactionId.toLowerCase() === b.transactionId.toLowerCase() && a.index === b.index;
}

/** Step 2 (J3): the live tickets paying out to `address`, found through the indexer and checked on the node. */
export async function findByPayout(ctx: Ctx, store: WebStore, address: string): Promise<number> {
  const api = await ctx.indexer.tickets(address);
  if (!api.tickets.length) return 0;
  const utxos = unwrap(await ctx.lottery.call('nodeEntries', api.tickets));
  const live = api.tickets.filter((o) => utxos.some((u) => sameOutpoint(u, o)));
  if (!live.length) return 0;
  const checked = unwrap(
    await ctx.lottery.call('checkIndexerObjects', { covenantId: ctx.config.covenantId, objects: live, utxos }),
  );
  const have = await store.tickets(ctx.config.covenantId);
  const node = unwrap(await ctx.lottery.call('nodeInfo'));
  let added = 0;
  for (const c of checked) {
    if (c.view.role !== 'ticket') continue;
    if (have.some((t) => sameOutpoint(t.ticket.utxo, c.object.utxo))) continue;
    // The bundle and payout as the wasm reads them from the state (no TypeScript decoding).
    const card = unwrap(
      await ctx.lottery.call('ticketCard', {
        covenantId: ctx.config.covenantId,
        ticket: c.object,
        virtualDaaScore: node.virtualDaaScore,
      }),
    );
    const bought = api.bought.find((b) =>
      sameOutpoint({ transactionId: b.transactionId, index: b.ticketIndex }, c.object.utxo),
    );
    const key = ticketKey(c.object.utxo.transactionId, c.object.utxo.index);
    if (have.some((t) => t.key === key)) continue;
    await store.putTicket({
      key,
      covenantId: ctx.config.covenantId,
      network: ctx.config.network,
      round: c.view.round,
      shardIndex: bought?.shardIndex,
      ticket: c.object,
      bundle: card.combos.map((x) => x.combination),
      payout: card.payout,
      savedMs: String(Date.now()),
    });
    added++;
  }
  return added;
}

const STAGE = { open: 0, closed: 1, drawn: 2 } as const;

/** Step 3: copy each round again when the chain shows a later stage; reports void rounds (display only). */
async function syncRounds(
  ctx: Ctx,
  store: WebStore,
  rounds: string[],
): Promise<{ voids: Set<string>; apiDown: boolean }> {
  const voids = new Set<string>();
  for (const r of rounds) {
    const have = await store.round(ctx.config.covenantId, r);
    if (have?.stage === 'drawn') continue;
    try {
      const api = await ctx.indexer.round(r);
      if (api.status === 'void') {
        voids.add(r);
        continue;
      }
      const want = api.status === 'open' ? 0 : api.status === 'closing' ? 1 : 2;
      if (have && STAGE[have.stage] >= want) continue;
      await copyRound(ctx, store, r);
    } catch (e) {
      if (apiUnavailable(e)) return { voids, apiDown: true };
      console.info(`round copy ${r}: ${describe(e).code ?? 'error'}: ${describe(e).detail}`);
    }
  }
  return { voids, apiDown: false };
}

interface RoundChain {
  copy: CopyState;
  pools: CovUtxo[];
  round?: StoredRound;
}

/** Step 4: a round copy against the chain, without the API. */
export async function checkRoundCopy(ctx: Ctx, round: StoredRound | undefined): Promise<RoundChain> {
  if (!round) return { copy: 'missing', pools: [] };
  if (round.stage === 'open') return { copy: 'open', pools: [], round };
  const scripts = [round.against, ...(round.result?.pools ?? [])]
    .filter((o): o is NonNullable<typeof o> => !!o)
    .map((o) => o.scriptPublicKey)
    .filter((s, i, a) => a.indexOf(s) === i)
    .slice(0, 16);
  const live = unwrap(await ctx.lottery.call('nodeScriptEntries', { covenantId: ctx.config.covenantId, scripts }));
  const c = await ctx.lottery.call('checkCopy', { covenantId: ctx.config.covenantId, round, live });
  if ('error' in c) {
    if (c.error.code === 'copy_mismatch') return { copy: 'mismatch', pools: [], round };
    throw new PageError(c.error.code, c.error.detail);
  }
  return { copy: c.ok.status, pools: c.ok.pools, round };
}

/** The local-copy box's state from every live ticket's round copy. */
export function copyBoxOf(cards: CardData[]): CopyBox {
  const active = cards.filter((c) => !c.stored.ended && !c.spent);
  if (!active.length) return 'not-needed';
  if (active.some((c) => c.copy === 'mismatch')) return 'mismatch';
  if (active.every((c) => c.copy === 'verified' || c.copy === 'gone' || c.void)) return 'complete';
  return 'incomplete';
}

/** Steps 1-5. */
export async function loadTickets(
  ctx: Ctx,
  store: WebStore,
  address?: string,
  opts: { now?: () => number; online?: boolean } = {},
): Promise<TicketsData> {
  const now = opts.now ?? Date.now;
  const node = unwrap(await ctx.lottery.call('nodeInfo'));
  const observedAt = now();
  await settlePending(ctx, store);
  let apiDown = opts.online === false;
  if (address && !apiDown) {
    try {
      await findByPayout(ctx, store, address);
    } catch (e) {
      if (apiUnavailable(e)) apiDown = true;
      else console.info(`tickets by payout: ${describe(e).code ?? 'error'}: ${describe(e).detail}`);
    }
  }
  const tickets = (await store.tickets(ctx.config.covenantId)).sort((a, b) =>
    BigInt(b.round) > BigInt(a.round) ? 1 : BigInt(b.round) < BigInt(a.round) ? -1 : a.key.localeCompare(b.key),
  );
  const rounds = [...new Set(tickets.filter((t) => !t.ended).map((t) => t.round))];
  let voids = new Set<string>();
  if (!apiDown) {
    const s = await syncRounds(ctx, store, rounds);
    voids = s.voids;
    apiDown = s.apiDown;
  }
  const chains = new Map<string, RoundChain>();
  for (const r of rounds) {
    try {
      chains.set(r, await checkRoundCopy(ctx, await store.round(ctx.config.covenantId, r)));
    } catch (e) {
      console.info(`round ${r} copy check: ${describe(e).code ?? 'error'}: ${describe(e).detail}`);
      chains.set(r, { copy: 'missing', pools: [] });
    }
  }
  const live = tickets.filter((t) => !t.ended);
  const utxos = live.length ? unwrap(await ctx.lottery.call('nodeEntries', live.map(objectOf))) : [];
  const cards: CardData[] = [];
  for (const t of tickets) {
    const rc = chains.get(t.round) ?? { copy: 'missing' as CopyState, pools: [] };
    const isVoid = voids.has(t.round);
    const stage = rc.round?.stage;
    if (t.ended) {
      cards.push({ stored: t, spent: false, copy: rc.copy, stage, void: isVoid });
      continue;
    }
    const u = utxos.find((x) => sameOutpoint(x, t.ticket.utxo));
    if (!u) {
      cards.push({ stored: t, spent: true, copy: rc.copy, stage, void: isVoid });
      continue;
    }
    if (rc.copy === 'mismatch') {
      // A kept copy the chain contradicts builds nothing (its counts would not be the chain's).
      cards.push({
        stored: t,
        spent: false,
        copy: rc.copy,
        stage,
        void: isVoid,
        error: { code: 'copy_mismatch', message: messageOf('copy_mismatch') },
      });
      continue;
    }
    const ticket: CovUtxo = { utxo: u, state: t.ticket.state };
    const drawn = rc.round?.stage === 'drawn';
    const r = await ctx.lottery.call('ticketCard', {
      covenantId: ctx.config.covenantId,
      ticket,
      pools: rc.pools,
      purchases: drawn ? rc.round?.purchases : undefined,
      winning: drawn ? rc.round?.result?.winning : undefined,
      claimDeadline: drawn ? rc.round?.result?.claimDeadline : undefined,
      void: isVoid,
      virtualDaaScore: node.virtualDaaScore,
    });
    if ('error' in r) {
      const copy: CopyState =
        r.error.code === 'tree_mismatch' || r.error.code === 'chain_mismatch' ? 'mismatch' : rc.copy;
      cards.push({
        stored: t,
        spent: false,
        copy,
        stage,
        void: isVoid,
        error: { code: r.error.code, message: r.error.message },
      });
      continue;
    }
    cards.push({
      stored: t,
      card: r.ok,
      chain: { stored: t, ticket, pools: rc.pools, purchases: rc.round?.purchases ?? [], card: r.ok },
      spent: false,
      copy: rc.copy,
      stage,
      void: isVoid,
    });
  }
  return { cards, copyBox: copyBoxOf(cards), apiDown, virtualDaaScore: node.virtualDaaScore, observedAt };
}
