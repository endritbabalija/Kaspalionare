// S5 Vault (spec 3 S5, 6.3; J5; F6 part A): nothing the indexer says is shown as checked. The vault, the
// wallet's positions and requests and the records they settle against are read from the indexer
// (`GET /v1/vault`, `/v1/positions?owner=`, `/v1/requests?owner=`), every object is read from a public node by
// outpoint (spent ones, which an indexer a little behind may still list, are left out), checked by
// `checkIndexerObjects` (its state against the node's entry), and only then given to `vaultView` (Rust), which
// computes every figure and leaves out any position or request the wallet does not own.
import type { ApiObject, CheckedObject, CovUtxo, Utxo, VaultPage } from '@wasm/web.js';
import { PageError, unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import type { Clock } from '@/rounds/load';
import { nodeEntries } from '@/tickets/load';

export interface VaultData {
  /** Every figure of the page (`vaultView`). */
  page: VaultPage;
  /** The live vault the figures and every build are made from. */
  vault: CovUtxo;
  /** The wallet the positions and requests are shown for. */
  owner: string | null;
  clock: Clock;
}

function sameOutpoint(u: Utxo, o: ApiObject): boolean {
  return u.transactionId.toLowerCase() === o.transactionId.toLowerCase() && u.index === o.index;
}

/** The objects the node holds (each once), checked against its entries. */
async function liveChecked(ctx: Ctx, objects: ApiObject[]): Promise<CheckedObject[]> {
  const once = objects.filter(
    (o, i) =>
      objects.findIndex(
        (x) => x.transactionId.toLowerCase() === o.transactionId.toLowerCase() && x.index === o.index,
      ) === i,
  );
  if (!once.length) return [];
  const utxos = await nodeEntries(ctx, once);
  const live = once.filter((o) => utxos.some((u) => sameOutpoint(u, o)));
  if (!live.length) return [];
  return unwrap(
    await ctx.lottery.call('checkIndexerObjects', { covenantId: ctx.config.covenantId, objects: live, utxos }),
  );
}

const of = (objs: ApiObject[], role: ApiObject['role']) => objs.filter((o) => o.role === role);

export async function loadVault(ctx: Ctx, owner: string | null, now: () => number = Date.now): Promise<VaultData> {
  const [v, positions, requests] = await Promise.all([
    ctx.indexer.vault(),
    owner ? ctx.indexer.positions(owner) : Promise.resolve(null),
    owner ? ctx.indexer.requests(owner) : Promise.resolve(null),
  ]);
  const vaults = await liveChecked(ctx, of(v.objects, 'vault'));
  const vault = vaults[0];
  if (vaults.length !== 1 || !vault) {
    throw new PageError('chain_mismatch', `${vaults.length} live vaults of the indexer's on the node`);
  }
  const mine = await liveChecked(ctx, [
    ...of(positions?.objects ?? [], 'position'),
    ...of(requests?.objects ?? [], 'request'),
    ...of(requests?.objects ?? [], 'record'),
  ]);
  const objects = (role: CheckedObject['role']) => mine.filter((o) => o.role === role).map((o) => o.object);
  const node = unwrap(await ctx.lottery.call('nodeInfo'));
  const page = unwrap(
    await ctx.lottery.call('vaultView', {
      covenantId: ctx.config.covenantId,
      vault: vault.object,
      positions: objects('position'),
      requests: objects('request'),
      records: objects('record'),
      owner: owner ?? undefined,
      virtualDaaScore: node.virtualDaaScore,
    }),
  );
  if (page.foreign) console.info(`vault: ${page.foreign} positions or requests of another owner left out`);
  return { page, vault: vault.object, owner, clock: { virtualDaa: node.virtualDaaScore, observedAt: now() } };
}
