// S5's three actions (spec 3 S5, 6.2, 7; J5; F6 part A): a deposit request, a withdrawal request and a
// settlement, each built in the wasm from the live, node-checked vault, position, request and record
// (`buildRequestDeposit`, `buildRequestWithdraw`, `buildSettle`), its rows from `depositFigures`,
// `withdrawFigures` or `settleFigures` (bound to the built transaction), then the purchase's path: the wallet
// signs the listed inputs only (one wallet request at a time), `assemble` checks what came back, only then the
// node gets it; a lost answer is settled from the chain (the same signed transaction again). "Accepted" is one of
// the transaction's own outputs live on the node (the request, the payout, the change). Deposits are made for the
// wallet's own address only (a Schnorr P2PK owner the page can later withdraw for, `owner_type`). A withdrawal's
// shares come from `withdrawChoice` (Rust: the whole position, or w >= wmin leaving >= wmin), its owner input is a
// coin of the wallet at the position owner's address.
import type {
  BuiltTx,
  CovOut,
  DepositFigures,
  DepositInput,
  PositionCard,
  RequestCard,
  SettleFigures,
  SettleInput,
  Utxo,
  WithdrawChoice,
  WithdrawFigures,
  WithdrawInput,
} from '@wasm/web.js';
import { PageError, unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { coinsFor } from '@/play/buyFlow';
import { sendAndWait, type SendEnv, type SendEvent, type Watched } from '@/tickets/send';
import type { WalletAdapter } from '@/wallet/types';
import type { VaultData } from './load';

export interface VaultEnv {
  ctx: Ctx;
  wallet: WalletAdapter;
  /** The wallet's address: the owner of its requests, it pays the fee and gets the change. */
  address: string;
}

export type Prepared =
  | { kind: 'deposit'; built: BuiltTx; figures: DepositFigures }
  | { kind: 'withdraw'; built: BuiltTx; figures: WithdrawFigures; choice: WithdrawChoice }
  | { kind: 'settle'; built: BuiltTx; figures: SettleFigures };

async function signer(env: VaultEnv): Promise<void> {
  if ((await env.wallet.address()) !== env.address)
    throw new PageError('wallet_changed', 'the wallet signs for another address');
}

async function wallet(env: VaultEnv): Promise<{ coins: Utxo[]; feeRate: number; max: number }> {
  const [coins, rate, shape] = await Promise.all([
    env.wallet.utxos(),
    env.ctx.lottery.call('nodeFeeRate'),
    env.ctx.lottery.call('gameShape'),
  ]);
  return { coins, feeRate: unwrap(rate), max: shape.coinsMax };
}

export async function prepareDeposit(env: VaultEnv, d: VaultData, amount: string): Promise<Prepared> {
  await signer(env);
  const { ctx } = env;
  const w = await wallet(env);
  const input: DepositInput = {
    covenantId: ctx.config.covenantId,
    vault: d.vault,
    amount,
    owner: env.address,
    funds: { coins: coinsFor(w.coins, w.max), change: env.address, feeRate: w.feeRate },
  };
  const built = unwrap(await ctx.lottery.call('buildRequestDeposit', input));
  const figures = unwrap(
    await ctx.lottery.call('depositFigures', {
      covenantId: input.covenantId,
      vault: input.vault,
      amount,
      owner: input.owner,
      built,
    }),
  );
  return { kind: 'deposit', built, figures };
}

/** The shares a withdrawal takes: the whole position (`amount` null), or those worth at most `amount` sompi today. */
export async function withdrawChoice(ctx: Ctx, d: VaultData, p: PositionCard, amount: string | null) {
  return unwrap(
    await ctx.lottery.call('withdrawChoice', {
      covenantId: ctx.config.covenantId,
      vault: d.vault,
      position: p.object,
      amount: amount ?? undefined,
    }),
  );
}

export async function prepareWithdraw(
  env: VaultEnv,
  d: VaultData,
  p: PositionCard,
  amount: string | null,
): Promise<Prepared> {
  await signer(env);
  const { ctx } = env;
  if (!p.ownerKey) throw new PageError('owner_type', 'the position is not of a Schnorr P2PK owner');
  if (p.owner !== env.address) throw new PageError('wallet_script', 'the position is not this wallet’s');
  const choice = await withdrawChoice(ctx, d, p, amount);
  const w = await wallet(env);
  const own = coinsFor(w.coins, w.max);
  const [coin, ...rest] = own;
  if (!coin) throw new PageError('funding_short', 'the wallet has no coin for the owner input');
  const input: WithdrawInput = {
    covenantId: ctx.config.covenantId,
    vault: d.vault,
    position: p.object,
    shares: choice.shares,
    owner: { coin, kind: 'schnorr', publicKey: p.ownerKey },
    funds: { coins: rest.slice(0, Math.max(0, w.max - 1)), change: env.address, feeRate: w.feeRate },
  };
  const built = unwrap(await ctx.lottery.call('buildRequestWithdraw', input));
  const figures = unwrap(
    await ctx.lottery.call('withdrawFigures', {
      covenantId: input.covenantId,
      vault: input.vault,
      position: input.position,
      shares: input.shares,
      built,
    }),
  );
  return { kind: 'withdraw', built, figures, choice };
}

export async function prepareSettle(env: VaultEnv, r: RequestCard): Promise<Prepared> {
  await signer(env);
  const { ctx } = env;
  if (!r.record || r.status !== 'settleDue') throw new PageError('store_incomplete', 'the request has no live record');
  const w = await wallet(env);
  const input: SettleInput = {
    covenantId: ctx.config.covenantId,
    request: r.object,
    record: r.record,
    funds: { coins: coinsFor(w.coins, w.max), change: env.address, feeRate: w.feeRate },
  };
  const built = unwrap(await ctx.lottery.call('buildSettle', input));
  const figures = unwrap(
    await ctx.lottery.call('settleFigures', {
      covenantId: input.covenantId,
      request: input.request,
      record: input.record,
      built,
    }),
  );
  return { kind: 'settle', built, figures };
}

function covWatch(o: CovOut | null | undefined, role: Watched['role']): Watched[] {
  return o ? [{ index: o.index, amount: o.value, scriptPublicKey: o.scriptPublicKey, state: o.state, role }] : [];
}

/** The plain outputs of `built` paying the wallet (a payout, a reserve back, the change). */
async function walletOutputs(ctx: Ctx, built: BuiltTx): Promise<Watched[]> {
  const out: Watched[] = [];
  for (const o of built.summary.outputs) {
    if (!o.toWallet || o.covenant || !o.address) continue;
    out.push({
      index: o.index,
      amount: o.value,
      scriptPublicKey: unwrap(await ctx.lottery.call('scriptOf', o.address)),
      role: 'ticket',
    });
  }
  return out;
}

/**
 * What shows the transaction went through: its own outputs (F4 review S5: never only one another party can spend at
 * once; anyone may settle a request, and the next request or draw spends the vault).
 */
async function watched(ctx: Ctx, p: Prepared): Promise<Watched[]> {
  const plain = await walletOutputs(ctx, p.built);
  switch (p.kind) {
    case 'deposit':
      return [...covWatch(p.figures.request, 'request'), ...plain, ...covWatch(p.figures.vaultNext, 'vault')];
    case 'withdraw':
      return [
        ...covWatch(p.figures.request, 'request'),
        ...covWatch(p.figures.positionNext, 'position'),
        ...plain,
        ...covWatch(p.figures.vaultNext, 'vault'),
      ];
    case 'settle':
      return [
        ...plain,
        ...covWatch(p.figures.positionNext, 'position'),
        ...covWatch(p.figures.requestNext, 'request'),
        ...covWatch(p.figures.recordNext, 'record'),
      ];
  }
}

/** Sign, check and send a prepared action; accepted once one of its own outputs is live on the node. */
export async function sendVault(
  env: VaultEnv & Omit<SendEnv, 'ctx' | 'wallet'>,
  p: Prepared,
  onEvent: (e: SendEvent) => void,
): Promise<string> {
  const watch = await watched(env.ctx, p);
  if (!watch.length) throw new PageError('built_mismatch', 'the transaction leaves nothing to look for on the node');
  const { id } = await sendAndWait({ ...env }, p.built, watch, onEvent);
  return id;
}
