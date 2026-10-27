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
  PaidOut,
  ReturnFigures,
  ReturnInput,
  Signed,
  StoredAction,
  StoredTicket,
  TicketCard,
} from '@wasm/web.js';
import { PageError, unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { coinsFor } from '@/play/buyFlow';
import type { WebStore } from '@/store/webstore';
import type { WalletAdapter } from '@/wallet/types';
import { sendAndWait, unrefused, type SendEnv, type SendEvent, type Watched } from './send';

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
  /** The pause between builds after a refusal (`unrefused`; tests). */
  sleep?: (ms: number) => Promise<void>;
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

/** Build a claim, never one the node has just refused (`unrefused`, F6 part A review N1). */
export function prepareClaim(env: ActEnv, t: TicketChain, index: number): Promise<PreparedClaim> {
  return unrefused(() => buildClaimOnce(env, t, index), env.sleep);
}

async function buildClaimOnce(env: ActEnv, t: TicketChain, index: number): Promise<PreparedClaim> {
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

/** Build a deposit return, never one the node has just refused (`unrefused`). */
export function prepareReturn(env: ActEnv, t: TicketChain): Promise<PreparedReturn> {
  return unrefused(() => buildReturnOnce(env, t), env.sleep);
}

async function buildReturnOnce(env: ActEnv, t: TicketChain): Promise<PreparedReturn> {
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

/** A plain output of `built` (a payout or the change) to watch, or nothing when it is a covenant output. */
async function plainOutput(env: ActEnv, built: BuiltTx, index: number): Promise<Watched | undefined> {
  const o = built.summary.outputs[index];
  if (!o?.address || o.covenant) return undefined;
  const scriptPublicKey = unwrap(await env.ctx.lottery.call('scriptOf', o.address));
  return { index, amount: o.value, scriptPublicKey, role: 'ticket' };
}

/** The wallet's change outputs from `from` on (evidence the transaction went through, F4 review S5). */
async function changeOutputs(env: ActEnv, built: BuiltTx, from: number): Promise<Watched[]> {
  const out: Watched[] = [];
  for (const o of built.summary.outputs.slice(from)) {
    if (!o.toWallet) continue;
    const w = await plainOutput(env, built, o.index);
    if (w) out.push(w);
  }
  return out;
}

/** What a claim or return leaves for the ticket: its successor (PAY, ACCRUE) and its payout output. */
async function actionOf(
  env: ActEnv,
  kind: StoredAction['kind'],
  built: BuiltTx,
  successor: ClaimFigures['ticketNext'],
  paidAt: number | undefined,
  index?: number,
): Promise<{ action: (s: Signed) => StoredAction; watch: Watched[] }> {
  const watch: Watched[] = [];
  if (successor) {
    watch.push({
      index: successor.index,
      amount: built.summary.outputs[successor.index]?.value ?? '0',
      scriptPublicKey: successor.scriptPublicKey,
      state: successor.state,
      role: 'ticket',
    });
  }
  const pw = paidAt === undefined ? undefined : await plainOutput(env, built, paidAt);
  if (pw) watch.push(pw);
  watch.push(...(await changeOutputs(env, built, (paidAt ?? successor?.index ?? 0) + 1)));
  const paid: PaidOut | undefined = pw
    ? { index: pw.index, value: pw.amount, scriptPublicKey: pw.scriptPublicKey }
    : undefined;
  return {
    watch,
    action: (signed) => ({
      kind,
      index,
      signed,
      successor: successor ?? undefined,
      successorValue: successor ? built.summary.outputs[successor.index]?.value : undefined,
      paid,
      savedMs: String(Date.now()),
    }),
  };
}

/**
 * A ticket record after its claim or return went through (`id` accepted): the successor (PAY, ACCRUE; with
 * the node's entry when it is still live, else as built: another claimer may have spent it already) or the
 * ticket's end (FINAL, a return). Also settles a pending action on a later visit (load.ts).
 */
export function settledTicket(
  stored: StoredTicket,
  a: StoredAction,
  live?: { index: number; entry: CovUtxo['utxo'] },
): StoredTicket {
  const id = a.signed.id;
  const now = String(Date.now());
  if (a.successor) {
    const s = a.successor;
    const utxo =
      live && live.index === s.index
        ? live.entry
        : {
            transactionId: id,
            index: s.index,
            amount: a.successorValue ?? stored.ticket.utxo.amount,
            scriptPublicKey: s.scriptPublicKey,
            blockDaaScore: '0',
            isCoinbase: false,
            covenantId: stored.covenantId,
          };
    return { ...stored, ticket: { utxo, state: s.state }, pending: undefined, savedMs: now };
  }
  return {
    ...stored,
    pending: undefined,
    ended: {
      kind: a.kind === 'claim' ? 'claimed' : 'returned',
      transactionId: id,
      amount: a.paid?.value ?? '0',
      endedMs: now,
    },
  };
}

/**
 * Send a prepared claim or return (F4 review S6): the signed transaction is kept with the ticket before it
 * is sent (`pending`), so an outcome the page does not learn is settled on the next visit and the successor
 * stays reachable without the indexer API; then the ticket's successor or its end is kept.
 */
async function sendAction(
  env: ActEnv & Omit<SendEnv, 'ctx' | 'wallet'>,
  store: WebStore | undefined,
  t: TicketChain,
  built: BuiltTx,
  plan: { action: (s: Signed) => StoredAction; watch: Watched[] },
  onEvent: (e: SendEvent) => void,
): Promise<string> {
  let action: StoredAction | undefined;
  const onSigned = async (final: Signed) => {
    action = plan.action(final);
    if (store) await store.putTicket({ ...t.stored, pending: action });
  };
  try {
    const { id, entry, watched } = await sendAndWait(
      { ...env, ctx: env.ctx, wallet: env.wallet },
      built,
      plan.watch,
      onEvent,
      onSigned,
    );
    if (store && action) await store.putTicket(settledTicket(t.stored, action, { index: watched.index, entry }));
    return id;
  } catch (e) {
    // A transaction known not to have gone through (refused, or its input taken: node_conflict) leaves no
    // pending record; an unknown outcome keeps it for the next visit.
    if (store && action && !(e instanceof PageError && e.code === 'submit_unknown')) await store.putTicket(t.stored);
    throw e;
  }
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
  // FINAL pays at output 1, PAY at output 2 (design 7.4); ACCRUE pays nothing now.
  const paidAt = f.claim.mode === 'final' ? 1 : f.claim.mode === 'pay' ? 2 : undefined;
  const plan = await actionOf(env, 'claim', p.built, f.ticketNext, paidAt, f.index);
  return sendAction(env, store, t, p.built, plan, onEvent);
}

/** Send a prepared return; then keep the ticket's end. */
export async function sendReturn(
  env: ActEnv & Omit<SendEnv, 'ctx' | 'wallet'>,
  store: WebStore | undefined,
  t: TicketChain,
  p: PreparedReturn,
  onEvent: (e: SendEvent) => void,
): Promise<string> {
  const plan = await actionOf(env, 'return', p.built, undefined, p.input.pool ? 1 : 0);
  return sendAction(env, store, t, p.built, plan, onEvent);
}
