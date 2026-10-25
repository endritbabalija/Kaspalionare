// The purchase (spec S1, 6.2, 7): read the open round's live shards (the indexer's objects, each checked
// against the node's entry for its outpoint), build on one of them in the wasm (`buildBuy`), take the cost
// panel's rows from the built transaction (`buyFigures`), have the wallet sign the listed inputs only, run
// the post-sign check (`assemble`) and only then submit; wait until the ticket is in the node's UTXO set,
// keep it in the local store (R1: outpoint, full state bytes, the node's entry) and start the round's copy.
// On shard contention (the node says an input is already spent: node_conflict) the purchase is rebuilt on
// another shard, up to 3 times (spec S1), and says so; the rebuild is signed only when it is the purchase
// the player saw (F2/F3 review M2: same round, combinations, price, deposit, referral, fee and total),
// else the new figures are shown for a fresh press, and a round that stopped selling ends in sales_closed.
// When the node's answer to a submit is lost, the page finds out from the chain whether the purchase went
// through before it says anything else (F2/F3 review S3). No lottery arithmetic here: every figure comes
// from the wasm or the chain.
import type {
  ApiObject,
  BuiltTx,
  BuyFigures,
  BuyInput,
  CheckedInstance,
  CheckedObject,
  Combination,
  GameShape,
  Signed,
  Utxo,
} from '@wasm/web.js';
import type { TxStep } from '@/components/TxProgress';
import type { DevHooks } from '@/dev/hooks';
import { PageError, unwrap } from '@/lib/errors';
import { checkInstance, type Ctx } from '@/net/round';
import { copyRoundSoon } from '@/store/roundCopy';
import { ticketKey, type WebStore } from '@/store/webstore';
import type { WalletAdapter } from '@/wallet/types';
import type { LotteryClient } from '@/worker/client';

/** Spec S1: rebuilds on another shard after a conflict. */
export const REBUILDS_MAX = 3;

const instances = new WeakMap<Ctx, Promise<CheckedInstance>>();

/** The launch bundle checked once per session (B_MAX, the instance). */
export function instanceOf(ctx: Ctx): Promise<CheckedInstance> {
  let p = instances.get(ctx);
  if (!p) {
    p = checkInstance(ctx);
    p.catch(() => instances.delete(ctx));
    instances.set(ctx, p);
  }
  return p;
}

export interface Sales {
  round: string;
  /** Whether the round sells now: live shards of the round on the node and the node's clock before T. */
  open: boolean;
  /** The bonus range, from the live shards' state. */
  m: number;
  /** T (DAA score), from the live shards' state. */
  deadline: string;
  bMax: number;
  /** The game's shape (5 of 1..30, the coins a build takes), from the wasm. */
  shape: GameShape;
  shards: CheckedObject[];
  virtualDaaScore: string;
  observedAt: number;
}

function shardIndexOf(o: CheckedObject): number {
  return o.view.role === 'shard' ? Number(o.view.shardIndex) : -1;
}

function sameOutpoint(u: Utxo, o: ApiObject): boolean {
  return u.transactionId.toLowerCase() === o.transactionId.toLowerCase() && u.index === o.index;
}

/**
 * The current round's live shards, each checked against the node (spent ones, which the indexer may still
 * list, are left out). Whether the round sells comes from the chain only (F2/F3 review S4): shards of the
 * round live on the node (`close()` spends them all) and the node's clock before their deadline T; the
 * indexer's status field is not used.
 */
