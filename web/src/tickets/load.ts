// S2 My tickets' data (spec 3 S2, 6.3; J2, J3): the store brought up to date, then every ticket as a card.
//
// 1. Purchases whose outcome the page did not learn (F2/F3 review S3, the `pending` store): the ticket output
//    live on the node makes it a ticket; otherwise, within RESEND_MS of its signature, the same signed
//    transaction is sent again, and an input spent without our ticket drops it. An older one is never sent
//    by itself (F4 review note): it waits for the player's press (`resendPending`).
//    Claims and returns whose outcome is not known (F4 review S6, a ticket's `pending`): their successor or
//    payout output live settles them; the same RESEND_MS rule for sending again.
// 2. J3: with a wallet connected and the indexer API up, the live tickets paying out to its address are
//    found (`/v1/tickets?payout=`), each read from the node by outpoint and checked (`checkIndexerObjects`);
//    only a ticket whose payout (read by the wasm from its state) is this wallet's is kept (F4 review S2).
// 3. Each round the store holds tickets in is copied again when the chain shows a later stage, or when its
//    copy cannot tell a ticket's result (`copyRound` and `mergeCopy`: never a stage back, never fewer pools).
//    "Void" is the indexer's word checked against the chain (`checkIndexerRound`'s `voidSupported`) and
//    against this copy (a live marker or a kept result refuse it; F4 review S1).
// 4. Each round copy is checked again against the chain without the API (`checkCopy`: the lists against the
//    live marker or a live pool, found on a node by script), which also gives the round's live pools and the
//    pools the chain shows absorbed.
// 5. Each ticket's entry from the node by outpoint, and its card from the wasm (`ticketCard`).
// With the API down, steps 2 and 3 are skipped: the claim needs only the local copy and a node.
import type {
  ApiObject,
  ApiRound,
  CovUtxo,
  RoundCopyStage,
  StoredAction,
  StoredPending,
  StoredRound,
  StoredTicket,
  TicketCard,
  Utxo,
} from '@wasm/web.js';
import { describe, messageOf, PageError, unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { copyRound } from '@/store/roundCopy';
import { ticketKey, type WebStore } from '@/store/webstore';
import { settleSubmit } from '@/play/buyFlow';
import { settledTicket, type TicketChain } from './claimFlow';
import { firstLive, type Watched } from './send';

/** A signed purchase, claim or return is sent again by itself only this long after it was signed. */
export const RESEND_MS = 10 * 60_000;

export type CopyState = 'verified' | 'open' | 'gone' | 'incomplete' | 'missing' | 'mismatch';

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
  /** The round is void: the indexer says so and neither the chain nor this copy contradicts it (F4 review S1). */
  void: boolean;
  error?: { code: string | null; message: string };
}

/** The local-copy box (spec 4): complete and verified, incomplete, mismatch, not needed. */
export type CopyBox = 'complete' | 'incomplete' | 'mismatch' | 'not-needed';

