// @vitest-environment node
// Spec 8 "Post-sign check", through the real page path: the page's own signAndSubmit (play/buyFlow.ts), the
// LotteryClient and the worker's message handler, the real wasm (src/wasm/dev) building and assembling. A
// mock wallet alters each of the spec's eight things in its answer; every one is refused before anything is
// sent (the submit is never called). The honest wallet's answer is sent once. Plus the wallet adapters'
// rules: KasWare always gets the explicit index list, refuses another network, and one request runs at a
// time. The builds come from the fixture inputs of crates/web (src/test/fixtures/page-inputs.json, checked
// current by `cargo test -p web --test api page_fixtures_are_current`), with a fresh quick-wallet key in
// place of the fixture wallet's coins; no key is in the repo.
import { beforeAll, describe, expect, it, vi } from 'vitest';
import type { BuiltTx, BuyInput, CloseInput, QuickKey, Signed } from '@wasm/web.js';
import fixtures from '@/test/fixtures/page-inputs.json';
import { wasmClient } from '@/test/wasmWorker';
import { PageError, unwrap } from '@/lib/errors';
import { signAndSubmit } from '@/play/buyFlow';
import { MemoryKV } from '@/store/kv';
import { KasWareAdapter, type KasWareApi } from './kasware';
import { QuickKeyStore, QuickWalletAdapter } from './quick';
import type { WalletAdapter } from './types';

const fx = fixtures as unknown as { 'buy b=1 ref': BuyInput; 'buy b=4': BuyInput; 'close k=16': CloseInput };
const lottery = wasmClient('testnet-10');

let key: QuickKey;
let spk: string;
let quick: QuickWalletAdapter;
let buyBuilt: BuiltTx;
let closeBuilt: BuiltTx;
const otherSpk = '0000' + '20' + '42'.repeat(32) + 'ac';

beforeAll(async () => {
  key = unwrap(await lottery.call('quickWalletNew'));
  spk = unwrap(await lottery.call('scriptOf', key.address));
  const store = new QuickKeyStore(new MemoryKV(), 'testnet-10');
  await store.replace(null, key);
  quick = new QuickWalletAdapter(lottery, store, 'testnet-10', () => Promise.resolve([]), key);
  const own = <T extends { funds: BuyInput['funds'] }>(i: T): T => ({
    ...i,
    funds: { ...i.funds, change: key.address, coins: i.funds.coins.map((c) => ({ ...c, scriptPublicKey: spk })) },
  });
  buyBuilt = unwrap(await lottery.call('buildBuy', { ...own(fx['buy b=1 ref']), payout: key.address }));
  closeBuilt = unwrap(await lottery.call('buildClose', own(fx['close k=16'])));
}, 120_000);

type Json = Record<string, unknown> & { inputs: Record<string, unknown>[]; outputs: Record<string, unknown>[] };

/** A wallet that signs honestly (the quick wallet), then alters its answer. */
function mock(alter: (v: Json) => void): WalletAdapter {
  return {
    kind: 'quick',
    connect: () => quick.connect(),
    network: () => quick.network(),
    address: () => quick.address(),
    utxos: () => quick.utxos(),
    async signInputs(built, indices) {
      const v = JSON.parse(await quick.signInputs(built, indices)) as Json;
      alter(v);
      return JSON.stringify(v);
    },
  };
}

async function attempt(w: WalletAdapter, built: BuiltTx) {
  const submit = vi.fn((s: Signed) => Promise.resolve(s.id));
  const steps: string[] = [];
  let error: unknown = null;
  try {
    await signAndSubmit(lottery, w, built, submit, (s) => steps.push(s));
  } catch (e) {
    error = e;
  }
  return { submit, steps, error };
}

const last = (v: Json) => v.outputs.length - 1;

const ALTERATIONS: [string, 'buy' | 'close', (v: Json) => void, string][] = [
  ['a covenant signature script', 'buy', (v) => void ((v.inputs[0] as Json).signatureScript = '00'), 'signed_changed'],
  ['an output value', 'buy', (v) => void ((v.outputs[1] as Json).value = '1'), 'signed_changed'],
  ['an output script', 'buy', (v) => void ((v.outputs[2] as Json).scriptPublicKey = otherSpk), 'signed_changed'],
  ['an extra output', 'buy', (v) => void v.outputs.push({ ...(v.outputs[2] as Json) }), 'signed_changed'],
  [
    'the change address',
    'buy',
    (v) => void ((v.outputs[last(v)] as Json).scriptPublicKey = otherSpk),
    'signed_changed',
  ],
  [
    "an unlisted input's signature",
    'close',
    (v) => void ((v.inputs[5] as Json).signatureScript = (v.inputs[18] as Json).signatureScript),
    'signed_changed',
  ],
  ['the lock time', 'buy', (v) => void (v.lockTime = '1'), 'signed_changed'],
  ['the payload', 'buy', (v) => void (v.payload = '00'), 'signed_changed'],
  // Beyond the spec's list: a listed input left unsigned, and a signature that does not verify.
  ['a listed input left unsigned', 'buy', (v) => void ((v.inputs[1] as Json).signatureScript = ''), 'signed_missing'],
  [
    'a signature byte flipped',
    'buy',
    (v) => {
      const s = (v.inputs[1] as Json).signatureScript as string;
      (v.inputs[1] as Json).signatureScript = s.slice(0, 10) + (s[10] === '0' ? '1' : '0') + s.slice(11);
    },
    'signature_invalid',
  ],
];

