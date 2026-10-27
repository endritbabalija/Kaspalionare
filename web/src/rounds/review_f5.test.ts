// @vitest-environment node
// F5 review regressions (`covenant-reviewer` on F5), through the page's own code and the real dev wasm:
// - S2: an indexer answer for another round than the one asked for is refused (`chain_mismatch`), in the client
//   (rounds, purchases, anchor proof) and so in the list row and the detail (the reviewer's probe: round N's
//   honest, chain-checked objects served under round 999).
// - S5: a void is built without the indexer's purchases (the purchases endpoint down).
// - S1: a draw or void absorbs the expired pools the indexer lists, each read from the node; the indexer's list
//   decides nothing (a pool the node does not hold is left out), and without the list it absorbs none.
import { beforeAll, describe, expect, it } from 'vitest';
import type {
  AnchorBlock,
  ApiObject,
  ApiRound,
  ApiRoundRow,
  CloseInput,
  CovUtxo,
  DrawInput,
  QuickKey,
  Utxo,
} from '@wasm/web.js';
import { PageError, unwrap } from '@/lib/errors';
import { Indexer } from '@/net/indexer';
import { MemoryKV } from '@/store/kv';
import fixtures from '@/test/fixtures/page-inputs.json';
import { ctxOf, lottery, Sim } from '@/test/ticketWorld';
import { QuickKeyStore, QuickWalletAdapter } from '@/wallet/quick';
import { dueOf, prepareCaller } from './caller';
import { checkRow, loadDetail, type DetailData } from './load';

const fx = fixtures as unknown as Record<string, CloseInput & DrawInput>;
const close = fx['close k=16'] as CloseInput;
const voidIn = fx['void absorbing 2'] as DrawInput & { absorbed: CovUtxo[] };
const drawIn = fx['draw absorbing 3'] as DrawInput & { absorbed: CovUtxo[] };
const drawBlock = (fixtures as unknown as Record<string, AnchorBlock>)['draw absorbing 3 anchor block'] as AnchorBlock;

function apiObject(role: ApiObject['role'], c: CovUtxo): ApiObject {
  const u = c.utxo;
  return {
    role,
    transactionId: u.transactionId,
    index: u.index,
    amount: u.amount,
    blockDaaScore: u.blockDaaScore,
    scriptPublicKey: u.scriptPublicKey,
    state: c.state,
  };
}

/** A fetch answering `body(path)` with 200, or 404 for null. */
function fakeFetch(body: (path: string) => string | null): typeof fetch {
  return ((input: string | URL | Request) => {
    const path = new URL(typeof input === 'string' ? input : input instanceof URL ? input.href : input.url).pathname;
    const b = body(path);
    return Promise.resolve(
      new Response(b ?? '{"error":{"code":"round_unknown","message":"x","detail":"x"}}', {
        status: b === null ? 404 : 200,
      }),
    );
  }) as typeof fetch;
}

/** The fixture `close k=16` round (open) as an indexer serves it, on a stand-in node. */
async function openRound() {
  const sim = new Sim(close.covenantId);
  sim.vdaa = close.virtualDaaScore;
  const objs: [ApiObject['role'], CovUtxo][] = [
    ['controller', close.controller],
    ['params', close.params],
    ...close.shards.map((s) => ['shard', s] as [ApiObject['role'], CovUtxo]),
  ];
  for (const [, c] of objs) sim.add(c.utxo);
  const cv = unwrap(await lottery.call('decodeState', 'controller', close.controller.state));
  if (cv.role !== 'controller') throw new Error('not a controller');
  const api: ApiRound = {
    round: cv.round,
    status: 'open',
    m: Number(cv.m),
    k: Number(cv.k),
    deadline: cv.deadline,
    anchorDaaScore: undefined,
    close: undefined,
    draw: undefined,
    buys: 0,
    combinations: 0,
    objects: objs.map(([r, c]) => apiObject(r, c)),
    incomplete: undefined,
    gaps: [],
  };
  return { sim, api, round: cv.round };
}

