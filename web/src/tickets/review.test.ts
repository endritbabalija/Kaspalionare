// @vitest-environment node
// F4 review regressions (the reviewer's probe src/review/f4probe.test.ts, inverted, and the fixes' own
// tests), through the page's own code (tickets/*, store/*) and the real dev wasm with the stand-in node of
// src/test/ticketWorld.ts. Each guards money or data; each has its control.
import { beforeAll, describe, expect, it, vi } from 'vitest';
import type { QuickKey, StoredPending } from '@wasm/web.js';
import { PageError, unwrap } from '@/lib/errors';
import { BACKUP_FILE_MAX, importBackup, writeBackup } from '@/store/backup';
import { MemoryKV } from '@/store/kv';
import { WebStore } from '@/store/webstore';
import { copyOf, ctxOf, fast, fx, INFO, lottery, marked, need, pooled, Sim, world } from '@/test/ticketWorld';
import { QuickKeyStore, QuickWalletAdapter } from '@/wallet/quick';
import { ARGS_MAX_BYTES } from '@/worker/client';
import { prepareClaim, sendClaim } from './claimFlow';
import { findByPayout, loadTickets, RESEND_MS, resendPending } from './load';

let key: QuickKey;
let quick: QuickWalletAdapter;

beforeAll(async () => {
  key = unwrap(await lottery.call('quickWalletNew'));
  const spk = unwrap(await lottery.call('scriptOf', key.address));
  const ks = new QuickKeyStore(new MemoryKV(), 'testnet-10');
  await ks.replace(null, key);
  const coins = need(fx['claim k=4']).funds.coins.map((c) => ({ ...c, scriptPublicKey: spk }));
  quick = new QuickWalletAdapter(lottery, ks, 'testnet-10', () => Promise.resolve(coins), key);
}, 120_000);

const env = (w: { ctx: Parameters<typeof prepareClaim>[0]['ctx'] }) => ({
  ctx: w.ctx,
  wallet: quick,
  address: key.address,
});

describe('M1: Return deposit is offered only when nothing unclaimed can still be claimed', () => {
  it('past the claim deadline with the pool live (path a) and from T + D_RET (path b): a claimable index blocks it', async () => {
    for (const vdaa of ['7500100', '13100000']) {
      const w = await world('claim k=4');
      w.sim.vdaa = vdaa;
      const card = need(need((await loadTickets(w.ctx, w.store, key.address)).cards[0]).card);
      expect(card.combos[0]?.status).toBe('claimable');
      expect(card.returnable).toBeUndefined();
    }
    // Control (the same world, the win claimed first): tickets.test.ts "return_deposit()".
  });
});

