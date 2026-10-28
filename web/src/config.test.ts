import { describe, expect, it } from 'vitest';
import { readConfig, TESTNET_FAUCET_LINK, type ConfigEnv } from './config';

const COV = 'ab'.repeat(32);
const base: ConfigEnv = { VITE_INDEXER_URL: 'http://127.0.0.1:8790/v1', VITE_COVENANT_ID: COV };

describe('config (spec 6.5, 6.6)', () => {
  it('defaults to testnet-10 and normalises the indexer URL', () => {
    const r = readConfig(base, false);
    if (!('config' in r)) throw new Error(r.problems.join(', '));
    expect(r.config.network).toBe('testnet-10');
    expect(r.config.indexerUrls).toEqual(['http://127.0.0.1:8790/v1/']);
    expect(r.config.nodeUrl).toBeUndefined();
  });

  it('takes several indexer URLs, in order (F6 part E), and refuses a bad or repeated one', () => {
    const r = readConfig({ ...base, VITE_INDEXER_URL: 'https://a.example/v1, https://b.example/v1/' }, false);
    if (!('config' in r)) throw new Error(r.problems.join(', '));
    expect(r.config.indexerUrls).toEqual(['https://a.example/v1/', 'https://b.example/v1/']);
    expect(
      'problems' in readConfig({ ...base, VITE_INDEXER_URL: 'https://a.example/v1/,https://a.example/v1' }, false),
    ).toBe(true);
    expect('problems' in readConfig({ ...base, VITE_INDEXER_URL: 'https://a.example/v1/,javascript:x' }, false)).toBe(
      true,
    );
    expect('problems' in readConfig({ ...base, VITE_INDEXER_URL: ' , ' }, false)).toBe(true);
  });

  it('allows the simnet and its faucet in development only', () => {
    const env: ConfigEnv = { ...base, VITE_NETWORK: 'simnet', VITE_FAUCET_URL: 'http://127.0.0.1:8791/faucet' };
    const dev = readConfig(env, true);
    if (!('config' in dev)) throw new Error(dev.problems.join(', '));
    expect(dev.config.faucetUrl).toBe('http://127.0.0.1:8791/faucet');
    expect('problems' in readConfig(env, false)).toBe(true);
    const tn = readConfig({ ...base, VITE_FAUCET_URL: 'http://x/faucet' }, true);
    if (!('config' in tn)) throw new Error(tn.problems.join(', '));
    expect(tn.config.faucetUrl).toBeUndefined();
  });

  it('refuses mainnet, a bad instance and a referral address off testnet', () => {
    expect('problems' in readConfig({ ...base, VITE_NETWORK: 'mainnet' }, true)).toBe(true);
    expect('problems' in readConfig({ ...base, VITE_COVENANT_ID: 'xyz' }, false)).toBe(true);
    expect('problems' in readConfig({ ...base, VITE_REFERRAL_ADDRESS: 'kaspa:qq' }, false)).toBe(true);
    expect('problems' in readConfig({ VITE_COVENANT_ID: COV }, false)).toBe(true);
    expect('problems' in readConfig({ ...base, VITE_REFERRAL_ADDRESS: 'kaspasim:qq' }, false)).toBe(true);
    expect('problems' in readConfig({ ...base, VITE_FAUCET_LINK: 'http://faucet.example' }, false)).toBe(true);
  });

  it('takes a kaspasim: site owner on the simnet dev run only', () => {
    const r = readConfig({ ...base, VITE_NETWORK: 'simnet', VITE_REFERRAL_ADDRESS: 'kaspasim:qq' }, true);
    if (!('config' in r)) throw new Error(r.problems.join(', '));
    expect(r.config.referralAddress).toBe('kaspasim:qq');
    expect(
      'problems' in readConfig({ ...base, VITE_NETWORK: 'simnet', VITE_REFERRAL_ADDRESS: 'kaspatest:qq' }, true),
    ).toBe(true);
  });

  it('links the testnet-10 faucet by default (owner, 2026-10-26); none on the simnet, which has its dev faucet', () => {
    const tn = readConfig(base, false);
    if (!('config' in tn)) throw new Error(tn.problems.join(', '));
    expect(tn.config.faucetLink).toBe(TESTNET_FAUCET_LINK);
    expect(TESTNET_FAUCET_LINK).toBe('https://faucet-tn10.kaspanet.io');
    const other = readConfig({ ...base, VITE_FAUCET_LINK: 'https://faucet.example/tn10' }, false);
    if (!('config' in other)) throw new Error(other.problems.join(', '));
    expect(other.config.faucetLink).toBe('https://faucet.example/tn10');
    const sim = readConfig({ ...base, VITE_NETWORK: 'simnet' }, true);
    if (!('config' in sim)) throw new Error(sim.problems.join(', '));
    expect(sim.config.faucetLink).toBeUndefined();
  });

  it('takes an https explorer only (F2/F3 review note)', () => {
    for (const bad of [
      'javascript:alert(1)',
      'http://explorer.example',
      'data:text/html,x',
      'explorer.example',
      'https://u:p@x.example',
    ]) {
      expect('problems' in readConfig({ ...base, VITE_EXPLORER_URL: bad }, false)).toBe(true);
    }
    const ok = readConfig({ ...base, VITE_EXPLORER_URL: 'https://explorer-tn10.kaspa.org' }, false);
    if (!('config' in ok)) throw new Error(ok.problems.join(', '));
    expect(ok.config.explorerUrl).toBe('https://explorer-tn10.kaspa.org');
  });

  it('a built testnet-10 page takes a wss:// node only (its CSP refuses ws://, check-csp)', () => {
    expect('problems' in readConfig({ ...base, VITE_NODE_URL: 'ws://node.example/tn10' }, false)).toBe(true);
    const ok = readConfig({ ...base, VITE_NODE_URL: 'wss://node.example/tn10' }, false);
    expect('config' in ok).toBe(true);
    const dev = readConfig({ ...base, VITE_NODE_URL: 'ws://127.0.0.1:17210' }, true);
    expect('config' in dev).toBe(true);
  });
});
