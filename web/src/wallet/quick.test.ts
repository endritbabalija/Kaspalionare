// @vitest-environment node
// F2/F3 review S1 and S2 (the reviewer's probe web/src/review/f3probe.test.ts, inverted), on the real dev wasm
// through the page's LotteryClient. Never prints a secret key.
import { describe, expect, it } from 'vitest';
import type { BuiltTx, BuyInput } from '@wasm/web.js';
import { unwrap } from '@/lib/errors';
import { captureReferral, referrerFor, validReferrer } from '@/referral/referral';
import { MemoryKV } from '@/store/kv';
import fixtures from '@/test/fixtures/page-inputs.json';
import { wasmClient } from '@/test/wasmWorker';
import { QuickKeyStore, QuickWalletAdapter, createQuickKey, importQuickKey } from './quick';

const lottery = wasmClient('testnet-10');

describe('S1: no quick wallet key is lost or swapped silently', () => {
  it('an import does not replace the stored key unless the player was shown it in the same flow', async () => {
    const store = new QuickKeyStore(new MemoryKV(), 'testnet-10');
    const k1 = await createQuickKey(lottery, store);
    const adapter = new QuickWalletAdapter(lottery, store, 'testnet-10', () => Promise.resolve([]), k1);
    const k2 = unwrap(await lottery.call('quickWalletNew'));
    // Another tab, or the Import form, without the replace step: refused, k1 stays.
    await expect(importQuickKey(lottery, store, k2.secretKey, null)).rejects.toMatchObject({ code: 'wallet_changed' });
    expect((await store.load())?.address).toBe(k1.address);
    // Naming a key that is not the stored one is refused too.
    await expect(importQuickKey(lottery, store, k2.secretKey, k2.address)).rejects.toMatchObject({
      code: 'wallet_changed',
    });
    expect((await store.load())?.address).toBe(k1.address);
    // The same key again is simply used.
    expect((await importQuickKey(lottery, store, k1.secretKey.toUpperCase(), null)).address).toBe(k1.address);
    // A second create while a key exists is refused.
    await expect(createQuickKey(lottery, store)).rejects.toMatchObject({ code: 'wallet_changed' });
    expect((await store.load())?.address).toBe(k1.address);
    // The explicit replace (k1 was shown): stored.
    await importQuickKey(lottery, store, k2.secretKey, k1.address);
    expect((await store.load())?.address).toBe(k2.address);
    // The adapter connected with k1 keeps k1's address (the session's payout) and signs nothing now.
    expect(await adapter.address()).toBe(k1.address);
    const built = { sign: [{ index: 0, sighashType: 1 }] } as unknown as BuiltTx;
    await expect(adapter.signInputs(built, [0])).rejects.toMatchObject({ code: 'wallet_changed' });
  });

  it('two tabs creating at once: one key is kept, the other tab is told (compare and swap)', async () => {
    const store = new QuickKeyStore(new MemoryKV(), 'testnet-10');
    const r = await Promise.allSettled([createQuickKey(lottery, store), createQuickKey(lottery, store)]);
    expect(r.filter((x) => x.status === 'fulfilled')).toHaveLength(1);
    const lostOne = r.find((x) => x.status === 'rejected') as PromiseRejectedResult;
    expect(lostOne.reason).toMatchObject({ code: 'wallet_changed' });
    const kept = (r.find((x) => x.status === 'fulfilled') as PromiseFulfilledResult<{ address: string }>).value;
    expect((await store.load())?.address).toBe(kept.address);
  });

  it('the adapter signs only inputs of its own key', async () => {
    const store = new QuickKeyStore(new MemoryKV(), 'testnet-10');
    const k1 = await createQuickKey(lottery, store);
    const adapter = new QuickWalletAdapter(lottery, store, 'testnet-10', () => Promise.resolve([]), k1);
    const other = unwrap(await lottery.call('quickWalletNew'));
    const built = { sign: [{ index: 0, sighashType: 1, address: other.address }] } as unknown as BuiltTx;
    await expect(adapter.signInputs(built, [0])).rejects.toMatchObject({ code: 'wallet_changed' });
  });
});

describe('referral parsing (the reviewer probe, as assertions)', () => {
  it('refuses values that are not an address of this network; a stored foreign value is not used', async () => {
    const k = unwrap(await lottery.call('quickWalletNew'));
    const a = k.address;
    expect(await validReferrer(lottery, a)).toBe(true);
    for (const bad of [
      `<img src=x onerror=alert(1)>`,
      'javascript:alert(1)',
      a + 'q'.repeat(200),
      a.replace('kaspatest:', 'kaspasim:'),
      a.slice(0, 20) + '​' + a.slice(20),
      a.replace(':', '：'),
    ]) {
      expect(await validReferrer(lottery, bad)).toBe(false);
    }
    const store = new Map<string, string>();
    const s = { getItem: (x: string) => store.get(x) ?? null, setItem: (x: string, y: string) => void store.set(x, y) };
    expect((await captureReferral(lottery, `?ref=${encodeURIComponent(a)}`, s)).kind).toBe('stored');
    store.set('kl.ref', 'kaspa:qqqq');
    expect(await referrerFor(lottery, undefined, s)).toBeUndefined();
  });
});

describe('S2: a self-referral (design 5.1) is the referral row paid to the own address', () => {
  it('buyFigures accepts it, with the same total rule as any referrer', async () => {
    const fx = fixtures as unknown as Record<string, BuyInput>;
    const k = unwrap(await lottery.call('quickWalletNew'));
    const spk = unwrap(await lottery.call('scriptOf', k.address));
    for (const name of ['buy b=1 ref', 'buy b=4']) {
      const base = fx[name] as BuyInput;
      const own: BuyInput = {
        ...base,
        payout: k.address,
        referrer: k.address,
        funds: {
          ...base.funds,
          change: k.address,
          coins: base.funds.coins.map((c) => ({ ...c, scriptPublicKey: spk })),
        },
      };
      const built = unwrap(await lottery.call('buildBuy', own));
      const f = unwrap(
        await lottery.call('buyFigures', {
          covenantId: own.covenantId,
          shard: own.shard,
          bundle: own.bundle,
          payout: own.payout,
          referrer: k.address,
          built,
        }),
      );
      expect(f.referral.paid).toBe(true);
      expect(f.referral.address).toBe(k.address);
      expect(BigInt(f.total)).toBe(BigInt(f.price) + BigInt(f.deposit) + BigInt(f.fee));
    }
  });
});