describe('M2: a drawn copy is checked for its pools; "expired" only with the chain showing the pool absorbed', () => {
  it('a copy listing only another tier`s pool: the win is unknown (never expired), the box incomplete, a re-copy asked', async () => {
    const m = need(marked['claim k=4']);
    const other = need(
      m.pools.find((p) => p.utxo.scriptPublicKey !== need(fx['claim k=4']).pools[0]?.utxo.scriptPublicKey),
    );
    const round = vi.fn(() => Promise.reject(new PageError('indexer_unavailable', 'stopped (test)')));
    const w = await world('claim k=4', { pools: [pooled(other)] }, { round });
    w.sim.add(other.utxo);
    w.sim.vdaa = '7500100'; // past the claim deadline: still no evidence the own pool was absorbed
    const data = await loadTickets(w.ctx, w.store);
    const c = need(data.cards[0]);
    expect(c.copy).toBe('verified');
    expect(c.card?.combos[0]?.status).toBe('unknown');
    expect(c.card?.returnable).toBeUndefined();
    expect(data.copyBox).toBe('incomplete');
    expect(round).toHaveBeenCalledWith(w.round.round);
    // Control: the copy lists the pool: claimable, complete.
    const ok = await world('claim k=4');
    const d2 = await loadTickets(ok.ctx, ok.store, key.address);
    expect(d2.cards[0]?.card?.combos[0]?.status).toBe('claimable');
    expect(d2.copyBox).toBe('complete');
  });

  it('a listed pool gone after its claim deadline is expired; before it, unknown', async () => {
    for (const [vdaa, want] of [
      ['7500100', 'expired'],
      ['1000', 'unknown'],
    ] as const) {
      const w = await world('claim k=4');
      w.sim.live.delete(
        `${need(w.input.pools[0]).utxo.transactionId.toLowerCase()}:${need(w.input.pools[0]).utxo.index}`,
      );
      w.sim.vdaa = vdaa;
      const c = need((await loadTickets(w.ctx, w.store, key.address)).cards[0]);
      expect(c.card?.combos[0]?.status).toBe(want);
    }
  });

  it('with the marker kept the copy lists every pool of the draw: complete; an import never makes it list fewer', async () => {
    const m = need(marked['claim k=4']);
    const w = await world('claim k=4', { pools: m.pools.map(pooled), marker: true });
    for (const p of m.pools) w.sim.add(p.utxo);
    const data = await loadTickets(w.ctx, w.store, key.address);
    expect(data.copyBox).toBe('complete');
    expect(data.cards[0]?.card?.combos[0]?.status).toBe('claimable');
    // A backup of the thin copy (one pool, no marker) imported into this store keeps the five pools.
    const thin = await world('claim k=4');
    const text = await writeBackup(thin.ctx, thin.store);
    await importBackup(w.ctx, w.store, text);
    expect((await w.store.round(w.round.covenantId, w.round.round))?.result?.pools).toHaveLength(5);
    // And the full copy imported into a store holding the thin one gives it all five.
    const full = await writeBackup(w.ctx, w.store);
    const into = await world('claim k=4');
    for (const p of m.pools) into.sim.add(p.utxo);
    await importBackup(into.ctx, into.store, full);
    expect((await into.store.round(w.round.covenantId, w.round.round))?.marker).toBeDefined();
    expect((await into.store.round(w.round.covenantId, w.round.round))?.result?.pools).toHaveLength(5);
  });
});

describe('S1: "void" only where the chain and the copy do not contradict it', () => {
  const voidRound = (r: string) => ({
    round: r,
    status: 'void',
    k: 4,
    objects: [],
    gaps: [],
    buys: 0,
    combinations: 0,
  });

  it('the indexer says void with nothing the chain shows: not void (the probe`s P2); supported: void; a kept result: not void', async () => {
    const w = await world('claim k=4', { stage: 'open' });
    const ctx = ctxOf(w.sim, { round: () => Promise.resolve(voidRound(w.round.round)) });
    const c = need((await loadTickets(ctx, w.store)).cards[0]);
    expect(c.void).toBe(false);
    expect(c.card?.stage).toBe('open');
    // Control: the chain check supports it (stubbed here; the Rust check is crates/indexer-api tests/review_f4.rs).
    const yes = (r: unknown) => ({ ok: { ...(r as object), voidSupported: true } });
    const supported = ctxOf(
      w.sim,
      { round: () => Promise.resolve(voidRound(w.round.round)) },
      { checkIndexerRound: (i: unknown) => yes({ round: (i as { round: { round: string } }).round.round }) },
    );
    const v = need((await loadTickets(supported, w.store)).cards[0]);
    expect(v.void).toBe(true);
    expect(v.card?.stage).toBe('void');
    // A drawn copy refuses it whatever the indexer and its check say.
    const d = await world('claim k=4');
    const drawn = ctxOf(
      d.sim,
      { round: () => Promise.resolve(voidRound(d.round.round)) },
      { checkIndexerRound: () => yes({}) },
    );
    expect(need((await loadTickets(drawn, d.store)).cards[0]).void).toBe(false);
  });
});

