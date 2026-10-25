// @vitest-environment node
// Spec 7, 6.6: a ?ref= is validated by the wasm (an address of the page's network) and remembered; anything
// else is refused and the stored one stays; with none, the site owner's address.
import { beforeAll, describe, expect, it } from 'vitest';
import { unwrap } from '@/lib/errors';
import { wasmClient } from '@/test/wasmWorker';
import { captureReferral, refParam, referralLink, referrerFor, REF_KEY } from './referral';

const lottery = wasmClient('testnet-10');
let a = '';
let b = '';

function store() {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), m };
}

beforeAll(async () => {
  a = unwrap(await lottery.call('quickWalletNew')).address;
  b = unwrap(await lottery.call('quickWalletNew')).address;
}, 60_000);

describe('referral links (spec 7)', () => {
  it('stores a valid link, replaces it with another valid one, and refuses the rest', async () => {
    const s = store();
    expect(await captureReferral(lottery, `?ref=${a}`, s)).toEqual({ kind: 'stored', address: a });
    expect(s.m.get(REF_KEY)).toBe(a);
    const other = (prefix: string) => [prefix, a.split(String.fromCharCode(58))[1]].join(String.fromCharCode(58));
    for (const bad of [
      other('kaspa'),
      other('kaspasim'),
      other('kaspadev'),
      '<b>x</b>',
      'kaspatest:',
      'x'.repeat(500),
    ]) {
      const r = await captureReferral(lottery, `?ref=${encodeURIComponent(bad)}`, s);
      expect(r.kind, bad).toBe('refused');
      expect(s.m.get(REF_KEY)).toBe(a);
    }
    expect(await captureReferral(lottery, `?ref=${b}`, s)).toEqual({ kind: 'stored', address: b });
    expect(await captureReferral(lottery, '', s)).toEqual({ kind: 'none' });
    expect(s.m.get(REF_KEY)).toBe(b);
  });

  it("uses the remembered link, else the site owner's address", async () => {
    const s = store();
    expect(await referrerFor(lottery, a, s)).toBe(a);
    s.setItem(REF_KEY, b);
    expect(await referrerFor(lottery, a, s)).toBe(b);
    s.setItem(REF_KEY, 'garbage');
    expect(await referrerFor(lottery, a, s)).toBe(a);
    expect(await referrerFor(lottery, undefined, store())).toBeUndefined();
  });

  it('reads ?ref and writes the link of an address', () => {
    expect(refParam('?x=1&ref=%20abc%20')).toBe('abc');
    expect(refParam('')).toBeNull();
    const link = referralLink(a, 'https://site.example/path?ref=old#/play');
    expect(new URL(link).searchParams.get('ref')).toBe(a);
    expect(new URL(link).hash).toBe('');
  });
});
