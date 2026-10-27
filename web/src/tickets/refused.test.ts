// @vitest-environment node
// F6 part A review N1: after a conflict the page never signs or sends again a transaction ID the node has just
// refused (an orphan: an input already spent); it builds again instead (`unrefused`). Through the page's own claim
// code (tickets/*, the shared send path of buys, claims, the vault and the caller panel) and the real dev wasm on
// the stand-in node of src/test/ticketWorld.ts. Each guard has its control (see the handoff).
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import type { QuickKey, Utxo } from '@wasm/web.js';
import { PageError, unwrap } from '@/lib/errors';
import { forgetRefused, refusedLately } from '@/play/buyFlow';
import { MemoryKV } from '@/store/kv';
import { fast, fx, lottery, need, world } from '@/test/ticketWorld';
import { QuickKeyStore, QuickWalletAdapter } from '@/wallet/quick';
import { prepareClaim, sendClaim } from './claimFlow';
import { loadTickets } from './load';
import { UNREFUSED } from './send';

let key: QuickKey;
let coins: Utxo[];
let quick: QuickWalletAdapter;

beforeAll(async () => {
  key = unwrap(await lottery.call('quickWalletNew'));
  const spk = unwrap(await lottery.call('scriptOf', key.address));
  const ks = new QuickKeyStore(new MemoryKV(), 'testnet-10');
  await ks.replace(null, key);
  coins = need(fx['claim k=4']).funds.coins.map((c) => ({ ...c, scriptPublicKey: spk }));
  // The wallet's coins as the node lists them now (a test changes them, as the node catching up does).
  quick = new QuickWalletAdapter(lottery, ks, 'testnet-10', () => Promise.resolve(coins), key);
}, 120_000);

afterEach(() => {
  forgetRefused();
  vi.restoreAllMocks();
});

async function refusedClaim() {
  const w = await world('claim k=4');
  const c = need((await loadTickets(w.ctx, w.store, key.address)).cards[0]);
  const env = { ctx: w.ctx, wallet: quick, address: key.address };
  const p = await prepareClaim(env, need(c.chain), 0);
  // The wallet's fee coin spent by a claim from another browser of the same wallet: refused (an orphan).
  w.sim.gone.add(`${need(coins[0]).transactionId.toLowerCase()}:${need(coins[0]).index}`);
  let sent = 0;
  const submit = (s: Parameters<typeof w.sim.submit>[0]) => {
    sent++;
    return Promise.resolve(w.sim.submit(s));
  };
  await expect(
    sendClaim({ ...env, ...fast, submit }, w.store, need(c.chain), p, () => undefined),
  ).rejects.toMatchObject({
    code: 'node_conflict',
  });
  expect(sent).toBe(1);
  expect(refusedLately(p.built.id)).toBe(true);
  return { w, c, env, p, submit, sent: () => sent };
}

describe('N1: a transaction the node has just refused is never signed or sent again', () => {
  it('pressed again with the same build: refused before the wallet signs, nothing sent', async () => {
    const { w, c, env, p, submit, sent } = await refusedClaim();
    const sign = vi.spyOn(quick, 'signInputs');
    await expect(
      sendClaim({ ...env, ...fast, submit }, w.store, need(c.chain), p, () => undefined),
    ).rejects.toMatchObject({ code: 'node_conflict' });
    expect(sign).not.toHaveBeenCalled();
    expect(sent()).toBe(1);
  });

  it('built again while the node has not caught up: the same ID is not offered; once the coins move, a new build', async () => {
    const { w, c, env, p } = await refusedClaim();
    const pauses: number[] = [];
    const sleep = (ms: number) => {
      pauses.push(ms);
      return Promise.resolve();
    };
    // The node still lists the spent coin: every build is the refused transaction, so none is offered.
    const e: unknown = await prepareClaim({ ...env, sleep }, need(c.chain), 0).catch((x: unknown) => x);
    expect(e).toBeInstanceOf(PageError);
    expect((e as PageError).code).toBe('node_conflict');
    expect(pauses).toHaveLength(UNREFUSED.tries - 1);
    // The node catches up after the first pause (the coin gone, the wallet's change listed): another transaction.
    const before = coins;
    try {
      pauses.length = 0;
      const moved = { ...need(coins[0]), transactionId: 'ab'.repeat(32) };
      const later = (ms: number) => {
        coins = [moved];
        return sleep(ms);
      };
      const q = await prepareClaim({ ...env, sleep: later }, need(c.chain), 0);
      expect(q.built.id).not.toBe(p.built.id);
      expect(pauses).toHaveLength(1);
      expect(w.sim.submitted).toEqual([]);
    } finally {
      coins = before;
    }
  });
});
