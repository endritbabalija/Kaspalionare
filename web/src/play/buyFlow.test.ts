// @vitest-environment node
// F2/F3 review M2, S3 and S4 (the reviewer's probe web/src/review/rebuild.test.ts, inverted): the control
// flow of `buy()` around the signature and the submit. Every wasm, node and indexer call is a stub that
// records what the flow asks for; the point is what the page does between a conflict, a lost answer or a
// closed round and the next signature, not the transactions themselves (the post-sign tests run those on the
// real wasm).
import { describe, expect, it } from 'vitest';
import type { Signed } from '@wasm/web.js';
import { PageError } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { MemoryKV } from '@/store/kv';
import { WebStore } from '@/store/webstore';
import type { WalletAdapter } from '@/wallet/types';
import { buy, loadSales, prepareBuy, prepareWithReferral, type BuyEnv, type BuyEvent, type Prepared } from './buyFlow';

const COV = 'aa'.repeat(32);

interface World {
  /** The round the indexer names as current, and its status field (never trusted). */
  round: string;
  status: string;
  /** The live shards (index) on the node, of `round`. */
  live: number[];
  /** The fee the next build has. */
  fee: string;
  /** Transaction ids whose ticket is in the node's UTXO set. */
  landed: Set<string>;
  /** Referrers the builder refuses. */
  badReferrer?: string;
  builds: number;
  log: string[];
}

function shardObj(i: number) {
  return {
    role: 'shard',
    transactionId: (i + 16).toString(16).padStart(2, '0').repeat(32),
    index: 0,
    amount: '1',
    blockDaaScore: '1',
    scriptPublicKey: '00',
    state: '00',
  };
}

function stubCtx(w: World): Ctx {
  const call = async (name: string, ...args: unknown[]): Promise<unknown> => {
    w.log.push(name);
    switch (name) {
      case 'gameShape':
        return { mainMax: 30, mainPicks: 5, bMax: 4, coinsMax: 8 };
      case 'checkIndexerInstance':
        return { ok: { bMax: 4 } };
      case 'nodeEntries': {
        const objs = args[0] as { role: string; transactionId: string; index: number }[];
        if (objs[0]?.role === 'ticket') {
          const o = objs[0];
          return { ok: w.landed.has(o.transactionId) ? [{ transactionId: o.transactionId, index: o.index }] : [] };
        }
        const liveIds = new Set(w.live.map((i) => shardObj(i).transactionId));
        return {
          ok: objs
            .filter((o) => liveIds.has(o.transactionId))
            .map((o) => ({ transactionId: o.transactionId, index: o.index })),
        };
      }
      case 'checkIndexerObjects': {
        const objs = (args[0] as { objects: { transactionId: string }[] }).objects;
        return {
          ok: objs.map((o) => {
            const i = w.live.find((x) => shardObj(x).transactionId === o.transactionId) ?? -1;
            return {
              role: 'shard',
              object: { utxo: {}, state: '00' },
              view: { role: 'shard', shardIndex: String(i), m: '2', deadline: '999999999', round: w.round },
            };
          }),
        };
      }
      case 'nodeInfo':
        return { ok: { virtualDaaScore: '1' } };
      case 'nodeFeeRate':
        return { ok: 1000 };
      case 'buildBuy': {
        const input = args[0] as { referrer?: string };
        if (w.badReferrer && input.referrer === w.badReferrer)
          return { error: { code: 'built_mismatch', detail: 'referrer' } };
        w.builds++;
        return {
          ok: { id: `tx-${w.round}-${w.builds}`, transaction: '{}', sign: [{ index: 1, sighashType: 1 }], fee: w.fee },
        };
      }
      case 'buyFigures': {
        const input = args[0] as { referrer?: string; built: { fee: string } };
        return {
          ok: {
            round: w.round,
            shardIndex: 1,
            b: 1,
            fee: input.built.fee,
            price: '100',
            deposit: '13',
            depositBack: '5',
            total: String(113 + Number(input.built.fee)),
            referral: { amount: '5', paid: !!input.referrer, address: input.referrer },
            ticket: { index: 1, state: '00', scriptPublicKey: '00', address: 'x' },
          },
        };
      }
      case 'assemble': {
        const b = (args[0] as { built: { id: string } }).built;
        return { ok: { id: b.id, transaction: '{}' } };
      }
      case 'verifyState':
        return { ok: {} };
      default:
        throw new Error(`unexpected call ${name}`);
    }
  };
  return {
    config: { covenantId: COV, network: 'testnet-10' } as Ctx['config'],
    lottery: { call } as unknown as Ctx['lottery'],
    indexer: {
      status: async () => ({ covenantId: COV, currentRound: w.round }),
      round: async () => ({
        round: w.round,
        status: w.status,
        objects: [0, 1, 2, 3].map(shardObj),
      }),
      purchases: async () => ({ shards: ['', '', '', ''] }),
      instanceText: async () => '',
    } as unknown as Ctx['indexer'],
  };
}

