// The purchase (spec S1, 6.2, 7): read the open round's live shards (the indexer's objects, each checked
// against the node's entry for its outpoint), build on one of them in the wasm (`buildBuy`), take the cost
// panel's rows from the built transaction (`buyFigures`), have the wallet sign the listed inputs only, run
// the post-sign check (`assemble`) and only then submit; wait until the ticket is in the node's UTXO set,
// keep it in the local store (R1: outpoint, full state bytes, the node's entry) and start the round's copy.
// On shard contention (the node says an input is already spent: node_conflict) the purchase is rebuilt on
// another shard, up to 3 times (spec S1), and says so. No lottery arithmetic here: every figure comes from
// the wasm or the chain.
import type {
  ApiObject,
  BuiltTx,
  BuyFigures,
  BuyInput,
  CheckedInstance,
  CheckedObject,
  Combination,
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
/** The most coins one build takes (lottery::web::build::F_MAX). */
export const COINS_MAX = 8;

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
  /** Whether the round sells now: open on the chain and before T at the node's clock. */
  open: boolean;
  /** The bonus range, from the live shards' state. */
  m: number;
  /** T (DAA score), from the live shards' state. */
  deadline: string;
  bMax: number;
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

/** The current round's live shards, each checked against the node (spent ones, which the indexer may still list, are left out). */
export async function loadSales(ctx: Ctx, now: () => number = Date.now): Promise<Sales> {
  const status = await ctx.indexer.status();
  if (status.covenantId.toLowerCase() !== ctx.config.covenantId) {
    throw new PageError('chain_mismatch', `the indexer serves instance ${status.covenantId}`);
  }
  if (!status.currentRound) throw new PageError('indexer_unavailable', 'the index has no round yet');
  const [inst, round] = await Promise.all([instanceOf(ctx), ctx.indexer.round(status.currentRound)]);
  const objs = round.objects.filter((o) => o.role === 'shard');
  const utxos = objs.length ? unwrap(await ctx.lottery.call('nodeEntries', objs)) : [];
  const live = objs.filter((o) => utxos.some((u) => sameOutpoint(u, o)));
  const shards = live.length
    ? unwrap(await ctx.lottery.call('checkIndexerObjects', { covenantId: ctx.config.covenantId, objects: live, utxos }))
    : [];
  const node = unwrap(await ctx.lottery.call('nodeInfo'));
  const first = shards[0]?.view;
  const m = first?.role === 'shard' ? Number(first.m) : 0;
  const deadline = first?.role === 'shard' ? first.deadline : '0';
  const open = round.status === 'open' && shards.length > 0 && BigInt(node.virtualDaaScore) < BigInt(deadline);
  return {
    round: round.round,
    open,
    m,
    deadline,
    bMax: inst.bMax,
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
}

/** The wallet's coins for a build: plain coins, the largest first, at most COINS_MAX. */
export function coinsFor(coins: Utxo[]): Utxo[] {
  const big = (c: Utxo) => BigInt(c.amount);
  return [...coins]
    .filter((c) => !c.covenantId)
    .sort((a, b) => (big(b) > big(a) ? 1 : big(b) < big(a) ? -1 : 0))
    .slice(0, COINS_MAX);
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
    funds: { coins: coinsFor(coins), change: env.address, feeRate: unwrap(rate) },
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
 * Sign, check, send (spec 6.2): the wallet signs the listed inputs only; `assemble` (the post-sign check in
 * the wasm) refuses anything the wallet changed, before anything is sent; then `submit`. Resolves to the id.
 */
export async function signAndSubmit(
  lottery: LotteryClient,
  wallet: WalletAdapter,
  built: BuiltTx,
  submit: (s: Signed) => Promise<string>,
  onStep: (s: TxStep) => void,
  dev?: DevHooks,
): Promise<string> {
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
  const id = await submit(final);
  if (id.toLowerCase() !== final.id.toLowerCase()) {
    throw new PageError('node_refused', `the node answered id ${id}, the transaction is ${final.id}`);
  }
  onStep('sent');
  return id;
}

export type BuyEvent =
  | { kind: 'prepared'; prepared: Prepared }
  | { kind: 'step'; step: TxStep }
  | { kind: 'rebuilt'; tries: number; max: number };

export interface Bought {
  id: string;
  prepared: Prepared;
  rebuilds: number;
  /** The round's copy, running in the background (resolves when kept). */
  copy?: Promise<unknown>;
}

function isStale(e: unknown): boolean {
  return (
    e instanceof PageError &&
    (e.code === 'chain_mismatch' || e.code === 'tree_mismatch' || e.code === 'indexer_unavailable')
  );
}

/** Prepare again after a conflict, waiting out the indexer catching up (a stale copy is chain_mismatch). */
async function prepareFresh(
  env: BuyEnv,
  bundle: Combination[],
  referrer: string | undefined,
  exclude: Set<number>,
): Promise<Prepared> {
  const sleep = env.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  let last: unknown;
  for (let i = 0; i < 8; i++) {
    try {
      const sales = await loadSales(env.ctx);
      if (!sales.open) throw new PageError('refused', `round ${sales.round} is not selling`);
      return await prepareBuy(env, sales, bundle, referrer, exclude);
    } catch (e) {
      if (!isStale(e)) throw e;
      last = e;
      await sleep(1500);
    }
  }
  throw last;
}

/** The ticket's entry once the node holds it (the purchase is in a block), checked against its state. */
export async function waitAccepted(env: BuyEnv, id: string, f: BuyFigures, timeoutMs = 120_000): Promise<Utxo> {
  const { lottery } = env.ctx;
  const sleep = env.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  const obj: ApiObject = {
    role: 'ticket',
    transactionId: id,
    index: f.ticket.index,
    amount: f.deposit,
    blockDaaScore: '0',
    scriptPublicKey: f.ticket.scriptPublicKey,
    state: f.ticket.state,
  };
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    const u = unwrap(await lottery.call('nodeEntries', [obj])).find((x) => x.index === f.ticket.index);
    if (u) {
      unwrap(
        await lottery.call('verifyState', env.ctx.config.covenantId, 'ticket', { utxo: u, state: f.ticket.state }),
      );
      return u;
    }
    await sleep(1000);
  }
  throw new PageError(
    'node_unavailable',
    `transaction ${id}: its ticket is not in the node's UTXO set after ${timeoutMs / 1000} s`,
  );
}

/** The whole purchase, from a prepared build (what the cost panel showed), with the rebuilds of spec S1. */
export async function buy(
  env: BuyEnv,
  first: Prepared,
  referrer: string | undefined,
  onEvent: (e: BuyEvent) => void,
): Promise<Bought> {
  const { ctx } = env;
  const bundle = first.input.bundle;
  const exclude = new Set<number>();
  let prepared = first;
  let rebuilds = 0;
  const submit = env.submit ?? (async (s: Signed) => unwrap(await ctx.lottery.call('nodeSubmit', s)));
  let id: string;
  for (;;) {
    onEvent({ kind: 'step', step: 'built' });
    try {
      id = await signAndSubmit(
        ctx.lottery,
        env.wallet,
        prepared.built,
        submit,
        (step) => onEvent({ kind: 'step', step }),
        env.dev,
      );
      break;
    } catch (e) {
      if (!(e instanceof PageError && e.code === 'node_conflict') || rebuilds >= REBUILDS_MAX) throw e;
      exclude.add(prepared.shardIndex);
      rebuilds++;
      onEvent({ kind: 'rebuilt', tries: rebuilds, max: REBUILDS_MAX });
      prepared = await prepareFresh(env, bundle, referrer, exclude);
      onEvent({ kind: 'prepared', prepared });
    }
  }
  const entry = await waitAccepted(env, id, prepared.figures);
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
  return { id, prepared, rebuilds, copy };
}