export async function loadSales(ctx: Ctx, now: () => number = Date.now): Promise<Sales> {
  const status = await ctx.indexer.status();
  if (status.covenantId.toLowerCase() !== ctx.config.covenantId) {
    throw new PageError('chain_mismatch', `the indexer serves instance ${status.covenantId}`);
  }
  if (!status.currentRound) throw new PageError('indexer_unavailable', 'the index has no round yet');
  const [inst, round, shape] = await Promise.all([
    instanceOf(ctx),
    ctx.indexer.round(status.currentRound),
    ctx.lottery.call('gameShape'),
  ]);
  const objs = round.objects.filter((o) => o.role === 'shard');
  const utxos = objs.length ? unwrap(await ctx.lottery.call('nodeEntries', objs)) : [];
  const live = objs.filter((o) => utxos.some((u) => sameOutpoint(u, o)));
  const shards = live.length
    ? unwrap(await ctx.lottery.call('checkIndexerObjects', { covenantId: ctx.config.covenantId, objects: live, utxos }))
    : [];
  const other = shards.find((s) => s.view.role !== 'shard' || s.view.round !== round.round);
  if (other) throw new PageError('chain_mismatch', `round ${round.round}: a shard of another round was served`);
  const node = unwrap(await ctx.lottery.call('nodeInfo'));
  const first = shards[0]?.view;
  const m = first?.role === 'shard' ? Number(first.m) : 0;
  const deadline = first?.role === 'shard' ? first.deadline : '0';
  const open = shards.length > 0 && BigInt(node.virtualDaaScore) < BigInt(deadline);
  return {
    round: round.round,
    open,
    m,
    deadline,
    bMax: inst.bMax,
    shape,
    shards,
    virtualDaaScore: node.virtualDaaScore,
    observedAt: now(),
  };
}

export interface Prepared {
  input: BuyInput;
  built: BuiltTx;
  figures: BuyFigures;
  shardIndex: number;
}

export interface BuyEnv {
  ctx: Ctx;
  wallet: WalletAdapter;
  /** The wallet's address: the ticket's payout and the change. */
  address: string;
  store?: WebStore;
  dev?: DevHooks;
  /** A uniform integer in [0, n) (the shard pick). */
  pick?: (n: number) => number;
  sleep?: (ms: number) => Promise<void>;
  /** The submit (tests replace it); default nodeSubmit. */
  submit?: (s: Signed) => Promise<string>;
  /** Waits (tests shorten them). */
  timing?: Partial<Timing>;
}

export interface Timing {
  /** How long a sent purchase may take to reach the node's UTXO set before the page asks the node again. */
  acceptMs: number;
  /** Such windows before the outcome is given up as unknown. */
  acceptWindows: number;
  /** Tries to learn a lost submit's outcome, and the pause between them. */
  settleTries: number;
  settleDelayMs: number;
}

const TIMING: Timing = { acceptMs: 120_000, acceptWindows: 5, settleTries: 20, settleDelayMs: 3_000 };

function timing(env: BuyEnv): Timing {
  return { ...TIMING, ...env.timing };
}

function sleeper(env: BuyEnv): (ms: number) => Promise<void> {
  return env.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
}

/** The wallet's coins for a build: plain coins, the largest first, at most `max` (the wasm's F_MAX). */
export function coinsFor(coins: Utxo[], max: number): Utxo[] {
  const big = (c: Utxo) => BigInt(c.amount);
  return [...coins]
    .filter((c) => !c.covenantId)
    .sort((a, b) => (big(b) > big(a) ? 1 : big(b) < big(a) ? -1 : 0))
    .slice(0, max);
}

function defaultPick(n: number): number {
  const b = new Uint32Array(1);
  crypto.getRandomValues(b);
  return (b[0] as number) % n;
}

