// The F4 test world (tickets/*.test.ts): a small stand-in public node (`Sim`: a UTXO set, the virtual DAA
// score, submits that add their outputs), a context whose node calls it answers and whose other calls go to
// the real dev wasm (in-process, through the worker's handler), and the local copy of a fixture claim's round
// (crates/web's page fixtures, src/test/fixtures/page-inputs.json). The indexer API is stopped unless a test
// gives it answers.
import type {
  ClaimInput,
  CovUtxo,
  ReturnInput,
  Signed,
  StoredObject,
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
import { roundKey, WebStore } from '@/store/webstore';
import type { LotteryClient } from '@/worker/client';

export const fx = fixtures as unknown as Record<string, ClaimInput & ReturnInput>;
/** Each claim round's marker and every pool its draw created (crates/web tests/common `marked_round`). */
export const marked = (fixtures as unknown as Record<string, Record<string, Marked>>)['marked rounds'] as Record<
  string,
  Marked
>;
export interface Marked {
  marker: StoredObject;
  markerUtxo: CovUtxo;
  pools: CovUtxo[];
}

export const lottery = wasmClient('testnet-10');
export const INFO: WebstoreInfo = {
  format: 'webstore/2',
  reads: ['webstore/1'],
  database: 'kaspalionare',
  version: 2,
  stores: ['meta', 'tickets', 'rounds', 'pending'],
  walletDatabase: 'w',
};

type Json = {
  outputs: { value: string; scriptPublicKey: string; covenant?: unknown }[];
  inputs: { transactionId: string; index: number }[];
};

export const op = (u: { transactionId: string; index: number }) => `${u.transactionId.toLowerCase()}:${u.index}`;

/** A stand-in public node: a UTXO set, the virtual DAA score, and submits that add their outputs. */
export class Sim {
  live = new Map<string, Utxo>();
  submitted: Signed[] = [];
  vdaa = '1000';
  /** A submit's answer is lost: the node takes nothing and the page hears node_unavailable. */
  lose = false;
  /** Another claimer spends output 0 (the pool's successor) of every accepted transaction at once. */
  poolTaken = false;
  /** Outpoints spent (by a transaction here or `spend`): a later spend of one is a conflict. */
  gone = new Set<string>();
  constructor(readonly cov: string) {}
  add(u: Utxo) {
    this.live.set(op(u), u);
  }
  /** Another transaction spends this outpoint. */
  spend(u: { transactionId: string; index: number }) {
    this.live.delete(op(u));
    this.gone.add(op(u));
  }
  apply(s: Signed): string {
    const tx = JSON.parse(s.transaction) as Json;
    for (const i of tx.inputs) {
      if (this.gone.has(op(i))) throw new PageError('node_conflict', `input ${op(i)} is spent`);
    }
    if (this.submitted.some((x) => x.id === s.id)) {
      throw new PageError('node_refused', `transaction ${s.id} was already accepted by the consensus`);
    }
    this.submitted.push(s);
    // The wallet's coins are not tracked here (tests reuse them); the chain objects are.
    for (const i of tx.inputs) if (this.live.has(op(i))) this.spend(i);
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
    if (this.poolTaken) this.spend({ transactionId: s.id, index: 0 });
    return s.id;
  }
  submit(s: Signed): string {
    if (this.lose) throw new PageError('node_unavailable', 'the answer was lost (test)');
    return this.apply(s);
  }
}

export type Api = Partial<Record<'tickets' | 'round' | 'purchases' | 'status', (...a: unknown[]) => Promise<unknown>>>;

/** A context on `sim`; `indexer` answers what the test gives (else indexer_unavailable); `calls` overrides wasm calls. */
export function ctxOf(
  sim: Sim,
  indexer: Api = {},
  calls: Record<string, (...a: unknown[]) => unknown> = {},
  log: string[] = [],
): Ctx {
  const call = async (name: string, ...args: unknown[]): Promise<unknown> => {
    log.push(name);
    const own = calls[name];
    if (own) return own(...args);
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
        try {
          return { ok: sim.submit(args[0] as Signed) };
        } catch (e) {
          const p = e as PageError;
          return { error: { code: p.code, message: p.message, detail: p.detail } };
        }
      default:
        return (lottery.call as (n: string, ...a: unknown[]) => Promise<unknown>)(name, ...args);
    }
  };
  const down = () => Promise.reject(new PageError('indexer_unavailable', 'stopped (test)'));
  return {
    config: { covenantId: sim.cov, network: 'testnet-10' } as Ctx['config'],
    lottery: { call } as unknown as LotteryClient,
    indexer: { tickets: down, round: down, purchases: down, status: down, ...indexer } as unknown as Ctx['indexer'],
  };
}

export function need<T>(x: T | null | undefined): T {
  if (x === null || x === undefined) throw new Error('missing in the test world');
  return x;
}

export const fast = { acceptMs: 50, pollMs: 1, settleTries: 1, settleDelayMs: 1, sleep: () => Promise.resolve() };

export function pooled(c: CovUtxo): StoredObject {
  return { role: 'pool', state: c.state, scriptPublicKey: c.utxo.scriptPublicKey };
}

/**
 * The local copy of a fixture claim's round and its ticket, as copyRound keeps them: drawn with the claim's
 * own pool (`pools`: which pools the copy lists), or closed with the round's marker, or open.
 */
export async function copyOf(
  name: string,
  opts: { stage?: StoredRound['stage']; pools?: StoredObject[]; marker?: boolean } = {},
): Promise<{ ticket: StoredTicket; round: StoredRound; input: ClaimInput }> {
  const i = need(fx[name]) as ClaimInput;
  const pool = need(i.pools[0]);
  const pv = unwrap(await lottery.call('decodeState', 'pool', pool.state));
  if (pv.role !== 'pool') throw new Error('not a pool');
  const roots = unwrap(
    await lottery.call('checkRoots', { covenantId: i.covenantId, against: pool, purchases: i.purchases }),
  );
  const card = unwrap(
    await lottery.call('ticketCard', { covenantId: i.covenantId, ticket: i.ticket, virtualDaaScore: '1' }),
  );
  const stage = opts.stage ?? 'drawn';
  const pools = opts.pools ?? [pooled(pool)];
  const m = need(marked[name]);
  const base: StoredRound = {
    key: roundKey(i.covenantId, pv.round),
    covenantId: i.covenantId,
    network: 'testnet-10',
    round: pv.round,
    m: Number(pv.m),
    k: Number(pv.k),
    stage,
    purchases: i.purchases,
    roots: roots.roots,
    checkedMs: '1',
  };
  const round: StoredRound =
    stage === 'drawn'
      ? {
          ...base,
          against: pools[0],
          result: { winning: pv.win, claimDeadline: pv.deadline, pools },
          marker: opts.marker ? m.marker : undefined,
        }
      : stage === 'closed'
        ? { ...base, against: m.marker }
        : base;
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
    round,
  };
}

/** The copy in a fresh store, the ticket and the claim's own pool live on a fresh stand-in node. */
export async function world(name: string, opts: Parameters<typeof copyOf>[1] = {}, indexer: Api = {}) {
  const c = await copyOf(name, opts);
  const sim = new Sim(c.input.covenantId);
  sim.add(c.input.ticket.utxo);
  for (const p of c.input.pools) sim.add(p.utxo);
  const store = new WebStore(INFO, new MemoryKV());
  await store.putTicket(c.ticket);
  await store.putRound(c.round);
  return { ...c, sim, store, ctx: ctxOf(sim, indexer) };
}
