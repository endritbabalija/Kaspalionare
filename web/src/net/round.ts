// A round as the page may show it (spec 6.3): the indexer's copy, each object checked against the
// node's live entry for its outpoint (never the indexer's entry, F1 re-review N1) by checkIndexerRound.
// The figures shown come from the decoded chain states (views) or from the wasm (jackpotOdds).
import type { CheckedInstance, CheckedRound, ControllerView, NodeInfo, RoundStatus } from '@wasm/web.js';
import type { Config } from '@/config';
import { PageError, unwrap } from '@/lib/errors';
import type { LotteryClient } from '@/worker/client';
import type { Indexer } from './indexer';

export interface Ctx {
  config: Config;
  lottery: LotteryClient;
  indexer: Indexer;
}

export async function checkInstance(ctx: Ctx): Promise<CheckedInstance> {
  const bundle = await ctx.indexer.instanceText();
  return unwrap(
    await ctx.lottery.call('checkIndexerInstance', {
      bundle,
      covenantId: ctx.config.covenantId,
      bundleHash: ctx.config.bundleHash,
    }),
  );
}

export async function checkedRound(ctx: Ctx, r: string): Promise<CheckedRound> {
  const round = await ctx.indexer.round(r);
  const utxos = unwrap(await ctx.lottery.call('nodeEntries', round.objects));
  return unwrap(await ctx.lottery.call('checkIndexerRound', { covenantId: ctx.config.covenantId, round, utxos }));
}

/** The S1 round card's figures. */
export interface RoundCardData {
  round: string;
  status: RoundStatus;
  /** Whether a live chain object showed the status (else it is the indexer's word). */
  statusChecked: boolean;
  m: number;
  /** J, sompi (the round's controller state). */
  jackpot: string;
  /** T, the sales deadline (DAA score; the controller state). */
  deadline: string;
  /** C = N x m (the wasm). */
  odds: string;
  /** The node's virtual DAA score and when the page read it (the clock for Time and the countdown). */
  virtualDaaScore: string;
  observedAt: number;
  node: NodeInfo;
}

function controllerOf(c: CheckedRound): ControllerView | undefined {
  for (const o of c.objects) {
    if (o.view.role === 'controller' && o.view.round === c.round) return o.view;
  }
  return undefined;
}

/** The current round (the newest the index knows), checked against the node, as the round card shows it. */
export async function loadRoundCard(ctx: Ctx, now: () => number = Date.now): Promise<RoundCardData> {
  const status = await ctx.indexer.status();
  if (status.covenantId.toLowerCase() !== ctx.config.covenantId) {
    throw new PageError('chain_mismatch', `the indexer serves instance ${status.covenantId}`);
  }
  if (!status.currentRound) throw new PageError('indexer_unavailable', 'the index has no round yet');
  const checked = await checkedRound(ctx, status.currentRound);
  const ctrl = controllerOf(checked);
  if (!ctrl || checked.m === undefined) {
    throw new PageError('chain_mismatch', `round ${checked.round}: no live controller of the round on the node`);
  }
  const m = Number(ctrl.m);
  const odds = unwrap(await ctx.lottery.call('jackpotOdds', m));
  const node = unwrap(await ctx.lottery.call('nodeInfo'));
  return {
    round: checked.round,
    status: checked.status,
    statusChecked: checked.statusChecked,
    m,
    jackpot: ctrl.jackpot,
    deadline: ctrl.deadline,
    odds,
    virtualDaaScore: node.virtualDaaScore,
    observedAt: now(),
    node,
  };
}
