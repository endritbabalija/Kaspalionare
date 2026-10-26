// @vitest-environment node
// F5 (spec S4, 6.2, 7; J4): Run the round through the page's own code (rounds/*) and the real wasm (src/wasm/dev)
// behind the worker's message path, on the stand-in node of the F4 tests (`Sim`). The round is crates/web's
// fixture `close k=16` (src/test/fixtures/page-inputs.json), served as an indexer's open round and checked by
// `checkIndexerRound`; the caller is a fresh quick wallet. Covered: when close() is due (T passed, the previous
// round drawn), the rows bound to the built transaction (`callerFigures`), the press signing exactly that build,
// the post-sign check refusing a wallet that changed the caller output before anything is sent, a lost answer
// settled from the chain (the same transaction again), and no answer at all ending in submit_unknown.
import { beforeAll, describe, expect, it } from 'vitest';
import type { ApiObject, ApiRound, CloseInput, CovUtxo, QuickKey, Utxo } from '@wasm/web.js';
import { unwrap } from '@/lib/errors';
import { MemoryKV } from '@/store/kv';
import fixtures from '@/test/fixtures/page-inputs.json';
import { ctxOf, fast, lottery, need, Sim } from '@/test/ticketWorld';
import { QuickKeyStore, QuickWalletAdapter } from '@/wallet/quick';
import type { WalletAdapter } from '@/wallet/types';
import { dueOf, prepareCaller, sendCaller } from './caller';
import type { DetailData } from './load';

const close = (fixtures as unknown as Record<string, CloseInput>)['close k=16'] as CloseInput;

let key: QuickKey;
let quick: QuickWalletAdapter;
let coins: Utxo[];

beforeAll(async () => {
  key = unwrap(await lottery.call('quickWalletNew'));
  const spk = unwrap(await lottery.call('scriptOf', key.address));
  const ks = new QuickKeyStore(new MemoryKV(), 'testnet-10');
  await ks.replace(null, key);
  coins = close.funds.coins.map((c) => ({ ...c, scriptPublicKey: spk }));
  quick = new QuickWalletAdapter(lottery, ks, 'testnet-10', () => Promise.resolve(coins), key);
}, 120_000);

function apiObject(role: ApiObject['role'], c: CovUtxo): ApiObject {
  return {
    role,
    transactionId: c.utxo.transactionId,
    index: c.utxo.index,
    amount: c.utxo.amount,
    blockDaaScore: c.utxo.blockDaaScore,
    scriptPublicKey: c.utxo.scriptPublicKey,
    state: c.state,
  };
}

/** The fixture's round as an indexer serves it, checked against the stand-in node; the clock at `vdaa`. */
async function detail(vdaa: string) {
  const sim = new Sim(close.covenantId);
  sim.vdaa = vdaa;
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
  const ctx = ctxOf(sim);
  const utxos = objs.map(([, c]) => c.utxo);
  const checked = unwrap(await lottery.call('checkIndexerRound', { covenantId: close.covenantId, round: api, utxos }));
  const node = unwrap(await ctx.lottery.call('nodeInfo'));
  const d: DetailData = {
    api,
    checked,
    status: checked.status,
    node,
    clock: { virtualDaa: node.virtualDaaScore, observedAt: 0 },
  };
  return { sim, ctx, d, deadline: cv.deadline };
}

