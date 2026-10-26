// S4 Run the round (spec 3 S4, 7; J4; F5): `close()` when T has passed and the previous round is drawn, `draw()`
// from D while the anchor proof is bound to the block a public node reports, the void from D + draw_window. Every
// due time comes from the round's facts (Rust); the page only compares them with the node's virtual DAA score.
// A press builds in the wasm from the live chain objects (`buildClose`, `buildDraw`), takes the panel's rows from
// `callerFigures` (bound to the built transaction: the reward, the fee, the reserves and when they come back,
// the total), and goes through the purchase's path: the wallet signs the listed inputs only (one request at a
// time), `assemble` checks what came back, only then the node gets it; a lost answer is settled from the chain
// (the same signed transaction again; its caller output live means it went through). The draw's anchor proof
// is the indexer's (spec 6.1 fallback), used only once `checkIndexerRound` has bound it to the node's block and
// to D; the purchases only once they rebuild the marker's frozen roots. `draw()` checks the anchor on chain.
import type { BuiltTx, CallerFigures, CloseInput, DrawInput } from '@wasm/web.js';
import { PageError, unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { coinsFor } from '@/play/buyFlow';
import { sendAndWait, type SendEnv, type SendEvent, type Watched } from '@/tickets/send';
import type { WalletAdapter } from '@/wallet/types';
import { anchorBlockOf, liveRound, objectsOf, type DetailData } from './load';

export type CallerAction = 'close' | 'draw' | 'void';

/** What the panel shows (spec 4 "Caller panel": not due, due with reward, reward below fee). */
export type Due =
  | { kind: 'none' }
  /** Open, but round r - 1 is not drawn yet: `close()` waits for its `draw()`. */
  | { kind: 'waits' }
  | { kind: 'later'; action: CallerAction; from: string }
  /** From D, but no anchor proof bound to a public node's block yet. */
  | { kind: 'anchor'; from: string }
  | { kind: 'due'; action: CallerAction; from: string };

const after = (now: string, from: string) => BigInt(now) > BigInt(from);

export function dueOf(d: DetailData): Due {
  const f = d.checked.facts;
  const now = d.clock.virtualDaa;
  if (d.status === 'open' && f.deadline && f.previousDrawn !== undefined) {
    if (!f.previousDrawn) return { kind: 'waits' };
    const from = f.deadline.value;
    return after(now, from) ? { kind: 'due', action: 'close', from } : { kind: 'later', action: 'close', from };
  }
  if (d.status === 'closing' && f.d && f.voidFrom) {
    if (after(now, f.voidFrom.value)) return { kind: 'due', action: 'void', from: f.voidFrom.value };
    if (!after(now, f.d.value)) return { kind: 'later', action: 'draw', from: f.d.value };
    return d.provisional ? { kind: 'due', action: 'draw', from: f.d.value } : { kind: 'anchor', from: f.d.value };
  }
  return { kind: 'none' };
}

export interface CallerEnv {
  ctx: Ctx;
  wallet: WalletAdapter;
  address: string;
}

export interface PreparedCaller {
  action: CallerAction;
  round: string;
  built: BuiltTx;
  figures: CallerFigures;
}

async function funds(env: CallerEnv) {
  const [coins, rate, shape] = await Promise.all([
    env.wallet.utxos(),
    env.ctx.lottery.call('nodeFeeRate'),
    env.ctx.lottery.call('gameShape'),
  ]);
  return { coins: coinsFor(coins, shape.coinsMax), change: env.address, feeRate: unwrap(rate) };
}

function one(d: DetailData, role: 'controller' | 'params' | 'marker' | 'drawer' | 'vault', round?: string) {
  const o = objectsOf(d.checked, role).find(
    (x) => round === undefined || ('round' in x.view && x.view.round === round),
  );
  if (!o) throw new PageError('chain_mismatch', `round ${d.checked.round}: no live ${role} on the node`);
  return o.object;
}

/** Build the due transition of round `d` from its live, checked objects; the panel's rows bound to it. */
export async function prepareCaller(env: CallerEnv, d: DetailData, action: CallerAction): Promise<PreparedCaller> {
  const { ctx } = env;
  const covenantId = ctx.config.covenantId;
  const r = d.checked.round;
  const signer = await env.wallet.address();
  if (signer !== env.address) throw new PageError('wallet_changed', 'the wallet signs for another address');
  if (action === 'close') {
    const shards = objectsOf(d.checked, 'shard')
      .filter((s) => s.view.role === 'shard' && s.view.round === r)
      .sort(
        (a, b) =>
          Number(a.view.role === 'shard' && a.view.shardIndex) - Number(b.view.role === 'shard' && b.view.shardIndex),
      )
      .map((s) => s.object);
    if (shards.length !== d.checked.k) {
      throw new PageError(
        'chain_mismatch',
        `round ${r}: ${shards.length} of its ${d.checked.k} shards live on the node`,
      );
    }
    const input: CloseInput = {
      covenantId,
      controller: one(d, 'controller', r),
      params: one(d, 'params'),
      shards,
      virtualDaaScore: unwrap(await ctx.lottery.call('nodeInfo')).virtualDaaScore,
      funds: await funds(env),
    };
    const built = unwrap(await ctx.lottery.call('buildClose', input));
    const figures = unwrap(
      await ctx.lottery.call('callerFigures', {
        covenantId,
        controller: input.controller,
        shards: input.shards,
        built,
      }),
    );
    return { action, round: r, built, figures };
  }
  // draw() or the void: the purchases (and the anchor proof) checked against the chain first.
  const purchases = await ctx.indexer.purchases(r);
  const { round, utxos } = await liveRound(ctx, d.api);
  const proof = action === 'draw' ? ((await ctx.indexer.anchorProof(r)) ?? undefined) : undefined;
  if (action === 'draw' && !proof)
    throw new PageError('anchor_invalid', `round ${r}: the indexer serves no anchor proof`);
  const anchorBlock = proof ? await anchorBlockOf(ctx, proof) : undefined;
  if (proof && !anchorBlock)
    throw new PageError('anchor_invalid', `round ${r}: the node does not report the anchor block`);
  const checked = unwrap(
    await ctx.lottery.call('checkIndexerRound', {
      covenantId,
      round,
      utxos,
      purchases,
      anchorProof: proof,
      anchorBlock,
    }),
  );
  if (!checked.purchases)
    throw new PageError('chain_mismatch', `round ${r}: its purchases are not checked against its roots`);
  if (proof && !(checked.winning && checked.anchorBound)) {
    throw new PageError('anchor_invalid', `round ${r}: the anchor proof is not bound to the node's block`);
  }
  const next = String(BigInt(r) + 1n);
  const ch = { ...d, checked };
  const input: DrawInput = {
    covenantId,
    drawer: one(ch, 'drawer'),
    marker: one(ch, 'marker', r),
    vault: one(ch, 'vault'),
    controller: one(ch, 'controller', next),
    absorbed: [],
    purchases: checked.purchases,
    anchorProof: proof?.anchorProof,
    funds: await funds(env),
  };
  const built = unwrap(await ctx.lottery.call('buildDraw', input));
  const figures = unwrap(
    await ctx.lottery.call('callerFigures', {
      covenantId,
      controller: input.controller,
      marker: input.marker,
      drawer: input.drawer,
      built,
    }),
  );
  if (figures.kind !== action) {
    throw new PageError('built_mismatch', `built a ${figures.kind}, the panel asked for a ${action}`);
  }
  return { action, round: r, built, figures };
}

/** The wallet's outputs of `built` (the caller output and any reserve refund): evidence it went through. */
async function walletOutputs(ctx: Ctx, built: BuiltTx, caller: number): Promise<Watched[]> {
  const out: Watched[] = [];
  const order = [caller, ...built.summary.outputs.map((o) => o.index).filter((i) => i !== caller)];
  for (const i of order) {
    const o = built.summary.outputs[i];
    if (!o?.toWallet || o.covenant || !o.address) continue;
    out.push({
      index: i,
      amount: o.value,
      scriptPublicKey: unwrap(await ctx.lottery.call('scriptOf', o.address)),
      role: 'ticket',
    });
  }
  return out;
}

/** Sign, check and send a prepared transition; accepted once one of the wallet's own outputs is live. */
export async function sendCaller(
  env: CallerEnv & Omit<SendEnv, 'ctx' | 'wallet'>,
  p: PreparedCaller,
  onEvent: (e: SendEvent) => void,
): Promise<string> {
  const watch = await walletOutputs(env.ctx, p.built, p.figures.callerOutput);
  if (!watch.length) throw new PageError('built_mismatch', 'the transaction pays nothing back to the wallet');
  const { id } = await sendAndWait({ ...env, ctx: env.ctx, wallet: env.wallet }, p.built, watch, onEvent);
  return id;
}