/** Build the purchase on one live shard not in `exclude`, with its cost panel figures. */
export async function prepareBuy(
  env: BuyEnv,
  sales: Sales,
  bundle: Combination[],
  referrer: string | undefined,
  exclude: ReadonlySet<number> = new Set(),
): Promise<Prepared> {
  const { ctx } = env;
  const candidates = sales.shards.filter((s) => !exclude.has(shardIndexOf(s)));
  if (!candidates.length) throw new PageError('node_conflict', `round ${sales.round}: no live shard left to build on`);
  // The payout and the change go to the address the wallet signs for (F2/F3 review S1).
  const signer = await env.wallet.address();
  if (signer !== env.address) {
    throw new PageError(
      'wallet_changed',
      `the wallet signs for ${signer.slice(-8)}, the page pays out to ${env.address.slice(-8)}`,
    );
  }
  const forced = env.dev?.forceShard;
  const chosen =
    (forced !== undefined ? candidates.find((s) => shardIndexOf(s) === forced) : undefined) ??
    (candidates[(env.pick ?? defaultPick)(candidates.length)] as CheckedObject);
  const shardIndex = shardIndexOf(chosen);
  if (env.dev) env.dev.lastShard = shardIndex;
  const [purchases, coins, rate] = await Promise.all([
    ctx.indexer.purchases(sales.round),
    env.wallet.utxos(),
    ctx.lottery.call('nodeFeeRate'),
  ]);
  const input: BuyInput = {
    covenantId: ctx.config.covenantId,
    shard: chosen.object,
    purchases: purchases.shards[shardIndex] ?? '',
    bundle,
    payout: env.address,
    referrer,
    funds: { coins: coinsFor(coins, sales.shape.coinsMax), change: env.address, feeRate: unwrap(rate) },
  };
  const built = unwrap(await ctx.lottery.call('buildBuy', input));
  const figures = unwrap(
    await ctx.lottery.call('buyFigures', {
      covenantId: input.covenantId,
      shard: input.shard,
      bundle,
      payout: input.payout,
      referrer,
      built,
    }),
  );
  return { input, built, figures, shardIndex };
}

/**
 * The cost panel's build with the referrer the browser remembers, falling back to the site owner's when a
 * remembered link cannot be used for this purchase (F2/F3 review S2: a stored referral never blocks buying).
 * The panel then shows the referral the transaction really pays.
 */
export async function prepareWithReferral(
  env: BuyEnv,
  sales: Sales,
  bundle: Combination[],
  referrer: string | undefined,
  fallback: string | undefined,
): Promise<Prepared> {
  try {
    return await prepareBuy(env, sales, bundle, referrer);
  } catch (e) {
    if (!referrer || referrer === fallback || !(e instanceof PageError)) throw e;
    let p: Prepared;
    try {
      p = await prepareBuy(env, sales, bundle, fallback);
    } catch {
      throw e;
    }
    console.info(`referral link not usable for this purchase (${e.code}: ${e.detail}); the site owner's is used`);
    return p;
  }
}

/** A submit whose answer was lost: the node may or may not have the transaction. */
function outcomeUnknown(e: unknown): boolean {
  return e instanceof PageError && (e.code === 'node_unavailable' || e.code === 'worker_stopped');
}

/** The node already holds this very transaction (rusty-kaspa v2.1.0 mining/errors/src/mempool.rs:24, 28). */
function alreadyHeld(e: unknown): boolean {
  if (!(e instanceof PageError) || e.code !== 'node_refused') return false;
  const d = e.detail.toLowerCase();
  return d.includes('is already in the mempool') || d.includes('was already accepted by the consensus');
}

export interface SettleOpts {
  /** Whether the transaction's own output is in the node's UTXO set (it went through). */
  landed?: () => Promise<boolean>;
  onChecking?: (on: boolean) => void;
  sleep?: (ms: number) => Promise<void>;
  tries?: number;
  delayMs?: number;
}

async function isLanded(o: SettleOpts): Promise<boolean> {
  if (!o.landed) return false;
  try {
    return await o.landed();
  } catch {
    return false;
  }
}

/**
 * Learn whether `final` reached the node (F2/F3 review S3). The same signed transaction is sent again: it
 * has the same id and spends the same inputs, so it cannot buy twice. The node then takes it (it did not
 * have it), says it already holds it, or says an input is spent: then either this very transaction went
 * through (its output is live) or another one took the shard first (node_conflict: nothing of ours is on the
 * chain, and a rebuild is safe). With no answer after `tries`, submit_unknown.
 */
