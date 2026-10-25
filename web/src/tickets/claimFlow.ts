// Claims and deposit returns (spec S2, 7 "Claims", "Deposits"; J2): built in the wasm from the ticket's
// node entry, the live pools of its round (found on a node by their scripts from the local copy's round
// result) and the local copy's purchase lists, so neither needs the indexer API (spec 6.3: "a claim built
// from the local copy needs no API: only a public node for the live UTXOs"). The claim row's figures come
// from `claimFigures`, the return's from `returnFigures`, each checked against the built transaction, so
// what the row shows is what is signed. Sent through the purchase's path (sign the listed inputs, the
// post-sign check, then the node). The store follows: the ticket's successor after PAY or ACCRUE, the end
// after FINAL or a return.
import type {
  BuiltTx,
  ClaimFigures,
  ClaimInput,
  CovUtxo,
  ReturnFigures,
  ReturnInput,
  StoredTicket,
  TicketCard,
} from '@wasm/web.js';
import { unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { coinsFor } from '@/play/buyFlow';
import type { WebStore } from '@/store/webstore';
import type { WalletAdapter } from '@/wallet/types';
import { sendAndWait, type SendEnv, type SendEvent } from './send';

/** What a claim or a return is built from: the ticket's live entry and the round's chain objects. */
export interface TicketChain {
  stored: StoredTicket;
  /** The node's entry for the ticket's outpoint, with its state bytes. */
  ticket: CovUtxo;
  /** The round's live pools. */
  pools: CovUtxo[];
  /** The round's purchase lists (all k, from the local copy). */
  purchases: string[];
  card: TicketCard;
}

export interface ActEnv {
  ctx: Ctx;
  wallet: WalletAdapter;
  /** The wallet's address: it pays the fee and gets the change. */
  address: string;
}

async function funds(env: ActEnv) {
  const [coins, rate, shape] = await Promise.all([
    env.wallet.utxos(),
    env.ctx.lottery.call('nodeFeeRate'),
    env.ctx.lottery.call('gameShape'),
  ]);
  return { coins: coinsFor(coins, shape.coinsMax), change: env.address, feeRate: unwrap(rate) };
}

export interface PreparedClaim {
  kind: 'claim';
  input: ClaimInput;
  built: BuiltTx;
  figures: ClaimFigures;
}

export async function prepareClaim(env: ActEnv, t: TicketChain, index: number): Promise<PreparedClaim> {
  const { ctx } = env;
  const input: ClaimInput = {
    covenantId: ctx.config.covenantId,
    ticket: t.ticket,
    pools: t.pools,
    purchases: t.purchases,
    index,
    funds: await funds(env),
  };
  const built = unwrap(await ctx.lottery.call('buildClaim', input));
  const figures = unwrap(
    await ctx.lottery.call('claimFigures', {
      covenantId: input.covenantId,
      ticket: input.ticket,
      pools: input.pools,
      purchases: input.purchases,
      index,
      built,
    }),
  );
  return { kind: 'claim', input, built, figures };
}

export interface PreparedReturn {
  kind: 'return';
  input: ReturnInput;
  built: BuiltTx;
  figures: ReturnFigures;
}

export async function prepareReturn(env: ActEnv, t: TicketChain): Promise<PreparedReturn> {
  const { ctx } = env;
  const r = t.card.returnable;
  if (!r) throw new Error('return_deposit() is not valid for this ticket yet');
  const input: ReturnInput = {
    covenantId: ctx.config.covenantId,
    ticket: t.ticket,
    pool: r.pool,
    funds: await funds(env),
  };
  const built = unwrap(await ctx.lottery.call('buildReturnDeposit', input));
  const figures = unwrap(
    await ctx.lottery.call('returnFigures', {
      covenantId: input.covenantId,
      ticket: input.ticket,
      pool: input.pool,
      built,
    }),
  );
  return { kind: 'return', input, built, figures };
}

/** Send a prepared claim; then keep the ticket's successor (PAY, ACCRUE) or its end (FINAL). */
export async function sendClaim(
  env: ActEnv & Omit<SendEnv, 'ctx' | 'wallet'>,
  store: WebStore | undefined,
  t: TicketChain,
  p: PreparedClaim,
  onEvent: (e: SendEvent) => void,
): Promise<string> {
  const f = p.figures;
  const next = f.ticketNext;
  const watch = next
    ? {
        index: next.index,
        amount: p.built.summary.outputs[next.index]?.value ?? '0',
        scriptPublicKey: next.scriptPublicKey,
        state: next.state,
        role: 'ticket' as const,
      }
    : {
        index: f.poolNext.index,
        amount: f.poolNext.value,
        scriptPublicKey: f.poolNext.scriptPublicKey,
        state: f.poolNext.state,
        role: 'pool' as const,
      };
  const { id, entry } = await sendAndWait({ ...env, ctx: env.ctx, wallet: env.wallet }, p.built, watch, onEvent);
  if (store) {
    const updated: StoredTicket = next
      ? { ...t.stored, ticket: { utxo: entry, state: next.state }, savedMs: String(Date.now()) }
      : { ...t.stored, ended: { kind: 'claimed', transactionId: id, amount: f.paidNow, endedMs: String(Date.now()) } };
    await store.putTicket(updated);
  }
  return id;
}

/** Send a prepared return; then keep the ticket's end. */
export async function sendReturn(
  env: ActEnv & Omit<SendEnv, 'ctx' | 'wallet'>,
  store: WebStore | undefined,
  t: TicketChain,
  p: PreparedReturn,
  onEvent: (e: SendEvent) => void,
): Promise<string> {
  const at = p.input.pool ? 1 : 0;
  const out = p.built.summary.outputs[at];
  const spk = unwrap(await env.ctx.lottery.call('scriptOf', p.figures.payout));
  const watch = { index: at, amount: out?.value ?? p.figures.amount, scriptPublicKey: spk, role: 'ticket' as const };
  const { id } = await sendAndWait({ ...env, ctx: env.ctx, wallet: env.wallet }, p.built, watch, onEvent);
  if (store) {
    await store.putTicket({
      ...t.stored,
      ended: { kind: 'returned', transactionId: id, amount: p.figures.amount, endedMs: String(Date.now()) },
    });
  }
  return id;
}
