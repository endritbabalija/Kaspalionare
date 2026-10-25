// @vitest-environment node
// F4 (spec S2, S8, 6.3, 7, 8 "Local copy"): My tickets' claims, deposit returns, the local copy and the
// backup, through the page's own code (tickets/*, store/*) and the real wasm (src/wasm/dev) behind the
// worker's message path. The chain is a small stand-in node (`Sim`): it answers the node calls from its UTXO
// set and, on a submit, adds the submitted transaction's outputs, so a claim is "accepted" exactly when its
// own output is live. The inputs are crates/web's fixture claims (src/test/fixtures/page-inputs.json, checked
// current by `cargo test -p web --test api page_fixtures_are_current`) with a fresh quick-wallet key's coins.
// The indexer API is stopped in every test: claims are built from the local copy and a node only.
import { beforeAll, describe, expect, it, vi } from 'vitest';
import type {
  ClaimInput,
  CovUtxo,
  QuickKey,
  ReturnInput,
  Signed,
  StoredRound,
  StoredTicket,
  Utxo,
  WebstoreInfo,
} from '@wasm/web.js';
import fixtures from '@/test/fixtures/page-inputs.json';
import { wasmClient } from '@/test/wasmWorker';
import { PageError, unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { MemoryKV } from '@/store/kv';
import { importBackup, writeBackup } from '@/store/backup';
import { roundKey, WebStore } from '@/store/webstore';
import { QuickKeyStore, QuickWalletAdapter } from '@/wallet/quick';
import type { WalletAdapter } from '@/wallet/types';
import type { LotteryClient } from '@/worker/client';
import { prepareClaim, prepareReturn, sendClaim, sendReturn } from './claimFlow';
import { loadTickets } from './load';

const fx = fixtures as unknown as Record<string, ClaimInput & ReturnInput>;
const lottery = wasmClient('testnet-10');
const INFO: WebstoreInfo = {
  format: 'webstore/2',
  reads: ['webstore/1'],
  database: 'kaspalionare',
  version: 2,
  stores: ['meta', 'tickets', 'rounds', 'pending'],
  walletDatabase: 'w',
};

let key: QuickKey;
let spk: string;
let quick: QuickWalletAdapter;
let coins: Utxo[];

type Json = { outputs: { value: string; scriptPublicKey: string; covenant?: unknown }[] };

const op = (u: { transactionId: string; index: number }) => `${u.transactionId.toLowerCase()}:${u.index}`;

/** A stand-in public node: a UTXO set, the virtual DAA score, and submits that add their outputs. */
class Sim {
  live = new Map<string, Utxo>();
  submitted: Signed[] = [];
  vdaa = '1000';
  constructor(readonly cov: string) {}
  add(u: Utxo) {
    this.live.set(op(u), u);
  }
  submit(s: Signed): string {
    this.submitted.push(s);
    const tx = JSON.parse(s.transaction) as Json & { inputs: { transactionId: string; index: number }[] };
    for (const i of tx.inputs) this.live.delete(op(i));
    tx.outputs.forEach((o, index) =>
      this.add({
        transactionId: s.id,
        index,
        amount: String(o.value),
        scriptPublicKey: o.scriptPublicKey,
        blockDaaScore: '5000',
        isCoinbase: false,
        covenantId: o.covenant ? this.cov : undefined,
      }),
    );
    return s.id;
  }
}

function ctxOf(sim: Sim): Ctx {
  const call = async (name: string, ...args: unknown[]): Promise<unknown> => {
    switch (name) {
      case 'nodeFeeRate':
        return { ok: 100 };
      case 'nodeInfo':
        return {
          ok: { url: 'sim', network: 'testnet-10', serverVersion: 'sim', isSynced: true, virtualDaaScore: sim.vdaa },
        };
      case 'nodeEntries':
        return {
          ok: (args[0] as { transactionId: string; index: number }[]).map((o) => sim.live.get(op(o))).filter(Boolean),
        };
      case 'nodeScriptEntries': {
        const i = args[0] as { covenantId: string; scripts: string[] };
        return {
          ok: [...sim.live.values()].filter(
            (u) => i.scripts.includes(u.scriptPublicKey) && u.covenantId === i.covenantId,
          ),
        };
      }
      case 'nodeSubmit':
        return { ok: sim.submit(args[0] as Signed) };
      default:
        return (lottery.call as (n: string, ...a: unknown[]) => Promise<unknown>)(name, ...args);
    }
  };
  const down = () => Promise.reject(new PageError('indexer_unavailable', 'stopped (test)'));
  return {
    config: { covenantId: sim.cov, network: 'testnet-10' } as Ctx['config'],
    lottery: { call } as unknown as LotteryClient,
    indexer: { tickets: down, round: down, purchases: down, status: down } as unknown as Ctx['indexer'],
  };
}

function need<T>(x: T | null | undefined): T {
  if (x === null || x === undefined) throw new Error('missing in the test world');
  return x;
}

const fast = { acceptMs: 50, pollMs: 1, settleTries: 1, settleDelayMs: 1, sleep: () => Promise.resolve() };

beforeAll(async () => {
  key = unwrap(await lottery.call('quickWalletNew'));
  spk = unwrap(await lottery.call('scriptOf', key.address));
  const ks = new QuickKeyStore(new MemoryKV(), 'testnet-10');
  await ks.replace(null, key);
  coins = need(fx['claim k=4']).funds.coins.map((c) => ({ ...c, scriptPublicKey: spk }));
  quick = new QuickWalletAdapter(lottery, ks, 'testnet-10', () => Promise.resolve(coins), key);
}, 120_000);

/** The local copy of a fixture claim's round (drawn) and its ticket, checked as copyRound keeps them. */
async function copyOf(name: string): Promise<{ ticket: StoredTicket; round: StoredRound; input: ClaimInput }> {
  const i = fx[name] as ClaimInput;
  const pool = i.pools[0] as CovUtxo;
  const pv = unwrap(await lottery.call('decodeState', 'pool', pool.state));
  if (pv.role !== 'pool') throw new Error('not a pool');
  const roots = unwrap(
    await lottery.call('checkRoots', { covenantId: i.covenantId, against: pool, purchases: i.purchases }),
  );
  const card = unwrap(
    await lottery.call('ticketCard', { covenantId: i.covenantId, ticket: i.ticket, virtualDaaScore: '1' }),
  );
  const po = { role: 'pool' as const, state: pool.state, scriptPublicKey: pool.utxo.scriptPublicKey };
  return {
    input: i,
    ticket: {
      key: `${'ee'.repeat(32)}:1`,
      covenantId: i.covenantId,
      network: 'testnet-10',
      round: pv.round,
      shardIndex: 0,
      ticket: i.ticket,
      bundle: card.combos.map((c) => c.combination),
      payout: card.payout,
      savedMs: '1',
    },
    round: {
      key: roundKey(i.covenantId, pv.round),
      covenantId: i.covenantId,
      network: 'testnet-10',
      round: pv.round,
      m: Number(pv.m),
      k: Number(pv.k),
      stage: 'drawn',
      purchases: i.purchases,
      roots: roots.roots,
      against: po,
      result: { winning: pv.win, claimDeadline: pv.deadline, pools: [po] },
      checkedMs: '1',
    },
  };
}

async function world(name: string) {
  const c = await copyOf(name);
  const sim = new Sim(c.input.covenantId);
  sim.add(c.input.ticket.utxo);
  for (const p of c.input.pools) sim.add(p.utxo);
  const store = new WebStore(INFO, new MemoryKV());
  await store.putTicket(c.ticket);
  await store.putRound(c.round);
  return { ...c, sim, store, ctx: ctxOf(sim) };
}

describe('claims from the local copy, with the indexer API stopped (spec 8 "Local copy")', () => {
  for (const [name, mode] of [
    ['claim k=4', 'pay'],
    ['claim pay h=2', 'pay'],
    ['claim accrue', 'accrue'],
    ['claim final', 'final'],
  ] as const) {
    it(`${name}: the card from the copy, the row's figures signed as shown, accepted, the store follows`, async () => {
      const w = await world(name);
      const data = await loadTickets(w.ctx, w.store, key.address);
      expect(data.apiDown).toBe(true);
      expect(data.copyBox).toBe('complete');
      const c = data.cards[0];
      expect(c?.copy).toBe('verified');
      const combo = c?.card?.combos.find((x) => x.status === 'claimable');
      expect(combo?.claim?.mode).toBe(mode);
      if (name === 'claim pay h=2') expect(combo?.claim?.sharedWith).toBe('1');
      const env = { ctx: w.ctx, wallet: quick, address: key.address };
      const p = await prepareClaim(env, need(need(c).chain), need(combo).index);
      // The row's figures are the card's claim, bound to the built transaction.
      expect(p.figures.claim).toEqual(combo?.claim);
      expect(p.figures.fee).toBe(p.built.fee);
      await sendClaim({ ...env, ...fast }, w.store, need(need(c).chain), p, () => undefined);
      expect(w.sim.submitted.map((s) => s.id)).toEqual([p.built.id]);
      const after = await w.store.ticket(w.ticket.key);
      if (mode === 'final') {
        expect(after?.ended).toMatchObject({ kind: 'claimed', transactionId: p.built.id, amount: p.figures.paidNow });
      } else {
        expect(after?.ticket.utxo.transactionId).toBe(p.built.id);
        expect(after?.ticket.state).toBe(p.figures.ticketNext?.state);
        // The next card reads the successor: that index is claimed.
        const next = await loadTickets(w.ctx, w.store, key.address);
        expect(next.cards[0]?.card?.combos[need(combo).index]?.status).toBe('claimed');
      }
    });
  }

  it('a wallet that changes the claim it signs is refused before anything is sent', async () => {
    const w = await world('claim k=4');
    const data = await loadTickets(w.ctx, w.store);
    const c = need(data.cards[0]);
    const env = { ctx: w.ctx, wallet: quick, address: key.address };
    const p = await prepareClaim(env, need(c.chain), 0);
    const bad: WalletAdapter = {
      ...quick,
      kind: 'quick',
      connect: () => quick.connect(),
      network: () => quick.network(),
      address: () => quick.address(),
      utxos: () => quick.utxos(),
      async signInputs(b, ix) {
        const v = JSON.parse(await quick.signInputs(b, ix)) as Json;
        (v.outputs[2] as { value: string }).value = '1';
        return JSON.stringify(v);
      },
    };
    await expect(
      sendClaim({ ...env, wallet: bad, ...fast }, w.store, need(c.chain), p, () => undefined),
    ).rejects.toMatchObject({
      code: 'signed_changed',
    });
    expect(w.sim.submitted).toEqual([]);
  });

  it('a kept copy whose lists the chain contradicts is shown as a mismatch, and builds nothing', async () => {
    const w = await world('claim k=4');
    const r = await w.store.round(w.round.covenantId, w.round.round);
    const lists = [...(r?.purchases ?? [])];
    const n = lists.findIndex((l) => l.length >= 12);
    lists[n] = need(lists[n]).slice(12);
    await w.store.putRound({ ...need(r), purchases: lists });
    const data = await loadTickets(w.ctx, w.store);
    expect(data.copyBox).toBe('mismatch');
    expect(data.cards[0]?.chain).toBeUndefined();
  });
});

describe('return_deposit() (spec S2, 7 "Deposits")', () => {
  it('path a (a pool past its claim deadline) and path b (from T + D_RET): the return as shown, accepted', async () => {
    for (const [vdaa, path] of [
      ['7500100', 'withPool'],
      ['13100000', 'alone'],
    ] as const) {
      const w = await world('claim k=4');
      w.sim.vdaa = vdaa;
      const data = await loadTickets(w.ctx, w.store);
      const c = need(data.cards[0]);
      expect(c.card?.returnable?.path).toBe(path);
      const env = { ctx: w.ctx, wallet: quick, address: key.address };
      const p = await prepareReturn(env, need(c.chain));
      expect(p.figures.amount).toBe(c.card?.value);
      expect(p.figures.path).toBe(path);
      await sendReturn({ ...env, ...fast }, w.store, need(c.chain), p, () => undefined);
      expect((await w.store.ticket(w.ticket.key))?.ended).toMatchObject({ kind: 'returned', amount: p.figures.amount });
    }
  });

  it('before either path is valid, nothing is offered', async () => {
    const w = await world('claim k=4');
    w.sim.vdaa = '7000000';
    const data = await loadTickets(w.ctx, w.store);
    expect(data.cards[0]?.card?.returnable).toBeUndefined();
    await expect(
      prepareReturn({ ctx: w.ctx, wallet: quick, address: key.address }, need(need(data.cards[0]).chain)),
    ).rejects.toThrow();
  });
});

describe('S8 backup and import (spec 8 "Local copy")', () => {
  it('after clearing the browser, the imported backup claims; a damaged or doctored file is refused', async () => {
    const w = await world('claim k=4');
    const text = await writeBackup(w.ctx, w.store);
    expect(text).not.toContain(key.secretKey);
    // A cleared browser: a new, empty store.
    const fresh = new WebStore(INFO, new MemoryKV());
    const report = await importBackup(w.ctx, fresh, text);
    expect(report.ticketsKept).toBe(1);
    expect(report.rounds[w.round.round]).toBe('kept');
    expect(report.summary[0]).toMatchObject({ round: w.round.round, tickets: 1, stage: 'drawn' });
    const data = await loadTickets(w.ctx, fresh, key.address);
    const c = need(data.cards[0]);
    const env = { ctx: w.ctx, wallet: quick, address: key.address };
    const p = await prepareClaim(env, need(c.chain), 0);
    await sendClaim({ ...env, ...fast }, fresh, need(c.chain), p, () => undefined);
    expect(w.sim.submitted).toHaveLength(1);

    const empty = new WebStore(INFO, new MemoryKV());
    const corrupted = text.slice(0, text.length / 2) + '7' + text.slice(text.length / 2 + 1);
    await expect(importBackup(w.ctx, empty, corrupted)).rejects.toMatchObject({ code: 'backup_invalid' });
    await expect(importBackup(w.ctx, empty, text.slice(0, 100))).rejects.toMatchObject({ code: 'backup_invalid' });
    expect(await empty.tickets(w.round.covenantId)).toEqual([]);
    expect(await empty.rounds(w.round.covenantId)).toEqual([]);
  });

  it('a file whose ticket the chain contradicts is refused whole (backup_mismatch); nothing is kept', async () => {
    const w = await world('claim final');
    const text = await writeBackup(w.ctx, w.store);
    // The node holds another output at the ticket's outpoint (the file names a UTXO that is not its ticket).
    const t = w.input.ticket.utxo;
    w.sim.add({ ...t, scriptPublicKey: need(w.input.pools[0]).utxo.scriptPublicKey });
    const empty = new WebStore(INFO, new MemoryKV());
    await expect(importBackup(w.ctx, empty, text)).rejects.toMatchObject({ code: 'backup_mismatch' });
    expect(await empty.tickets(w.round.covenantId)).toEqual([]);
    expect(await empty.rounds(w.round.covenantId)).toEqual([]);
  });

  it('a spent ticket and a round with nothing left on chain are left out, not refused', async () => {
    const w = await world('claim k=4');
    const text = await writeBackup(w.ctx, w.store);
    w.sim.live.clear();
    const empty = new WebStore(INFO, new MemoryKV());
    const r = await importBackup(w.ctx, empty, text);
    expect(r).toMatchObject({ ticketsKept: 0, ticketsSpent: 1 });
    expect(r.rounds[w.round.round]).toBe('gone');
  });
});

describe('a purchase whose outcome was not learnt (F2/F3 review S3, F4)', () => {
  it('settles on the next visit: the ticket output live makes it a ticket', async () => {
    const w = await world('claim k=4');
    const sub = vi.fn();
    const t = w.input.ticket;
    await w.store.putPending({
      key: t.utxo.transactionId,
      covenantId: w.round.covenantId,
      network: 'testnet-10',
      round: w.round.round,
      shardIndex: 0,
      ticket: {
        index: t.utxo.index,
        state: t.state,
        scriptPublicKey: t.utxo.scriptPublicKey,
        address: w.ticket.payout,
      },
      bundle: w.ticket.bundle,
      payout: w.ticket.payout,
      buy: {
        transactionId: t.utxo.transactionId,
        price: '1',
        deposit: t.utxo.amount,
        depositBack: '1',
        fee: '1',
        referral: { amount: '0', paid: false, address: undefined },
      },
      signed: { transaction: '{}', id: t.utxo.transactionId },
      savedMs: '1',
    });
    const fresh = await w.store.tickets(w.round.covenantId);
    await loadTickets(w.ctx, w.store);
    expect(sub).not.toHaveBeenCalled();
    expect(await w.store.pending(w.round.covenantId)).toEqual([]);
    expect((await w.store.tickets(w.round.covenantId)).length).toBe(fresh.length + 1);
  });
});