function world(over: Partial<World> = {}): World {
  return { round: '3', status: 'open', live: [0, 1, 2, 3], fee: '100', landed: new Set(), builds: 0, log: [], ...over };
}

function setup(w: World, submit: (s: Signed, n: number) => Promise<string>) {
  const signed: string[] = [];
  const submitted: string[] = [];
  const wallet: WalletAdapter = {
    kind: 'quick',
    connect: async () => 'a',
    network: async () => 'testnet-10',
    address: async () => 'a',
    utxos: async () => [],
    signInputs: async (b) => {
      signed.push(`${b.id} fee ${b.fee}`);
      return '{}';
    },
  };
  let n = 0;
  const env: BuyEnv = {
    ctx: stubCtx(w),
    wallet,
    address: 'a',
    pick: () => 0,
    sleep: async () => undefined,
    timing: { acceptMs: 20, acceptWindows: 2, settleTries: 4, settleDelayMs: 0 },
    submit: async (s) => {
      submitted.push(s.id);
      return submit(s, n++);
    },
  };
  const events: string[] = [];
  const on = (e: BuyEvent) =>
    events.push(e.kind === 'step' ? e.step : e.kind === 'checking' ? `checking:${e.on}` : e.kind);
  return { env, signed, submitted, events, on };
}

async function first(env: BuyEnv, referrer?: string): Promise<Prepared> {
  const sales = await loadSales(env.ctx);
  return prepareBuy(env, sales, [{ main: [1, 2, 3, 4, 5], bonus: 1 }], referrer);
}

const conflict = () => new PageError('node_conflict', 'transaction is an orphan where orphan is disallowed');
const lost = () => new PageError('node_unavailable', 'submitTransaction: websocket closed');

describe('M2: a rebuild is signed only when it is the purchase the player saw', () => {
  it('a close() while signing ends in sales_closed, never a ticket of round r + 1', async () => {
    const w = world();
    const t = setup(w, async (s, n) => {
      if (n === 0) {
        // close() took round 3's shards while the wallet was open; the indexer now names round 4.
        w.round = '4';
        throw conflict();
      }
      return s.id;
    });
    const p = await first(t.env);
    await expect(buy(t.env, p, undefined, t.on)).rejects.toMatchObject({ code: 'sales_closed' });
    expect(t.signed).toEqual(['tx-3-1 fee 100']);
    expect(t.submitted).toEqual(['tx-3-1']);
  });

  it('a round whose shards are all spent (indexer lagging) also ends in sales_closed', async () => {
    const w = world();
    const t = setup(w, async (_s, n) => {
      if (n === 0) {
        w.live = [];
        throw conflict();
      }
      throw new Error('no second submit');
    });
    const p = await first(t.env);
    await expect(buy(t.env, p, undefined, t.on)).rejects.toMatchObject({ code: 'sales_closed' });
    expect(t.signed).toHaveLength(1);
  });

  it('a rebuild with another fee stops for a fresh press, then the confirmed one is signed', async () => {
    const w = world();
    const t = setup(w, async (s, n) => {
      if (n === 0) {
        w.live = [1, 2, 3];
        w.fee = '777';
        throw conflict();
      }
      w.landed.add(s.id);
      return s.id;
    });
    const p = await first(t.env);
    const r = await buy(t.env, p, undefined, t.on);
    if (r.kind !== 'confirm') throw new Error('expected a confirmation');
    expect(r.prepared.figures.fee).toBe('777');
    expect(t.signed).toEqual(['tx-3-1 fee 100']);
    expect(t.events).toContain('rebuilt');
    // The player presses Buy on the new figures.
    const r2 = await buy(t.env, r.prepared, undefined, t.on, r.resume);
    expect(r2.kind).toBe('bought');
    expect(t.signed).toEqual(['tx-3-1 fee 100', 'tx-3-2 fee 777']);
    if (r2.kind === 'bought') expect(r2.rebuilds).toBe(1);
  });

  it('a rebuild with the same figures goes on without a press (spec S1: rebuilt n/3)', async () => {
    const w = world();
    const t = setup(w, async (s, n) => {
      if (n === 0) {
        w.live = [1, 2, 3];
        throw conflict();
      }
      w.landed.add(s.id);
      return s.id;
    });
    const p = await first(t.env);
    const r = await buy(t.env, p, undefined, t.on);
    expect(r.kind).toBe('bought');
    expect(t.signed).toEqual(['tx-3-1 fee 100', 'tx-3-2 fee 100']);
    expect(t.events.filter((e) => e === 'rebuilt')).toHaveLength(1);
  });
});

