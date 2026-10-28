// @vitest-environment node
// F4 (spec S2, S8, 6.3, 7, 8 "Local copy"): My tickets' claims, deposit returns, the local copy and the
// backup, through the page's own code (tickets/*, store/*) and the real wasm (src/wasm/dev) behind the
// worker's message path. The chain is a small stand-in node (`Sim`): it answers the node calls from its UTXO
// set and, on a submit, adds the submitted transaction's outputs, so a claim is "accepted" exactly when its
// own output is live. The inputs are crates/web's fixture claims (src/test/fixtures/page-inputs.json, checked
// current by `cargo test -p web --test api page_fixtures_are_current`) with a fresh quick-wallet key's coins.
// The indexer API is stopped in every test: claims are built from the local copy and a node only.
import { beforeAll, describe, expect, it, vi } from 'vitest';
import type { QuickKey, Utxo } from '@wasm/web.js';
import { unwrap } from '@/lib/errors';
import { MemoryKV } from '@/store/kv';
import { importBackup, writeBackup } from '@/store/backup';
import { WebStore } from '@/store/webstore';
import { fast, fx, INFO, lottery, need, world } from '@/test/ticketWorld';
import { QuickKeyStore, QuickWalletAdapter } from '@/wallet/quick';
import type { WalletAdapter } from '@/wallet/types';
import { prepareClaim, prepareReturn, sendClaim, sendReturn } from './claimFlow';
import { loadTickets } from './load';

let key: QuickKey;
let spk: string;
let quick: QuickWalletAdapter;
let coins: Utxo[];

type Json = { outputs: { value: string; scriptPublicKey: string; covenant?: unknown }[] };

beforeAll(async () => {
  key = unwrap(await lottery.call('quickWalletNew'));
  spk = unwrap(await lottery.call('scriptOf', key.address));
  const ks = new QuickKeyStore(new MemoryKV(), 'testnet-10');
  await ks.replace(null, key);
  coins = need(fx['claim k=4']).funds.coins.map((c) => ({ ...c, scriptPublicKey: spk }));
  quick = new QuickWalletAdapter(lottery, ks, 'testnet-10', () => Promise.resolve(coins), key);
}, 120_000);

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
  it('path a (a pool past its claim deadline) and path b (from T + D_RET): offered once the win is claimed, as shown, accepted', async () => {
    for (const [vdaa, path] of [
      ['7500100', 'withPool'],
      ['13100000', 'alone'],
    ] as const) {
      const w = await world('claim k=4');
      const env = { ctx: w.ctx, wallet: quick, address: key.address };
      // While index 0 is claimable no return is offered, on either path (F4 review M1): it would forfeit the prize.
      w.sim.vdaa = vdaa;
      const before = await loadTickets(w.ctx, w.store);
      expect(before.cards[0]?.card?.combos[0]?.status).toBe('claimable');
      expect(before.cards[0]?.card?.returnable).toBeUndefined();
      // The win claimed (PAY), the ticket's successor holds a claimed index and a non-winning one.
      w.sim.vdaa = '1000';
      const c0 = need((await loadTickets(w.ctx, w.store)).cards[0]);
      const pc = await prepareClaim(env, need(c0.chain), 0);
      await sendClaim({ ...env, ...fast }, w.store, need(c0.chain), pc, () => undefined);
      w.sim.vdaa = vdaa;
      const data = await loadTickets(w.ctx, w.store);
      const c = need(data.cards[0]);
      expect(c.card?.returnable?.path).toBe(path);
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

  // F6 security review: the file files each ticket under its outpoint; a ticket this browser already follows under
  // another key (its purchase's, after a claim) is matched by its entry's outpoint and keeps its own record.
  // Control: into an empty store the same file keeps the ticket, under its outpoint.
  it('an imported ticket is keyed by its outpoint and never kept twice', async () => {
    const w = await world('claim k=4');
    const text = await writeBackup(w.ctx, w.store);
    const held = new WebStore(INFO, new MemoryKV());
    await held.putTicket({ ...w.ticket, key: `${'ee'.repeat(32)}:0` });
    await importBackup(w.ctx, held, text);
    const kept = await held.tickets(w.round.covenantId);
    expect(kept.map((t) => t.key)).toEqual([`${'ee'.repeat(32)}:0`]);
    const empty = new WebStore(INFO, new MemoryKV());
    await importBackup(w.ctx, empty, text);
    const u = w.ticket.ticket.utxo;
    expect((await empty.tickets(w.round.covenantId)).map((t) => t.key)).toEqual([
      `${u.transactionId.toLowerCase()}:${u.index}`,
    ]);
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
    // Before the pool's claim deadline its absence is unexplained: the copy is not kept (taken again later).
    const empty = new WebStore(INFO, new MemoryKV());
    const early = await importBackup(w.ctx, empty, text);
    expect(early).toMatchObject({ ticketsKept: 0, ticketsSpent: 1 });
    expect(early.rounds[w.round.round]).toBe('open');
    // After it, the pool was absorbed: nothing on chain.
    w.sim.vdaa = '7500100';
    const r = await importBackup(w.ctx, new WebStore(INFO, new MemoryKV()), text);
    expect(r.rounds[w.round.round]).toBe('gone');
    expect(await empty.rounds(w.round.covenantId)).toEqual([]);
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