describe('S2: J3 keeps only tickets that pay this wallet', () => {
  it('a live ticket of another payout planted by the indexer is not stored (the probe`s P1); its own payout: stored', async () => {
    const i = need(fx['claim k=4']);
    const obj = { role: 'ticket' as const, ...i.ticket.utxo, state: i.ticket.state };
    const sim = new Sim(i.covenantId);
    sim.add(i.ticket.utxo);
    const tickets = (payout: string) => () => Promise.resolve({ payout, tickets: [obj], bought: [] });
    const store = new WebStore(INFO, new MemoryKV());
    expect(await findByPayout(ctxOf(sim, { tickets: tickets(key.address) }), store, key.address)).toBe(0);
    expect(await store.tickets(i.covenantId)).toEqual([]);
    // Control: the wallet that is the ticket's payout gets it.
    const own = (await copyOf('claim k=4')).ticket.payout;
    expect(await findByPayout(ctxOf(sim, { tickets: tickets(own) }), store, own)).toBe(1);
    expect((await store.tickets(i.covenantId))[0]?.payout).toBe(own);
  });
});

describe('S4: a closed copy whose marker is spent is incomplete', () => {
  it('the marker gone: incomplete (the probe`s P3); live: complete', async () => {
    const w = await world('claim k=4', { stage: 'closed' });
    const data = await loadTickets(w.ctx, w.store, key.address);
    expect(need(data.cards[0]).copy).toBe('incomplete');
    expect(data.copyBox).toBe('incomplete');
    w.sim.add(need(marked['claim k=4']).markerUtxo.utxo);
    const live = await loadTickets(w.ctx, w.store, key.address);
    expect(need(live.cards[0]).copy).toBe('verified');
    expect(live.copyBox).toBe('complete');
  });
});

describe('S5: a claim is accepted on its own outputs, not only on the pool`s successor', () => {
  it('FINAL with the pool`s new output spent at once by another claimer: accepted, its end stored (the probe`s P6)', async () => {
    for (const taken of [true, false]) {
      const w = await world('claim final');
      w.sim.poolTaken = taken;
      const c = need((await loadTickets(w.ctx, w.store, key.address)).cards[0]);
      const combo = need(c.card?.combos.find((x) => x.status === 'claimable'));
      const p = await prepareClaim(env(w), need(c.chain), combo.index);
      expect(p.figures.claim.mode).toBe('final');
      await sendClaim({ ...env(w), ...fast }, w.store, need(c.chain), p, () => undefined);
      expect((await w.store.ticket(w.ticket.key))?.ended).toMatchObject({ kind: 'claimed', transactionId: p.built.id });
    }
  });
});

describe('S6: a claim whose outcome is unknown is kept and settled on the next visit', () => {
  it('the answer lost: submit_unknown with the signed claim kept; next visit: the successor kept, one card even with J3', async () => {
    const w = await world('claim k=4');
    const c = need((await loadTickets(w.ctx, w.store, key.address)).cards[0]);
    const p = await prepareClaim(env(w), need(c.chain), 0);
    w.sim.lose = true;
    await expect(sendClaim({ ...env(w), ...fast }, w.store, need(c.chain), p, () => undefined)).rejects.toMatchObject({
      code: 'submit_unknown',
    });
    const kept = need(await w.store.ticket(w.ticket.key));
    expect(kept.pending?.signed.id).toBe(p.built.id);
    expect(kept.pending?.successor?.state).toBe(p.figures.ticketNext?.state);
    // It landed after all (another node had it); the API serves the successor as a ticket of this payout.
    w.sim.lose = false;
    w.sim.apply(need(kept.pending).signed);
    const succ = need(w.sim.live.get(`${p.built.id.toLowerCase()}:1`));
    const obj = { role: 'ticket' as const, ...succ, state: need(p.figures.ticketNext).state };
    const payout = kept.payout;
    const ctx = ctxOf(w.sim, { tickets: () => Promise.resolve({ payout, tickets: [obj], bought: [] }) });
    const data = await loadTickets(ctx, w.store, payout);
    expect(data.cards).toHaveLength(1);
    const after = need(await w.store.ticket(w.ticket.key));
    expect(after.pending).toBeUndefined();
    expect(after.ticket.utxo.transactionId).toBe(p.built.id);
    expect(data.cards[0]?.card?.combos[0]?.status).toBe('claimed');
  });

  it('a claim refused for good (its input spent) leaves no pending record', async () => {
    const w = await world('claim k=4');
    const c = need((await loadTickets(w.ctx, w.store, key.address)).cards[0]);
    const p = await prepareClaim(env(w), need(c.chain), 0);
    w.sim.spend(need(w.input.pools[0]).utxo);
    await expect(sendClaim({ ...env(w), ...fast }, w.store, need(c.chain), p, () => undefined)).rejects.toMatchObject({
      code: 'node_conflict',
    });
    expect((await w.store.ticket(w.ticket.key))?.pending).toBeUndefined();
  });
});