describe('S3: a lost submit answer is settled from the chain, never shown as a plain failure', () => {
  it('lost answer, the ticket is already live: accepted and stored, nothing sent again', async () => {
    const w = world();
    const t = setup(w, async (s) => {
      w.landed.add(s.id);
      throw lost();
    });
    const store = new WebStore(
      {
        format: 'webstore/1',
        database: 'kaspalionare',
        version: 1,
        stores: ['meta', 'tickets', 'rounds'],
        walletDatabase: 'w',
      },
      new MemoryKV(),
    );
    t.env.store = store;
    const p = await first(t.env);
    const r = await buy(t.env, p, undefined, t.on);
    expect(r.kind).toBe('bought');
    expect(t.submitted).toEqual(['tx-3-1']);
    expect(t.signed).toHaveLength(1);
    expect(t.events).toContain('checking:true');
    expect((await store.tickets(COV)).map((x) => x.buy.transactionId)).toEqual(['tx-3-1']);
  });

  it('lost answer, the node did not have it: the same signed transaction is sent again (one signature)', async () => {
    const w = world();
    const t = setup(w, async (s, n) => {
      if (n === 0) throw lost();
      w.landed.add(s.id);
      return s.id;
    });
    const r = await buy(t.env, await first(t.env), undefined, t.on);
    expect(r.kind).toBe('bought');
    expect(t.submitted).toEqual(['tx-3-1', 'tx-3-1']);
    expect(t.signed).toHaveLength(1);
  });

  it('lost answer, the node says it already holds it: sent', async () => {
    const w = world();
    const t = setup(w, async (s, n) => {
      if (n === 0) throw lost();
      w.landed.add(s.id);
      throw new PageError(
        'node_refused',
        `Rejected transaction ${s.id}: transaction ${s.id} is already in the mempool`,
      );
    });
    expect((await buy(t.env, await first(t.env), undefined, t.on)).kind).toBe('bought');
    expect(t.signed).toHaveLength(1);
  });

  it('lost answer, then an input is spent and our ticket is not live: a rebuild is safe (nothing of ours landed)', async () => {
    const w = world();
    const t = setup(w, async (s, n) => {
      if (n === 0) throw lost();
      if (n === 1) {
        w.live = [1, 2, 3];
        throw conflict();
      }
      w.landed.add(s.id);
      return s.id;
    });
    const r = await buy(t.env, await first(t.env), undefined, t.on);
    expect(r.kind).toBe('bought');
    expect(t.signed).toEqual(['tx-3-1 fee 100', 'tx-3-2 fee 100']);
  });

  it('lost answer, then an input is spent because our own transaction went through: no rebuild, no second purchase', async () => {
    const w = world();
    const t = setup(w, async (s, n) => {
      if (n === 0) throw lost();
      w.landed.add(s.id); // it was in a block by the time the page asked again
      throw conflict();
    });
    const r = await buy(t.env, await first(t.env), undefined, t.on);
    expect(r.kind).toBe('bought');
    expect(t.signed).toHaveLength(1);
  });

  it('no answer at all: submit_unknown after the tries, one signature, no rebuild', async () => {
    const w = world();
    const t = setup(w, async () => {
      throw lost();
    });
    await expect(buy(t.env, await first(t.env), undefined, t.on)).rejects.toMatchObject({ code: 'submit_unknown' });
    expect(t.signed).toHaveLength(1);
    expect(new Set(t.submitted)).toEqual(new Set(['tx-3-1']));
    expect(t.events).toEqual(expect.arrayContaining(['checking:true', 'checking:false']));
    expect(t.events).not.toContain('rebuilt');
  });

  it('sent but not in a block in time: asked again, still held, then accepted; never a plain failure', async () => {
    const w = world();
    let asked = 0;
    const t = setup(w, async (s, n) => {
      if (n === 0) return s.id;
      asked++;
      w.landed.add(s.id);
      throw new PageError('node_refused', `transaction ${s.id} is already in the mempool`);
    });
    const r = await buy(t.env, await first(t.env), undefined, t.on);
    expect(r.kind).toBe('bought');
    expect(asked).toBe(1);
    expect(t.signed).toHaveLength(1);
  });

  it('sent but never in a block and no answer: submit_unknown', async () => {
    const w = world();
    const t = setup(w, async (s, n) => {
      if (n === 0) return s.id;
      throw lost();
    });
    await expect(buy(t.env, await first(t.env), undefined, t.on)).rejects.toMatchObject({ code: 'submit_unknown' });
    expect(t.signed).toHaveLength(1);
  });
});

