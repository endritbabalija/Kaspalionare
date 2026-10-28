import { describe, expect, it } from 'vitest';
import { explorerLink } from './explorer';

const tx = '1b6cc6eb87f2aeeb3d9e824621482e2e9eb102c1f34421ca6aaf2b1e05408caf';

describe('explorerLink', () => {
  it('names no link without an explorer', () => {
    expect(explorerLink(undefined, 'transaction', tx)).toBeUndefined();
    expect(explorerLink('', 'transaction', tx)).toBeUndefined();
  });

  it('uses the kaspa.org layout by default', () => {
    expect(explorerLink('https://explorer-tn10.kaspa.org/', 'transaction', tx)).toBe(
      `https://explorer-tn10.kaspa.org/txs/${tx}`,
    );
    expect(explorerLink('https://explorer-tn10.kaspa.org', 'block', tx)).toBe(
      `https://explorer-tn10.kaspa.org/blocks/${tx}`,
    );
  });

  it('uses /transactions/ on kaspa.stream, the same address and block paths', () => {
    expect(explorerLink('https://tn10.kaspa.stream', 'transaction', tx)).toBe(
      `https://tn10.kaspa.stream/transactions/${tx}`,
    );
    expect(explorerLink('https://tn10.kaspa.stream/', 'address', 'kaspatest:qz')).toBe(
      'https://tn10.kaspa.stream/addresses/kaspatest%3Aqz',
    );
    expect(explorerLink('https://tn10.kaspa.stream', 'block', tx)).toBe(`https://tn10.kaspa.stream/blocks/${tx}`);
  });

  it('does not treat a look-alike host as kaspa.stream', () => {
    expect(explorerLink('https://notkaspa.stream.example', 'transaction', tx)).toBe(
      `https://notkaspa.stream.example/txs/${tx}`,
    );
    expect(explorerLink('https://evilkaspa.stream', 'transaction', tx)).toBe(`https://evilkaspa.stream/txs/${tx}`);
  });
});