describe('Notes: a stale signed purchase is not sent by itself; the limits agree', () => {
  async function pendingWorld(savedMs: number) {
    const w = await world('claim k=4');
    const t = w.input.ticket;
    // A signed purchase whose ticket output is not live (its outcome unknown), signed at `savedMs`.
    const p: StoredPending = {
      key: 'ab'.repeat(32),
      covenantId: w.round.covenantId,
      network: 'testnet-10',
      round: w.round.round,
      shardIndex: 0,
      ticket: { index: 1, state: t.state, scriptPublicKey: t.utxo.scriptPublicKey, address: w.ticket.payout },
      bundle: w.ticket.bundle,
      payout: w.ticket.payout,
      buy: {
        transactionId: 'ab'.repeat(32),
        price: '1',
        deposit: t.utxo.amount,
        depositBack: '1',
        fee: '1',
        referral: { amount: '0', paid: false, address: undefined },
      },
      signed: { transaction: '{"inputs":[],"outputs":[]}', id: 'ab'.repeat(32) },
      savedMs: String(savedMs),
    };
    await w.store.putPending(p);
    return { w, p };
  }

  it('within RESEND_MS it is sent again; older, it waits for a press, and a press sends it', async () => {
    const now = 10 * RESEND_MS;
    const fresh = await pendingWorld(now - 1000);
    await loadTickets(fresh.w.ctx, fresh.w.store, undefined, { now: () => now });
    expect(fresh.w.sim.submitted.map((s) => s.id)).toEqual([fresh.p.key]);
    const old = await pendingWorld(now - RESEND_MS - 1);
    const data = await loadTickets(old.w.ctx, old.w.store, undefined, { now: () => now });
    expect(old.w.sim.submitted).toEqual([]);
    expect(data.stale.map((x) => x.key)).toEqual([old.p.key]);
    await resendPending(old.w.ctx, old.w.store, old.p.key);
    expect(old.w.sim.submitted.map((s) => s.id)).toEqual([old.p.key]);
  });

  it('the node is asked in batches of indexerLimits().nodeObjects; the backup and worker caps are the Rust ones', async () => {
    const limits = await lottery.call('indexerLimits');
    expect(ARGS_MAX_BYTES).toBe(limits.workerArgs);
    expect(BACKUP_FILE_MAX).toBe(limits.backup);
    const w = await world('claim k=4');
    await w.store.putTicket({ ...w.ticket, key: `${'ef'.repeat(32)}:1` });
    const log: string[] = [];
    const ctx = ctxOf(w.sim, {}, { indexerLimits: () => ({ ...limits, nodeObjects: 1 }) }, log);
    await loadTickets(ctx, w.store, key.address);
    expect(log.filter((n) => n === 'nodeEntries').length).toBeGreaterThanOrEqual(2);
  });
});