describe('review F5 S2: the round number of an indexer answer', () => {
  it('the client refuses an answer for another round (rounds, purchases, anchor proof); the asked one passes', async () => {
    const { sim, api, round } = await openRound();
    const ctx = ctxOf(sim);
    const purchases = { round, m: api.m, k: api.k, shards: Array.from({ length: api.k }, () => '') };
    const proof = { round, anchorProof: '00'.repeat(272), source: 'draw' };
    const body = (p: string) => {
      if (p.endsWith('/purchases')) return JSON.stringify(purchases);
      if (p.endsWith('/anchor-proof')) return JSON.stringify(proof);
      if (p.includes('/rounds/')) return JSON.stringify(api);
      return null;
    };
    const ix = new Indexer('http://indexer.test/v1/', ctx.lottery, fakeFetch(body));
    expect((await ix.round(round)).round).toBe(round);
    expect((await ix.purchases(round)).round).toBe(round);
    expect((await ix.anchorProof(round))?.round).toBe(round);
    for (const call of [() => ix.round('999'), () => ix.purchases('999'), () => ix.anchorProof('999')]) {
      const e = await call().then(
        () => null,
        (x: unknown) => x,
      );
      expect(e).toBeInstanceOf(PageError);
      expect((e as PageError).code).toBe('chain_mismatch');
    }
    // A round number the page would never ask for (not a plain decimal) is refused too.
    await expect(ix.round('03')).rejects.toMatchObject({ code: 'chain_mismatch' });
  }, 120_000);

  it("the reviewer's probe: round N's checked objects under round 999 show as an error, not as round 999", async () => {
    const { sim, api } = await openRound();
    const base = ctxOf(sim);
    const ix = new Indexer(
      'http://indexer.test/v1/',
      base.lottery,
      fakeFetch((p) => (p.includes('/rounds/') ? JSON.stringify(api) : null)),
    );
    const ctx = { ...base, indexer: ix };
    const row = {
      round: '999',
      status: 'open',
      m: api.m,
      k: api.k,
      buys: 0,
      combinations: 0,
      sales: '1',
      jackpot: '1',
    };
    const shown = await checkRow(ctx, row as ApiRoundRow);
    expect(shown.status).toBeNull();
    expect(shown.code).toBe('chain_mismatch');
    expect(shown.sales).toBeUndefined();
    await expect(loadDetail(ctx, '999')).rejects.toMatchObject({ code: 'chain_mismatch' });
  }, 120_000);
});

// ------------------------------------------------------------------------------ S1, S5: the void of round 2

let key: QuickKey;
let quick: QuickWalletAdapter;
let coins: Utxo[];

beforeAll(async () => {
  key = unwrap(await lottery.call('quickWalletNew'));
  const spk = unwrap(await lottery.call('scriptOf', key.address));
  const ks = new QuickKeyStore(new MemoryKV(), 'testnet-10');
  await ks.replace(null, key);
  coins = voidIn.funds.coins.map((c) => ({ ...c, scriptPublicKey: spk }));
  quick = new QuickWalletAdapter(lottery, ks, 'testnet-10', () => Promise.resolve(coins), key);
}, 120_000);

/** The fixture call's build with this wallet's coins and change, at the stand-in node's fee rate (100). */
async function want(i: DrawInput, absorbed: CovUtxo[]) {
  return unwrap(
    await lottery.call('buildDraw', {
      ...i,
      absorbed,
      purchases: i.anchorProof ? i.purchases : [],
      funds: { coins, change: key.address, feeRate: 100 },
    }),
  );
}

/**
 * The fixture `void absorbing 2` (round 2 closing, past its void lock) as an indexer serves it, on a stand-in node
 * that also holds the two expired pools of round 1; `pools` is what the indexer's `GET /v1/pools` lists.
 */
