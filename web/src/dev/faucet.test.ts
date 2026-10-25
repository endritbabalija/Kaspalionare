import { describe, expect, it } from 'vitest';
import { requestFaucet } from './faucet';

const ok = (body: unknown, status = 200) =>
  (async () =>
    new Response(JSON.stringify(body), {
      status,
      headers: { 'content-type': 'application/json' },
    })) as unknown as typeof fetch;

describe('dev faucet client', () => {
  it('posts the address and reads the payment', async () => {
    let seen = '';
    const f = (async (u: string, init: RequestInit) => {
      seen = `${init.method} ${u}`;
      return new Response(JSON.stringify({ transactionId: 'ab'.repeat(32), amount: '10000000000' }), { status: 200 });
    }) as unknown as typeof fetch;
    const p = await requestFaucet('http://127.0.0.1:8791/faucet', 'kaspasim:qq', f);
    expect(p.amount).toBe('10000000000');
    expect(seen).toBe('POST http://127.0.0.1:8791/faucet?address=kaspasim%3Aqq');
  });
  it('refuses other networks before any request', async () => {
    await expect(requestFaucet('http://x/faucet', 'kaspatest:qq', ok({}))).rejects.toThrow(/kaspasim/);
    await expect(requestFaucet('http://x/faucet', 'kaspa:qq', ok({}))).rejects.toThrow(/kaspasim/);
  });
  it('reports the faucet error', async () => {
    await expect(requestFaucet('http://x/faucet', 'kaspasim:qq', ok({ error: 'no coin' }, 400))).rejects.toThrow(
      'faucet: no coin',
    );
    await expect(
      requestFaucet('http://x/faucet', 'kaspasim:qq', ok({ transactionId: 'zz', amount: '1' })),
    ).rejects.toThrow('malformed');
  });
});