export interface TicketsData {
  cards: CardData[];
  /** Signed purchases whose outcome is unknown and that are no longer sent by themselves (a press sends them). */
  stale: StoredPending[];
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

/** The node's entries for `objects`, asked in batches of the most one call reads (F4 review S7). */
export async function nodeEntries(ctx: Ctx, objects: ApiObject[]): Promise<Utxo[]> {
  if (!objects.length) return [];
  const max = (await ctx.lottery.call('indexerLimits')).nodeObjects;
  const out: Utxo[] = [];
  for (let i = 0; i < objects.length; i += max) {
    out.push(...unwrap(await ctx.lottery.call('nodeEntries', objects.slice(i, i + max))));
  }
  return out;
}

const submitOf = (ctx: Ctx) => async (s: { transaction: string; id: string }) =>
  unwrap(await ctx.lottery.call('nodeSubmit', s));

function pendingObject(p: StoredPending): ApiObject {
  return {
    role: 'ticket',
    transactionId: p.key,
    index: p.ticket.index,
    amount: p.buy.deposit,
    blockDaaScore: '0',
    scriptPublicKey: p.ticket.scriptPublicKey,
    state: p.ticket.state,
  };
}

async function pendingLive(ctx: Ctx, p: StoredPending): Promise<Utxo | undefined> {
  const u = (await nodeEntries(ctx, [pendingObject(p)])).find((x) => x.index === p.ticket.index);
  if (u)
    unwrap(await ctx.lottery.call('verifyState', ctx.config.covenantId, 'ticket', { utxo: u, state: p.ticket.state }));
  return u;
}

async function keepTicket(store: WebStore, p: StoredPending, u: Utxo): Promise<void> {
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

/** Send one pending purchase again (the same signed transaction) and settle it. */
async function settleOne(ctx: Ctx, store: WebStore, p: StoredPending): Promise<void> {
  try {
    await settleSubmit(p.signed, submitOf(ctx), {
      landed: async () => (await pendingLive(ctx, p)) !== undefined,
      tries: 1,
    });
    const u = await pendingLive(ctx, p);
    if (u) await keepTicket(store, p, u);
  } catch (e) {
    // An input spent and our ticket not live: it did not go through.
    if (e instanceof PageError && e.code === 'node_conflict') await store.delPending(p.key);
    else throw e;
  }
}

/** Step 1: settle the purchases whose outcome is not known; returns the ones that wait for a press. */
export async function settlePending(ctx: Ctx, store: WebStore, now: () => number = Date.now): Promise<StoredPending[]> {
  const stale: StoredPending[] = [];
  for (const p of await store.pending(ctx.config.covenantId)) {
    try {
      const u = await pendingLive(ctx, p);
      if (u) await keepTicket(store, p, u);
      else if (now() - Number(p.savedMs) <= RESEND_MS) await settleOne(ctx, store, p);
      else stale.push(p);
    } catch (e) {
      console.info(`pending purchase ${p.key}: ${describe(e).code ?? 'error'}: ${describe(e).detail}`);
    }
  }
  return stale;
}

/** The player's press on a pending purchase: the same signed transaction sent again, then settled. */
export async function resendPending(ctx: Ctx, store: WebStore, key: string): Promise<void> {
  const p = (await store.pending(ctx.config.covenantId)).find((x) => x.key === key);
  if (p) await settleOne(ctx, store, p);
}

/** The outputs that show a pending claim or return went through. */
function actionWatch(a: StoredAction): Watched[] {
  const w: Watched[] = [];
  const s = a.successor;
  if (s) {
    w.push({
      index: s.index,
      amount: a.successorValue ?? '0',
      scriptPublicKey: s.scriptPublicKey,
      state: s.state,
      role: 'ticket',
    });
  }
  if (a.paid)
    w.push({ index: a.paid.index, amount: a.paid.value, scriptPublicKey: a.paid.scriptPublicKey, role: 'ticket' });
  return w;
}

/** Step 1 (F4 review S6): settle the claims and returns whose outcome is not known. */
export async function settleActions(ctx: Ctx, store: WebStore, now: () => number = Date.now): Promise<void> {
  for (const t of await store.tickets(ctx.config.covenantId)) {
    const a = t.pending;
    if (!a || t.ended) continue;
    try {
      const watch = actionWatch(a);
      const hit = await firstLive(ctx, a.signed.id, watch);
      if (hit) {
        await store.putTicket(settledTicket(t, a, { index: hit.w.index, entry: hit.u }));
        continue;
      }
      const cur = (await nodeEntries(ctx, [objectOf(t)])).find((x) => sameOutpoint(x, t.ticket.utxo));
      // The ticket spent by another transaction: nothing of this action can land any more.
      if (!cur) await store.putTicket({ ...t, pending: undefined });
      else if (now() - Number(a.savedMs) <= RESEND_MS) {
        await settleSubmit(a.signed, submitOf(ctx), {
          landed: async () => (await firstLive(ctx, a.signed.id, watch)) !== undefined,
          tries: 1,
        });
      }
      // Older: never sent by itself; the card offers the claim again (a fresh build needs a fresh press).
    } catch (e) {
      if (e instanceof PageError && e.code === 'node_conflict') await store.putTicket({ ...t, pending: undefined });
      else console.info(`pending ${a.kind} of ${t.key}: ${describe(e).code ?? 'error'}: ${describe(e).detail}`);
    }
  }
}

function sameOutpoint(a: { transactionId: string; index: number }, b: { transactionId: string; index: number }) {
  return a.transactionId.toLowerCase() === b.transactionId.toLowerCase() && a.index === b.index;
}

/**
 * Step 2 (J3): the live tickets paying out to `address`, found through the indexer and checked on the node.
 * A ticket whose payout is not this wallet's is never kept as the wallet's (F4 review S2), and a ticket this
 * browser follows already (also as the successor of its pending claim) is not added twice.
 */
export async function findByPayout(ctx: Ctx, store: WebStore, address: string): Promise<number> {
  const api = await ctx.indexer.tickets(address);
  if (!api.tickets.length) return 0;
  const utxos = await nodeEntries(ctx, api.tickets);
  const live = api.tickets.filter((o) => utxos.some((u) => sameOutpoint(u, o)));
  if (!live.length) return 0;
  const checked = unwrap(
    await ctx.lottery.call('checkIndexerObjects', { covenantId: ctx.config.covenantId, objects: live, utxos }),
  );
  const have = await store.tickets(ctx.config.covenantId);
  const followed = (o: { transactionId: string; index: number }) =>
    have.some(
      (t) =>
        sameOutpoint(t.ticket.utxo, o) ||
        (t.pending?.successor &&
          sameOutpoint({ transactionId: t.pending.signed.id, index: t.pending.successor.index }, o)),
    );
  const node = unwrap(await ctx.lottery.call('nodeInfo'));
  let added = 0;
  for (const c of checked) {
    if (c.view.role !== 'ticket') continue;
    if (followed(c.object.utxo)) continue;
    // The bundle and payout as the wasm reads them from the state (no TypeScript decoding).
    const card = unwrap(
      await ctx.lottery.call('ticketCard', {
        covenantId: ctx.config.covenantId,
        ticket: c.object,
        virtualDaaScore: node.virtualDaaScore,
      }),
    );
    if (card.payout !== address) {
      console.info(
        `tickets by payout: ${c.object.utxo.transactionId}:${c.object.utxo.index} pays ${card.payout}, not ${address}`,
      );
      continue;
    }
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

/** Whether the indexer's "void" for round `r` stands against the chain (S1): `voidSupported`, and no kept result. */
async function voidStands(ctx: Ctx, api: ApiRound, have: StoredRound | undefined): Promise<boolean> {
  if (have?.stage === 'drawn') return false;
  const utxos = await nodeEntries(ctx, api.objects);
  const c = await ctx.lottery.call('checkIndexerRound', { covenantId: ctx.config.covenantId, round: api, utxos });
  if ('error' in c) {
    console.info(`round ${api.round} said void: ${c.error.code}: ${c.error.detail}`);
    return false;
  }
  return c.ok.voidSupported;
}

/**
 * Step 3: copy each round again when the chain shows a later stage, or (`force`) when its copy cannot tell a
 * ticket's result; reports the rounds whose "void" stands against the chain.
 */
async function syncRounds(
  ctx: Ctx,
  store: WebStore,
  rounds: string[],
  force: Set<string> = new Set(),
): Promise<{ voids: Set<string>; apiDown: boolean }> {
  const voids = new Set<string>();
  for (const r of rounds) {
    const have = await store.round(ctx.config.covenantId, r);
    if (have?.stage === 'drawn' && !force.has(r)) continue;
    try {
      const api = await ctx.indexer.round(r);
      if (api.status === 'void') {
        if (await voidStands(ctx, api, have)) voids.add(r);
        continue;
      }
      const want = api.status === 'open' ? 0 : api.status === 'closing' ? 1 : 2;
      if (have && STAGE[have.stage] >= want && !force.has(r)) continue;
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
  absorbed: number[];
  complete: boolean;
  round?: StoredRound;
}

/** Step 4: a round copy against the chain, without the API. */
export async function checkRoundCopy(
  ctx: Ctx,
  round: StoredRound | undefined,
  virtualDaaScore?: string,
): Promise<RoundChain> {
  const none = { pools: [], absorbed: [], complete: false };
  if (!round) return { copy: 'missing', ...none };
  if (round.stage === 'open') return { copy: 'open', ...none, round };
  const scripts = [round.against, round.marker, ...(round.result?.pools ?? [])]
    .filter((o): o is NonNullable<typeof o> => !!o)
    .map((o) => o.scriptPublicKey)
    .filter((s, i, a) => a.indexOf(s) === i)
    .slice(0, 16);
  const live = scripts.length
    ? unwrap(await ctx.lottery.call('nodeScriptEntries', { covenantId: ctx.config.covenantId, scripts }))
    : [];
  const c = await ctx.lottery.call('checkCopy', { covenantId: ctx.config.covenantId, round, live, virtualDaaScore });
  if ('error' in c) {
    if (c.error.code === 'copy_mismatch') return { copy: 'mismatch', ...none, round };
    throw new PageError(c.error.code, c.error.detail);
  }
  return { copy: c.ok.status, pools: c.ok.pools, absorbed: c.ok.absorbed, complete: c.ok.complete, round };
}

/** A card whose result the copy cannot tell (a winning tier whose pool it does not show). */
function unsure(c: CardData): boolean {
  return c.copy === 'incomplete' || !!c.card?.combos.some((x) => x.status === 'unknown');
}

/** The local-copy box's state from every live ticket's round copy. */
export function copyBoxOf(cards: CardData[]): CopyBox {
  const active = cards.filter((c) => !c.stored.ended && !c.spent);
  if (!active.length) return 'not-needed';
  if (active.some((c) => c.copy === 'mismatch')) return 'mismatch';
  if (active.some(unsure)) return 'incomplete';
  if (active.every((c) => c.copy === 'verified' || c.copy === 'gone' || c.void)) return 'complete';
  return 'incomplete';
}

/** Steps 4-5 for the tickets held. */
async function cardsOf(
  ctx: Ctx,
  store: WebStore,
  tickets: StoredTicket[],
  voids: Set<string>,
  vdaa: string,
): Promise<CardData[]> {
  const rounds = [...new Set(tickets.filter((t) => !t.ended).map((t) => t.round))];
  const chains = new Map<string, RoundChain>();
  for (const r of rounds) {
    try {
      chains.set(r, await checkRoundCopy(ctx, await store.round(ctx.config.covenantId, r), vdaa));
    } catch (e) {
      console.info(`round ${r} copy check: ${describe(e).code ?? 'error'}: ${describe(e).detail}`);
      chains.set(r, { copy: 'missing', pools: [], absorbed: [], complete: false });
    }
  }
  const live = tickets.filter((t) => !t.ended);
  const utxos = await nodeEntries(ctx, live.map(objectOf));
  const cards: CardData[] = [];
  for (const t of tickets) {
    const rc = chains.get(t.round) ?? { copy: 'missing' as CopyState, pools: [], absorbed: [], complete: false };
    const stage = rc.round?.stage;
    // This copy refuses a void: the round drawn here, or its marker live (closing).
    const isVoid = voids.has(t.round) && stage !== 'drawn' && !(stage === 'closed' && rc.copy === 'verified');
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
    const drawn = stage === 'drawn';
    const r = await ctx.lottery.call('ticketCard', {
      covenantId: ctx.config.covenantId,
      ticket,
      pools: rc.pools,
      absorbed: drawn ? rc.absorbed : [],
      complete: drawn && rc.complete,
      purchases: drawn ? rc.round?.purchases : undefined,
      winning: drawn ? rc.round?.result?.winning : undefined,
      claimDeadline: drawn ? rc.round?.result?.claimDeadline : undefined,
      void: isVoid,
      virtualDaaScore: vdaa,
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
  return cards;
}

function sorted(tickets: StoredTicket[]): StoredTicket[] {
  return tickets.sort((a, b) =>
    BigInt(b.round) > BigInt(a.round) ? 1 : BigInt(b.round) < BigInt(a.round) ? -1 : a.key.localeCompare(b.key),
  );
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
  const vdaa = node.virtualDaaScore;
  const observedAt = now();
  const stale = await settlePending(ctx, store, now);
  await settleActions(ctx, store, now);
  let apiDown = opts.online === false;
  if (address && !apiDown) {
    try {
      await findByPayout(ctx, store, address);
    } catch (e) {
      if (apiUnavailable(e)) apiDown = true;
      else console.info(`tickets by payout: ${describe(e).code ?? 'error'}: ${describe(e).detail}`);
    }
  }
  let tickets = sorted(await store.tickets(ctx.config.covenantId));
  const rounds = [...new Set(tickets.filter((t) => !t.ended).map((t) => t.round))];
  let voids = new Set<string>();
  if (!apiDown) {
    const s = await syncRounds(ctx, store, rounds);
    voids = s.voids;
    apiDown = s.apiDown;
  }
  let cards = await cardsOf(ctx, store, tickets, voids, vdaa);
  // A copy that cannot tell a ticket's result (F4 review M2, S4): taken again from the API, then checked again.
  const force = new Set(
    cards.filter((c) => !c.stored.ended && !c.spent && !c.void && unsure(c)).map((c) => c.stored.round),
  );
  if (force.size && !apiDown) {
    const s = await syncRounds(ctx, store, [...force], force);
    for (const v of s.voids) voids.add(v);
    apiDown = s.apiDown;
    tickets = sorted(await store.tickets(ctx.config.covenantId));
    cards = await cardsOf(ctx, store, tickets, voids, vdaa);
  }
  return { cards, stale, copyBox: copyBoxOf(cards), apiDown, virtualDaaScore: vdaa, observedAt };
}