async function closingRound(
  pools: CovUtxo[] | null,
  onNode: CovUtxo[] = voidIn.absorbed,
  i: DrawInput = voidIn,
  vdaa = '3000000',
) {
  const sim = new Sim(i.covenantId);
  sim.vdaa = vdaa;
  const objs: [ApiObject['role'], CovUtxo][] = [
    ['marker', i.marker],
    ['controller', i.controller],
    ['drawer', i.drawer],
    ['vault', i.vault],
  ];
  for (const [, c] of objs) sim.add(c.utxo);
  for (const p of onNode) sim.add(p.utxo);
  const mv = unwrap(await lottery.call('decodeState', 'marker', i.marker.state));
  if (mv.role !== 'marker') throw new Error('not a marker');
  const api: ApiRound = {
    round: mv.round,
    status: 'closing',
    m: Number(mv.m),
    k: Number(mv.k),
    deadline: undefined,
    anchorDaaScore: undefined,
    close: undefined,
    draw: undefined,
    buys: 0,
    combinations: 0,
    objects: objs.map(([r, c]) => apiObject(r, c)),
    incomplete: undefined,
    gaps: [],
  };
  const purchasesAsked: string[] = [];
  const utxos = objs.map(([, c]) => c.utxo);
  const first = unwrap(await lottery.call('checkIndexerRound', { covenantId: i.covenantId, round: api, utxos }));
  api.anchorDaaScore = first.facts.d?.value;
  const ctx = ctxOf(
    sim,
    {
      purchases: (r: unknown) => {
        purchasesAsked.push(String(r));
        if (!i.anchorProof) return Promise.reject(new PageError('indexer_unavailable', 'purchases down (test)'));
        return Promise.resolve({ round: mv.round, m: api.m, k: api.k, shards: i.purchases });
      },
      anchorProof: (r: unknown) =>
        Promise.resolve(i.anchorProof ? { round: String(r), anchorProof: i.anchorProof, source: 'node' } : null),
      ...(pools
        ? { pools: () => Promise.resolve({ owner: undefined, objects: pools.map((p) => apiObject('pool', p)) }) }
        : {}),
    },
    { nodeAnchorBlock: () => ({ ok: drawBlock }) },
  );
  const checked = unwrap(await lottery.call('checkIndexerRound', { covenantId: i.covenantId, round: api, utxos }));
  const node = unwrap(await ctx.lottery.call('nodeInfo'));
  const d: DetailData = {
    api,
    checked,
    status: checked.status,
    node,
    clock: { virtualDaa: node.virtualDaaScore, observedAt: 0 },
  };
  return { sim, ctx, d, purchasesAsked };
}

const outpoints = (cs: { transactionId: string; index: number }[]) => cs.map((c) => `${c.transactionId}:${c.index}`);

describe('review F5 S1, S5: the void absorbs the expired pools, without the purchases', () => {
  it('is due after the void lock and builds with the purchases endpoint down, absorbing both pools', async () => {
    const w = await closingRound(voidIn.absorbed);
    expect(dueOf(w.d)).toMatchObject({ kind: 'due', action: 'void' });
    const p = await prepareCaller({ ctx: w.ctx, wallet: quick, address: key.address }, w.d, 'void');
    expect(w.purchasesAsked).toEqual([]);
    expect(p.figures.kind).toBe('void');
    expect(p.figures.absorbed).toBe(2);
    expect(p.figures.reserves).toBe(0);
    const tx = JSON.parse(p.built.transaction) as { inputs: { transactionId: string; index: number }[] };
    expect(outpoints(tx.inputs.slice(4, 6))).toEqual(outpoints(voidIn.absorbed.map((a) => a.utxo)));
    // Byte for byte the fixture case's void (crates/web review_f5: = the library = the native builder's) with the
    // same absorbed set, built without purchases.
    expect(p.built.transaction).toBe((await want(voidIn, voidIn.absorbed)).transaction);
  }, 120_000);

  it('a draw absorbs the three expired pools: byte for byte the fixture draw with that absorbed set', async () => {
    const w = await closingRound(drawIn.absorbed, drawIn.absorbed, drawIn, '2001000');
    const p = await prepareCaller({ ctx: w.ctx, wallet: quick, address: key.address }, w.d, 'draw');
    expect(w.purchasesAsked).toEqual([w.d.checked.round]);
    expect(p.figures.kind).toBe('draw');
    expect(p.figures.absorbed).toBe(3);
    expect(p.figures.pools).toBeGreaterThanOrEqual(2);
    expect(p.figures.reserveBackFrom).toBe(w.d.checked.facts.claimDeadline?.value);
    const tx = JSON.parse(p.built.transaction) as { inputs: { transactionId: string; index: number }[] };
    expect(outpoints(tx.inputs.slice(4, 7))).toEqual(outpoints(drawIn.absorbed.map((a) => a.utxo)));
    expect(p.built.transaction).toBe((await want(drawIn, drawIn.absorbed)).transaction);
  }, 120_000);

  it('absorbs none without the list, and leaves out a listed pool the node does not hold', async () => {
    const none = await closingRound(null);
    const p0 = await prepareCaller({ ctx: none.ctx, wallet: quick, address: key.address }, none.d, 'void');
    expect(p0.figures.absorbed).toBe(0);
    const [a0, a1] = voidIn.absorbed as [CovUtxo, CovUtxo];
    const one = await closingRound([a0, a1], [a1]);
    const p1 = await prepareCaller({ ctx: one.ctx, wallet: quick, address: key.address }, one.d, 'void');
    expect(p1.figures.absorbed).toBe(1);
    const tx = JSON.parse(p1.built.transaction) as { inputs: { transactionId: string; index: number }[] };
    expect(outpoints(tx.inputs.slice(4, 5))).toEqual(outpoints([a1.utxo]));
  }, 120_000);
});