describe('post-sign check through the page path (spec 6.2, 8)', () => {
  it('the honest wallet passes: checked, then sent once', async () => {
    for (const built of [buyBuilt, closeBuilt]) {
      const r = await attempt(quick, built);
      expect(r.error).toBeNull();
      expect(r.submit).toHaveBeenCalledTimes(1);
      expect(r.submit.mock.calls[0]?.[0].id).toBe(built.id);
      expect(r.steps).toEqual(['signed', 'sent']);
    }
  });

  for (const [what, which, alter, code] of ALTERATIONS) {
    it(`a wallet that alters ${what} is refused before sending (${code})`, async () => {
      const r = await attempt(mock(alter), which === 'buy' ? buyBuilt : closeBuilt);
      expect(r.error).toBeInstanceOf(PageError);
      expect((r.error as PageError).code).toBe(code);
      expect(r.submit).not.toHaveBeenCalled();
      expect(r.steps).toEqual([]);
    });
  }

  it('a tampered built transaction (a figure the page showed) is refused before signing', async () => {
    const lie = { ...buyBuilt, fee: String(BigInt(buyBuilt.fee) + 1n) };
    const r = await attempt(quick, lie);
    expect((r.error as PageError).code).toBe('built_mismatch');
    expect(r.submit).not.toHaveBeenCalled();
  });
});

/** A KasWare stand-in: records each call, signs with the quick wallet's key, refuses overlapping requests. */
function fakeKasware(network = 'kaspa_testnet_10') {
  const calls: { signInputs: { index: number; sighashType: number }[] }[] = [];
  let open = 0;
  const account = { now: key.address };
  const api: KasWareApi = {
    requestAccounts: () => Promise.resolve([account.now]),
    getAccounts: () => Promise.resolve([account.now]),
    getNetwork: () => Promise.resolve(network),
    async signPskt(a) {
      if (open > 0) throw { code: 4001, message: 'please request after current approval resolve' };
      open++;
      try {
        calls.push({ signInputs: a.options.signInputs });
        await new Promise((r) => setTimeout(r, 20));
        const built = [buyBuilt, closeBuilt].find((b) => b.transaction === a.txJsonString);
        if (!built) throw new Error('unknown transaction');
        return await quick.signInputs(
          built,
          a.options.signInputs.map((s) => s.index),
        );
      } finally {
        open--;
      }
    },
  };
  return { api, calls, account };
}

describe('KasWare adapter (F0 result, DECISIONS 2026-10-26)', () => {
  it('always passes the explicit index list, and its answer goes through the post-sign check', async () => {
    const k = fakeKasware();
    const w = new KasWareAdapter(k.api, 'kaspa_testnet_10', () => Promise.resolve([]));
    expect(await w.connect()).toBe(key.address);
    const r = await attempt(w, closeBuilt);
    expect(r.error).toBeNull();
    expect(k.calls).toEqual([{ signInputs: closeBuilt.sign.map((s) => ({ index: s.index, sighashType: 1 })) }]);
    expect(k.calls[0]?.signInputs.map((s) => s.index)).toEqual([18]);
  });

  it('refuses a wallet on another network', async () => {
    for (const net of ['kaspa_mainnet', 'kaspa_testnet_11', 'kaspa_simnet']) {
      const w = new KasWareAdapter(fakeKasware(net).api, 'kaspa_testnet_10', () => Promise.resolve([]));
      await expect(w.connect()).rejects.toMatchObject({ code: 'wallet_network' });
    }
    // On the simnet the extension wallets are refused (the quick wallet only).
    const w = new KasWareAdapter(fakeKasware().api, null, () => Promise.resolve([]));
    await expect(w.connect()).rejects.toMatchObject({ code: 'wallet_network' });
  });

  it('runs one wallet request at a time', async () => {
    const k = fakeKasware();
    const w = new KasWareAdapter(k.api, 'kaspa_testnet_10', () => Promise.resolve([]));
    await w.connect();
    const both = await Promise.all([w.signInputs(buyBuilt, [1]), w.signInputs(closeBuilt, [18])]);
    expect(both.every((s) => typeof s === 'string')).toBe(true);
    expect(k.calls).toHaveLength(2);
  });

  // F6 security review: an account switched in KasWare after connect is refused before anything is signed, with
  // its own code (not a confusing signature_invalid from the post-sign check); connected again, it signs.
  it('refuses to sign after the account was switched in KasWare', async () => {
    const k = fakeKasware();
    const w = new KasWareAdapter(k.api, 'kaspa_testnet_10', () => Promise.resolve([]));
    await w.connect();
    k.account.now = 'kaspatest:qq0000000000000000000000000000000000000000000000000000000000000000';
    await expect(w.signInputs(closeBuilt, [18])).rejects.toMatchObject({ code: 'wallet_account' });
    expect(k.calls).toHaveLength(0);
    k.account.now = key.address;
    expect(typeof (await w.signInputs(closeBuilt, [18]))).toBe('string');
    expect(k.calls).toHaveLength(1);
  });

  it('refuses a sign list that is not the built transaction’s', async () => {
    const w = new KasWareAdapter(fakeKasware().api, 'kaspa_testnet_10', () => Promise.resolve([]));
    await w.connect();
    await expect(w.signInputs(buyBuilt, [0])).rejects.toThrow(/sign indices/);
    await expect(w.signInputs(buyBuilt, [])).rejects.toThrow(/sign indices/);
  });
});