export async function settleSubmit(
  final: Signed,
  submit: (s: Signed) => Promise<string>,
  o: SettleOpts = {},
): Promise<string> {
  const sleep = o.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  const tries = o.tries ?? TIMING.settleTries;
  o.onChecking?.(true);
  try {
    let last: unknown;
    for (let i = 0; i < tries; i++) {
      if (await isLanded(o)) return final.id;
      try {
        return await submit(final);
      } catch (e) {
        last = e;
        if (alreadyHeld(e)) return final.id;
        if (e instanceof PageError && e.code === 'node_conflict') {
          if (await isLanded(o)) return final.id;
          throw e;
        }
        if (!outcomeUnknown(e)) throw e;
      }
      await sleep(o.delayMs ?? TIMING.settleDelayMs);
    }
    const why = last instanceof PageError ? `${last.code}: ${last.detail}` : String(last);
    throw new PageError(
      'submit_unknown',
      `transaction ${final.id}: no answer from the node after ${tries} tries (${why})`,
    );
  } finally {
    o.onChecking?.(false);
  }
}

/**
 * Sign, check, send (spec 6.2): the wallet signs the listed inputs only; `assemble` (the post-sign check in
 * the wasm) refuses anything the wallet changed, before anything is sent; then `submit`. A submit whose
 * answer is lost is settled from the chain (`settleSubmit`), never reported as a plain failure. Resolves to
 * the id.
 */
export async function signAndSubmit(
  lottery: LotteryClient,
  wallet: WalletAdapter,
  built: BuiltTx,
  submit: (s: Signed) => Promise<string>,
  onStep: (s: TxStep) => void,
  dev?: DevHooks,
  settle?: SettleOpts,
): Promise<{ id: string; final: Signed }> {
  const signed = await wallet.signInputs(
    built,
    built.sign.map((s) => s.index),
  );
  const final = unwrap(await lottery.call('assemble', { built, signed }));
  onStep('signed');
  if (dev?.holdBeforeSubmit) {
    await new Promise<void>((resolve) => {
      dev.holding = true;
      dev.release = () => {
        dev.holding = false;
        dev.holdBeforeSubmit = false;
        resolve();
      };
    });
  }
  let id: string;
  try {
    id = await submit(final);
  } catch (e) {
    if (!outcomeUnknown(e)) throw e;
    console.info(`submit ${final.id}: the answer was lost (${(e as PageError).detail}); asking the chain`);
    id = await settleSubmit(final, submit, settle);
  }
  if (id.toLowerCase() !== final.id.toLowerCase()) {
    throw new PageError('node_refused', `the node answered id ${id}, the transaction is ${final.id}`);
  }
  onStep('sent');
  return { id, final };
}

export type BuyEvent =
  | { kind: 'prepared'; prepared: Prepared }
  | { kind: 'step'; step: TxStep }
  | { kind: 'checking'; on: boolean }
  | { kind: 'rebuilt'; tries: number; max: number };

export interface Bought {
  kind: 'bought';
  id: string;
  prepared: Prepared;
  rebuilds: number;
  /** The round's copy, running in the background (resolves when kept). */
  copy?: Promise<unknown>;
}

/** Where a purchase stopped for the player's confirmation, and how to go on from there. */
export interface Resume {
  rebuilds: number;
  exclude: number[];
}

/** A rebuild changed what the player saw: nothing was signed; the new figures wait for a fresh press. */
export interface ToConfirm {
  kind: 'confirm';
  prepared: Prepared;
  resume: Resume;
}

function isStale(e: unknown): boolean {
  return (
    e instanceof PageError &&
    (e.code === 'chain_mismatch' || e.code === 'tree_mismatch' || e.code === 'indexer_unavailable')
  );
}

