import { describe, expect, it } from 'vitest';
import { readConfig, type ConfigEnv } from './config';

const COV = 'ab'.repeat(32);
const base: ConfigEnv = { VITE_INDEXER_URL: 'http://127.0.0.1:8790/v1', VITE_COVENANT_ID: COV };

describe('config (spec 6.5, 6.6)', () => {
  it('defaults to testnet-10 and normalises the indexer URL', () => {
    const r = readConfig(base, false);
    if (!('config' in r)) throw new Error(r.problems.join(', '));
    expect(r.config.network).toBe('testnet-10');
    expect(r.config.indexerUrl).toBe('http://127.0.0.1:8790/v1/');
    expect(r.config.nodeUrl).toBeUndefined();
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
});
