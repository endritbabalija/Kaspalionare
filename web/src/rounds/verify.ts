// "Verify in this browser" (spec J6, S3; F5): every check runs in the worker (`verifyRound`, Rust), each with its
// own result (match, mismatch, not checkable): each object the indexer names against its live UTXO on a public
// node, the status those objects show, the indexer's frozen roots, each shard's tree rebuilt from the indexer's
// purchases against the chain's root, the winning numbers recomputed from the anchor proof against the record
// (the pools), and X.hash against the block a public node reports for it (fetched here by X.hash, never from the
// indexer). The indexer's whole answer is passed, spent objects included: they show as not checkable.
import type { AnchorBlock, ApiAnchorProof, ApiPurchases, RoundVerify } from '@wasm/web.js';
import { unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { anchorBlockOf } from './load';
import { withIndexer } from '@/net/indexers';

/** One indexer for every item (F6 part E); a lie is reported item by item, only an unreachable one moves on. */
export function verifyRound(ctx: Ctx, r: string): Promise<RoundVerify> {
  return withIndexer(ctx, (c) => verifyRoundOn(c, r));
}

async function verifyRoundOn(ctx: Ctx, r: string): Promise<RoundVerify> {
  const round = await ctx.indexer.round(r);
  const utxos = round.objects.length ? unwrap(await ctx.lottery.call('nodeEntries', round.objects)) : [];
  let purchases: ApiPurchases | undefined;
  try {
    purchases = await ctx.indexer.purchases(r);
  } catch (e) {
    console.info(`verify round ${r}: no purchases (${String(e)})`);
  }
  let anchorProof: ApiAnchorProof | undefined;
  try {
    anchorProof = (await ctx.indexer.anchorProof(r)) ?? undefined;
  } catch (e) {
    console.info(`verify round ${r}: no anchor proof (${String(e)})`);
  }
  let anchorBlock: AnchorBlock | undefined;
  if (anchorProof) {
    // A proof whose X the wasm cannot even read is the numbers' mismatch (verifyRound reports it).
    anchorBlock = await anchorBlockOf(ctx, anchorProof).catch(() => undefined);
  }
  return unwrap(
    await ctx.lottery.call('verifyRound', {
      covenantId: ctx.config.covenantId,
      round,
      utxos,
      purchases,
      anchorProof,
      anchorBlock,
    }),
  );
}