function sameBundle(a: Combination[], b: Combination[]): boolean {
  return (
    a.length === b.length &&
    a.every((x, i) => {
      const y = b[i];
      return y !== undefined && x.bonus === y.bonus && x.main.every((n, j) => n === y.main[j]);
    })
  );
}

/**
 * Whether a rebuild is the purchase the player saw (F2/F3 review M2): the same round, combinations, payout,
 * price, deposit and its date, referral (amount, paid, destination), fee and total. Only the shard and the
 * transaction itself may differ.
 */
export function sameAsShown(shown: Prepared, next: Prepared): boolean {
  const a = shown.figures;
  const b = next.figures;
  return (
    a.round === b.round &&
    a.b === b.b &&
    sameBundle(shown.input.bundle, next.input.bundle) &&
    shown.input.payout === next.input.payout &&
    a.price === b.price &&
    a.deposit === b.deposit &&
    a.depositBack === b.depositBack &&
    a.referral.amount === b.referral.amount &&
    a.referral.paid === b.referral.paid &&
    (a.referral.address ?? null) === (b.referral.address ?? null) &&
    a.fee === b.fee &&
    a.total === b.total
  );
}

/**
 * Prepare again after a conflict, in the same round only, waiting out the indexer catching up (a stale copy
 * is chain_mismatch). A round that stopped selling meanwhile (its deadline passed, or `close()` took its
 * shards) is sales_closed: a purchase never moves to the next round.
 */
async function prepareFresh(
  env: BuyEnv,
  round: string,
  bundle: Combination[],
  referrer: string | undefined,
  exclude: Set<number>,
): Promise<Prepared> {
  const sleep = sleeper(env);
  let last: unknown;
  for (let i = 0; i < 8; i++) {
    try {
      const sales = await loadSales(env.ctx);
      if (sales.round !== round || !sales.open) {
        throw new PageError('sales_closed', `round ${round} stopped selling (the page reads round ${sales.round})`);
      }
      return await prepareBuy(env, sales, bundle, referrer, exclude);
    } catch (e) {
      if (!isStale(e)) throw e;
      last = e;
      await sleep(1500);
    }
  }
  throw last;
}

function ticketObject(id: string, f: BuyFigures): ApiObject {
  return {
    role: 'ticket',
    transactionId: id,
    index: f.ticket.index,
    amount: f.deposit,
    blockDaaScore: '0',
    scriptPublicKey: f.ticket.scriptPublicKey,
    state: f.ticket.state,
  };
}

/** The ticket's entry when the node holds it (the purchase is in a block), checked against its state. */
export async function ticketEntry(env: BuyEnv, id: string, f: BuyFigures): Promise<Utxo | undefined> {
  const { lottery } = env.ctx;
  const u = unwrap(await lottery.call('nodeEntries', [ticketObject(id, f)])).find((x) => x.index === f.ticket.index);
  if (!u) return undefined;
  unwrap(await lottery.call('verifyState', env.ctx.config.covenantId, 'ticket', { utxo: u, state: f.ticket.state }));
  return u;
}

/**
 * The ticket's entry once the node holds it. When it takes longer than `acceptMs`, the node is asked again
 * with the same signed transaction (`settleSubmit`): still held, the wait goes on; dropped and its shard
 * taken, node_conflict (a rebuild is safe); no answer, or still not in a block after `acceptWindows`
 * windows, submit_unknown (F2/F3 review S3).
 */
