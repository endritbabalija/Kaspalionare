// Sending a claim or a deposit return (spec 6.2, S2): the same path as a purchase (buyFlow's
// `signAndSubmit`: the wallet signs the listed inputs only, `assemble` checks what came back, only then the
// node gets it; a lost answer is settled from the chain). "Accepted" is one of the transaction's own outputs
// in the node's UTXO set (its entry read by outpoint, never from the indexer). Several outputs are watched
// (F4 review S5): an output another party can spend at once (a pool's successor: claims on one tier compete
// for its single pool, design 7.4; a ticket's successor: anyone may claim its other index) is never the only
// evidence; the payout output and the change are watched too.
import type { ApiObject, BuiltTx, Signed, Utxo } from '@wasm/web.js';
import type { TxStep } from '@/components/TxProgress';
import { PageError, unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { refusedLately, settleSubmit, signAndSubmit } from '@/play/buyFlow';
import type { WalletAdapter } from '@/wallet/types';

/** How often, and how far apart, `unrefused` builds again. */
export const UNREFUSED = { tries: 5, delayMs: 2_000 };

/**
 * Build with `make` until its transaction ID is not one the node has just refused (F6 part A review N1): a build
 * made right after a conflict, from a node whose UTXO index has not caught up yet, comes out the very transaction
 * the node refused; it is built again after a pause, never offered or sent. Still the same after the tries:
 * node_conflict (the next read builds again).
 */
export async function unrefused<T extends { built: BuiltTx }>(
  make: () => Promise<T>,
  sleep: (ms: number) => Promise<void> = (ms) => new Promise<void>((r) => setTimeout(r, ms)),
): Promise<T> {
  for (let i = 1; ; i++) {
    const p = await make();
    if (!refusedLately(p.built.id)) return p;
    if (i >= UNREFUSED.tries) {
      throw new PageError(
        'node_conflict',
        `transaction ${p.built.id}: refused a moment ago and built the same ${i} times; the node has not caught up`,
      );
    }
    await sleep(UNREFUSED.delayMs);
  }
}

/** One output of a built transaction, to look for on the node. */
export interface Watched {
  index: number;
  amount: string;
  scriptPublicKey: string;
  /** For a covenant output: its state bytes (checked with verifyState once live). */
  state?: string;
  role: ApiObject['role'];
}

export interface SendEnv {
  ctx: Ctx;
  wallet: WalletAdapter;
  submit?: (s: Signed) => Promise<string>;
  sleep?: (ms: number) => Promise<void>;
  /** How long to wait for the output, and the pause between looks. */
  acceptMs?: number;
  pollMs?: number;
  settleTries?: number;
  settleDelayMs?: number;
}

export type SendEvent = { kind: 'step'; step: TxStep } | { kind: 'checking'; on: boolean };

function objectOf(id: string, w: Watched): ApiObject {
  return {
    role: w.role,
    transactionId: id,
    index: w.index,
    amount: w.amount,
    blockDaaScore: '0',
    scriptPublicKey: w.scriptPublicKey,
    state: w.state ?? '',
  };
}

/** The output's entry when the node holds it (a covenant output's state checked against it). */
export async function liveOutput(ctx: Ctx, id: string, w: Watched): Promise<Utxo | undefined> {
  const u = unwrap(await ctx.lottery.call('nodeEntries', [objectOf(id, w)])).find((x) => x.index === w.index);
  if (!u) return undefined;
  if (w.state !== undefined) {
    unwrap(await ctx.lottery.call('verifyState', ctx.config.covenantId, w.role, { utxo: u, state: w.state }));
  }
  return u;
}

function unknown(e: unknown): boolean {
  return e instanceof PageError && (e.code === 'node_unavailable' || e.code === 'worker_stopped');
}

/** The first of `watch` (in order) that the node holds, with its entry. */
export async function firstLive(ctx: Ctx, id: string, watch: Watched[]): Promise<{ w: Watched; u: Utxo } | undefined> {
  for (const w of watch) {
    const u = await liveOutput(ctx, id, w).catch((e: unknown) => {
      if (unknown(e)) return undefined;
      throw e;
    });
    if (u) return { w, u };
  }
  return undefined;
}

/**
 * Sign, check, send `built`, then wait until one of `watch` (its own outputs) is live on the node. A lost
 * answer, or outputs that take too long, are settled from the chain (`settleSubmit`: the same signed
 * transaction again); no answer at all is submit_unknown, never a second, different transaction. `onSigned`
 * keeps the signed transaction before it is sent (a pending record, settled on a later visit).
 */
export async function sendAndWait(
  env: SendEnv,
  built: BuiltTx,
  watch: Watched[],
  onEvent: (e: SendEvent) => void,
  onSigned?: (final: Signed) => Promise<void>,
): Promise<{ id: string; entry: Utxo; watched: Watched }> {
  const { ctx } = env;
  const sleep = env.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  const submit = env.submit ?? (async (s: Signed) => unwrap(await ctx.lottery.call('nodeSubmit', s)));
  const landed = async () => (await firstLive(ctx, built.id, watch)) !== undefined;
  const onChecking = (on: boolean) => onEvent({ kind: 'checking', on });
  const settle = {
    landed,
    onChecking,
    sleep: env.sleep,
    tries: env.settleTries ?? 20,
    delayMs: env.settleDelayMs ?? 3_000,
  };
  onEvent({ kind: 'step', step: 'built' });
  const { id, final } = await signAndSubmit(
    ctx.lottery,
    env.wallet,
    built,
    submit,
    (step) => onEvent({ kind: 'step', step }),
    undefined,
    settle,
    onSigned,
  );
  const acceptMs = env.acceptMs ?? 120_000;
  for (let w = 0; w < 5; w++) {
    const until = Date.now() + acceptMs;
    while (Date.now() < until) {
      const hit = await firstLive(ctx, id, watch);
      if (hit) {
        onEvent({ kind: 'step', step: 'accepted' });
        return { id, entry: hit.u, watched: hit.w };
      }
      await sleep(env.pollMs ?? 1000);
    }
    await settleSubmit(final, submit, settle);
  }
  throw new PageError('submit_unknown', `transaction ${id}: its output is not in the node's UTXO set yet`);
}