describe('Run the round: close() (spec S4, J4)', () => {
  it('is due only after T with the previous round drawn; its rows are the built transaction', async () => {
    const { deadline } = await detail(close.virtualDaaScore);
    const before = await detail(deadline);
    expect(dueOf(before.d)).toEqual({ kind: 'later', action: 'close', from: deadline });
    const w = await detail(close.virtualDaaScore);
    expect(dueOf(w.d)).toEqual({ kind: 'due', action: 'close', from: deadline });
    expect(w.d.checked.facts.previousDrawn).toBe(true);
    const env = { ctx: w.ctx, wallet: quick, address: key.address };
    const p = await prepareCaller(env, w.d, 'close');
    const f = p.figures;
    expect(f.kind).toBe('close');
    expect(f.reward).toBe(need(w.d.checked.facts.reward).value);
    expect(f.reserve).toBe(need(w.d.checked.facts.reserve).value);
    expect(f.reserves).toBe(1);
    expect(f.reserveBackWithDraw).toBe(true);
    expect(f.fee).toBe(p.built.fee);
    expect(f.feeAboveReward).toBe(BigInt(f.fee) > BigInt(f.reward));
    // The rows add up: Reserve + Network fee - Reward - Refund = Total = what leaves the wallet.
    expect(BigInt(f.reserve) + BigInt(f.fee) - BigInt(f.reward) - BigInt(f.refund)).toBe(BigInt(f.total));
    expect(BigInt(p.built.summary.walletIn) - BigInt(p.built.summary.walletBack)).toBe(BigInt(f.total));
    // The press signs exactly this build; accepted once the caller output is live.
    const events: string[] = [];
    const id = await sendCaller({ ...env, ...fast }, p, (e) => events.push(e.kind === 'step' ? e.step : 'checking'));
    expect(id).toBe(p.built.id);
    expect(w.sim.submitted.map((s) => s.id)).toEqual([p.built.id]);
    expect(events).toEqual(['built', 'signed', 'sent', 'accepted']);
    expect(w.sim.live.has(`${p.built.id}:${f.callerOutput}`)).toBe(true);
  }, 120_000);

  it('a wallet that changes the caller output is refused before anything is sent (post-sign check)', async () => {
    const w = await detail(close.virtualDaaScore);
    const env = { ctx: w.ctx, wallet: quick, address: key.address };
    const p = await prepareCaller(env, w.d, 'close');
    const thief: WalletAdapter = {
      ...quick,
      kind: 'quick',
      connect: () => quick.connect(),
      network: () => quick.network(),
      address: () => quick.address(),
      utxos: () => quick.utxos(),
      signInputs: async (b, ix) => {
        const s = await quick.signInputs(b, ix);
        const tx = JSON.parse(s) as { outputs: { value: string }[] };
        const last = need(tx.outputs[tx.outputs.length - 1]);
        last.value = String(BigInt(last.value) - 1000n);
        return JSON.stringify(tx);
      },
    };
    await expect(sendCaller({ ...env, wallet: thief, ...fast }, p, () => undefined)).rejects.toMatchObject({
      code: 'signed_changed',
    });
    expect(w.sim.submitted).toEqual([]);
  }, 120_000);

  it('a lost answer is settled from the chain; no answer at all is submit_unknown, never a second build', async () => {
    const w = await detail(close.virtualDaaScore);
    const env = { ctx: w.ctx, wallet: quick, address: key.address };
    const p = await prepareCaller(env, w.d, 'close');
    // Every answer is lost: the same signed transaction is sent again, then the outcome is unknown.
    w.sim.lose = true;
    const events: string[] = [];
    const run = sendCaller({ ...env, ...fast, settleTries: 3 }, p, (e) =>
      events.push(e.kind === 'step' ? e.step : `checking ${e.on}`),
    );
    await expect(run).rejects.toMatchObject({ code: 'submit_unknown' });
    expect(events).toContain('checking true');
    expect(w.sim.submitted).toEqual([]);
    // With the node answering again, the same transaction (same id) goes through.
    w.sim.lose = false;
    const id = await sendCaller({ ...env, ...fast }, p, () => undefined);
    expect(id).toBe(p.built.id);
    expect(w.sim.submitted.map((s) => s.id)).toEqual([p.built.id]);
  }, 120_000);
});

describe('what is due (spec S4: close() after T with the previous round drawn; draw() from D with a bound proof; void from D + draw_window)', () => {
  const fact = (value: string) => ({ value, sources: [] });
  const base = (
    status: DetailData['status'],
    facts: Partial<DetailData['checked']['facts']>,
    vdaa: string,
    provisional = false,
  ) =>
    ({
      status,
      checked: { facts: { pools: [], ...facts } },
      provisional: provisional ? ({} as DetailData['provisional']) : undefined,
      clock: { virtualDaa: vdaa, observedAt: 0 },
    }) as unknown as DetailData;
  it('open', () => {
    expect(dueOf(base('open', { deadline: fact('100'), previousDrawn: false }, '500'))).toEqual({ kind: 'waits' });
    expect(dueOf(base('open', { deadline: fact('100'), previousDrawn: true }, '100'))).toMatchObject({ kind: 'later' });
    expect(dueOf(base('open', { deadline: fact('100'), previousDrawn: true }, '101'))).toMatchObject({
      kind: 'due',
      action: 'close',
    });
  });
  it('closing', () => {
    const f = { d: fact('200'), voidFrom: fact('900') };
    expect(dueOf(base('closing', f, '200', true))).toMatchObject({ kind: 'later', action: 'draw' });
    expect(dueOf(base('closing', f, '201', false))).toEqual({ kind: 'anchor', from: '200' });
    expect(dueOf(base('closing', f, '201', true))).toMatchObject({ kind: 'due', action: 'draw' });
    expect(dueOf(base('closing', f, '901', true))).toMatchObject({ kind: 'due', action: 'void', from: '900' });
  });
  it('drawn, void or unchecked: nothing', () => {
    for (const s of ['drawn', 'void', null] as const)
      expect(dueOf(base(s, { d: fact('1') }, '9'))).toEqual({ kind: 'none' });
  });
});