describe('S4: whether the round sells comes from the chain, not the indexer', () => {
  it('live shards of the round sell whatever the indexer says; no live shard does not', async () => {
    const w = world({ status: 'drawn' });
    const t = setup(w, async (s) => s.id);
    expect((await loadSales(t.env.ctx)).open).toBe(true);
    w.status = 'open';
    w.live = [];
    expect((await loadSales(t.env.ctx)).open).toBe(false);
  });

  it('a shard of another round is refused', async () => {
    const w = world();
    const t = setup(w, async (s) => s.id);
    const ctx = t.env.ctx;
    const orig = ctx.lottery.call.bind(ctx.lottery) as (n: string, ...a: unknown[]) => Promise<unknown>;
    (ctx.lottery as unknown as { call: typeof orig }).call = async (n: string, ...a: unknown[]) => {
      const r = (await orig(n, ...a)) as { ok?: { view: { round: string } }[] };
      if (n === 'checkIndexerObjects' && r.ok?.[0]) r.ok[0].view.round = '2';
      return r;
    };
    await expect(loadSales(ctx)).rejects.toMatchObject({ code: 'chain_mismatch' });
  });
});

describe('S1 and S2 at build time', () => {
  it('the payout is the address the wallet signs for (wallet_changed otherwise)', async () => {
    const w = world();
    const t = setup(w, async (s) => s.id);
    const sales = await loadSales(t.env.ctx);
    const env = { ...t.env, address: 'b' };
    await expect(prepareBuy(env, sales, [{ main: [1, 2, 3, 4, 5], bonus: 1 }], undefined)).rejects.toMatchObject({
      code: 'wallet_changed',
    });
  });

  it('a stored referral the builder cannot use falls back to the site owner and never blocks buying', async () => {
    const w = world({ badReferrer: 'kaspatest:stored' });
    const t = setup(w, async (s) => s.id);
    const sales = await loadSales(t.env.ctx);
    const bundle = [{ main: [1, 2, 3, 4, 5] as [number, number, number, number, number], bonus: 1 }];
    const p = await prepareWithReferral(t.env, sales, bundle, 'kaspatest:stored', 'kaspatest:owner');
    expect(p.figures.referral.address).toBe('kaspatest:owner');
    // Control: with the owner's own referral refused too, the first error stands.
    w.badReferrer = 'kaspatest:owner';
    await expect(prepareWithReferral(t.env, sales, bundle, 'kaspatest:owner', 'kaspatest:owner')).rejects.toMatchObject(
      {
        code: 'built_mismatch',
      },
    );
  });
});