export async function waitAccepted(
  env: BuyEnv,
  id: string,
  f: BuyFigures,
  final?: Signed,
  onChecking?: (on: boolean) => void,
): Promise<Utxo> {
  const t = timing(env);
  const sleep = sleeper(env);
  const submit = env.submit ?? (async (s: Signed) => unwrap(await env.ctx.lottery.call('nodeSubmit', s)));
  for (let w = 0; w < t.acceptWindows; w++) {
    const until = Date.now() + t.acceptMs;
    while (Date.now() < until) {
      const u = await ticketEntry(env, id, f).catch((e: unknown) => {
        if (outcomeUnknown(e)) return undefined;
        throw e;
      });
      if (u) return u;
      await sleep(1000);
    }
    if (!final) break;
    await settleSubmit(final, submit, {
      landed: async () => (await ticketEntry(env, id, f)) !== undefined,
      onChecking,
      sleep,
      tries: t.settleTries,
      delayMs: t.settleDelayMs,
    });
  }
  throw new PageError(
    'submit_unknown',
    `transaction ${id}: its ticket is not in the node's UTXO set after ${(t.acceptWindows * t.acceptMs) / 1000} s`,
  );
}

/**
 * The whole purchase, from a prepared build (what the cost panel showed and the player pressed Buy on),
 * with the rebuilds of spec S1. Resolves to the bought ticket, or to `confirm` when a rebuild differs from
 * what was shown (nothing signed); rejects with sales_closed when the round stopped selling.
 */
export async function buy(
  env: BuyEnv,
  first: Prepared,
  referrer: string | undefined,
  onEvent: (e: BuyEvent) => void,
  resume?: Resume,
): Promise<Bought | ToConfirm> {
  const { ctx } = env;
  const bundle = first.input.bundle;
  const exclude = new Set<number>(resume?.exclude ?? []);
  let prepared = first;
  let rebuilds = resume?.rebuilds ?? 0;
  const submit = env.submit ?? (async (s: Signed) => unwrap(await ctx.lottery.call('nodeSubmit', s)));
  const t = timing(env);
  const onChecking = (on: boolean) => onEvent({ kind: 'checking', on });
  let id: string;
  let entry: Utxo;
  for (;;) {
    onEvent({ kind: 'step', step: 'built' });
    const f = prepared.figures;
    try {
      const sent = await signAndSubmit(
        ctx.lottery,
        env.wallet,
        prepared.built,
        submit,
        (step) => onEvent({ kind: 'step', step }),
        env.dev,
        {
          landed: async () => (await ticketEntry(env, prepared.built.id, f)) !== undefined,
          onChecking,
          sleep: env.sleep,
          tries: t.settleTries,
          delayMs: t.settleDelayMs,
        },
      );
      id = sent.id;
      entry = await waitAccepted(env, id, f, sent.final, onChecking);
      break;
    } catch (e) {
      if (!(e instanceof PageError && e.code === 'node_conflict') || rebuilds >= REBUILDS_MAX) throw e;
      exclude.add(prepared.shardIndex);
      rebuilds++;
      onEvent({ kind: 'rebuilt', tries: rebuilds, max: REBUILDS_MAX });
      const next = await prepareFresh(env, first.figures.round, bundle, referrer, exclude);
      onEvent({ kind: 'prepared', prepared: next });
      if (!sameAsShown(first, next)) {
        return { kind: 'confirm', prepared: next, resume: { rebuilds, exclude: [...exclude] } };
      }
      prepared = next;
    }
  }
  onEvent({ kind: 'step', step: 'accepted' });
  const f = prepared.figures;
  let copy: Promise<unknown> | undefined;
  if (env.store) {
    const store = env.store;
    await store.putTicket({
      key: ticketKey(id, f.ticket.index),
      covenantId: ctx.config.covenantId,
      network: ctx.config.network,
      round: f.round,
      shardIndex: f.shardIndex,
      ticket: { utxo: entry, state: f.ticket.state },
      bundle,
      payout: env.address,
      buy: {
        transactionId: id,
        price: f.price,
        deposit: f.deposit,
        depositBack: f.depositBack,
        fee: f.fee,
        referral: f.referral,
      },
      savedMs: String(Date.now()),
    });
    copy = copyRoundSoon(ctx, store, f.round);
    copy.catch((e: unknown) => console.warn(`round copy ${f.round} failed`, e));
  }
  return { kind: 'bought', id, prepared, rebuilds, copy };
}
