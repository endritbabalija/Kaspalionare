import { describe, expect, it } from 'vitest';
import { Serial } from './serial';

describe('Serial (one wallet request at a time)', () => {
  it('runs requests in order, and a request that throws does not stop the queue', async () => {
    const s = new Serial(1000);
    const seen: number[] = [];
    const a = s.run(async () => {
      await new Promise((r) => setTimeout(r, 20));
      seen.push(1);
      return 1;
    });
    const b = s.run(() => Promise.reject(new Error('no')));
    const c = s.run(async () => {
      seen.push(3);
      return 3;
    });
    expect(await a).toBe(1);
    await expect(b).rejects.toThrow('no');
    expect(await c).toBe(3);
    expect(seen).toEqual([1, 3]);
  });

  it('gives up on a request that never settles (wallet_timeout) and moves on (F2/F3 review note)', async () => {
    const s = new Serial(30);
    const stuck = s.run(() => new Promise<never>(() => undefined));
    const next = s.run(async () => 'next');
    await expect(stuck).rejects.toMatchObject({ code: 'wallet_timeout' });
    expect(await next).toBe('next');
  });
});
