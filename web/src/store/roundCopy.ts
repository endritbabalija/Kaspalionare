// The round's copy (spec 6.3): after a purchase, the compact purchase lists of all the round's shards from
// the indexer API, checked against the chain the way F2's checks do (every object against the node's entry
// for its outpoint; each list against its live shard's root while the round is open), then kept in the local
// store. Nothing the API says is kept unchecked. Right after a purchase the API may not have seen it yet (its
// shard's old outpoint is then spent on the node, a chain_mismatch), so the copy retries.
import type { RoundCopyStage, StoredRound } from '@wasm/web.js';
import { PageError, unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { roundKey, type WebStore } from './webstore';

export async function copyRound(
  ctx: Ctx,
  store: WebStore,
  round: string,
  now: () => number = Date.now,
): Promise<StoredRound> {
  const [r, purchases] = await Promise.all([ctx.indexer.round(round), ctx.indexer.purchases(round)]);
  const utxos = unwrap(await ctx.lottery.call('nodeEntries', r.objects));
  const checked = unwrap(
    await ctx.lottery.call('checkIndexerRound', { covenantId: ctx.config.covenantId, round: r, utxos, purchases }),
  );
  if (!checked.purchases || checked.m === undefined) {
    throw new PageError('store_incomplete', `round ${round}: no chain object to check the purchases against`);
  }
  const stage: RoundCopyStage = checked.status === 'open' ? 'open' : checked.status === 'drawn' ? 'drawn' : 'closed';
  const rec: StoredRound = {
    key: roundKey(ctx.config.covenantId, checked.round),
    covenantId: ctx.config.covenantId,
    network: ctx.config.network,
    round: checked.round,
    m: checked.m,
    k: checked.k,
    stage,
    purchases: checked.purchases,
    roots: checked.roots?.roots,
    checkedMs: String(now()),
  };
  await store.putRound(rec);
  return rec;
}

/** `copyRound`, retried while the API catches up with the chain (each failure is logged, the last thrown). */
export async function copyRoundSoon(
  ctx: Ctx,
  store: WebStore,
  round: string,
  opts: { tries?: number; delayMs?: number; sleep?: (ms: number) => Promise<void> } = {},
): Promise<StoredRound> {
  const tries = opts.tries ?? 20;
  const sleep = opts.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  let last: unknown;
  for (let i = 0; i < tries; i++) {
    try {
      return await copyRound(ctx, store, round);
    } catch (e) {
      last = e;
      if (e instanceof PageError) console.info(`round copy ${round}: try ${i + 1}: ${e.code}: ${e.detail}`);
      await sleep(opts.delayMs ?? 3000);
    }
  }
  throw last;
}
