import { describe, expect, it } from 'vitest';
import type { StoredRound, StoredTicket, WebstoreInfo } from '@wasm/web.js';
import { MemoryKV } from './kv';
import { roundKey, ticketKey, WebStore } from './webstore';

const info: WebstoreInfo = {
  format: 'webstore/2',
  reads: ['webstore/1'],
  database: 'kaspalionare',
  version: 2,
  stores: ['meta', 'tickets', 'rounds', 'pending'],
  walletDatabase: 'kaspalionare-wallet',
};

function ticket(cov: string, txid: string): StoredTicket {
  return {
    key: ticketKey(txid, 1),
    covenantId: cov,
    network: 'simnet',
    round: '3',
    shardIndex: 4,
    ticket: {
      utxo: {
        transactionId: txid,
        index: 1,
        amount: '13000000',
        scriptPublicKey: '0000aa',
        blockDaaScore: '9',
        isCoinbase: false,
        covenantId: cov,
      },
      state: '00',
    },
    bundle: [{ main: [1, 2, 3, 4, 5], bonus: 1 }],
    payout: 'kaspasim:q',
    buy: {
      transactionId: txid,
      price: '100000000',
      deposit: '13000000',
      depositBack: '1',
      fee: '1',
      referral: { amount: '5000000', paid: false, address: undefined },
    },
    savedMs: '1',
  };
}

function round(cov: string, stage: StoredRound['stage']): StoredRound {
  return {
    key: roundKey(cov, '3'),
    covenantId: cov,
    network: 'simnet',
    round: '3',
    m: 2,
    k: 16,
    stage,
    purchases: Array<string>(16).fill(''),
    checkedMs: '1',
  };
}

describe('local store webstore/2 (spec 6.3)', () => {
  it('writes its format once, keeps tickets and rounds per instance', async () => {
    const kv = new MemoryKV();
    const s = new WebStore(info, kv, () => 42);
    await s.putTicket(ticket('aa', '11'));
    await s.putTicket(ticket('bb', '22'));
    expect(kv.data.get('meta')?.get('format')).toEqual({ format: 'webstore/2', createdMs: '42' });
    expect((await s.tickets('aa')).map((t) => t.key)).toEqual(['11:1']);
    await s.putRound(round('aa', 'open'));
    expect((await s.round('aa', '3'))?.purchases).toHaveLength(16);
    expect(await s.round('bb', '3')).toBeUndefined();
  });

  it('refuses a store of another format', async () => {
    const kv = new MemoryKV();
    await kv.put('meta', 'format', { format: 'webstore/9', createdMs: '1' });
    await expect(new WebStore(info, kv).tickets('aa')).rejects.toMatchObject({ code: 'store_format' });
  });

  it('migrates an F3 webstore/1 store in place: its records read unchanged, the meta record says webstore/2', async () => {
    // A store as F3 wrote it: webstore/1 meta, a ticket with shardIndex and buy, an open round copy.
    const kv = new MemoryKV();
    await kv.put('meta', 'format', { format: 'webstore/1', createdMs: '5' });
    const old = ticket('aa', '11');
    await kv.put('tickets', old.key, old);
    await kv.put('rounds', roundKey('aa', '3'), round('aa', 'open'));
    const s = new WebStore(info, kv);
    expect(await s.tickets('aa')).toEqual([old]);
    expect((await s.round('aa', '3'))?.stage).toBe('open');
    expect(await s.format()).toBe('webstore/2');
    expect(kv.data.get('meta')?.get('format')).toEqual({ format: 'webstore/2', createdMs: '5' });
    expect(await s.pending('aa')).toEqual([]);
  });

  it('marks F3 copies past the open stage open, so they are copied again (F4 review note)', async () => {
    const kv = new MemoryKV();
    await kv.put('meta', 'format', { format: 'webstore/1', createdMs: '5' });
    const closed = { ...round('aa', 'closed'), key: roundKey('aa', '4'), round: '4' };
    const drawn = { ...round('aa', 'drawn'), key: roundKey('aa', '5'), round: '5' };
    const marker = { role: 'marker' as const, state: '00', scriptPublicKey: '0000aa' };
    const kept = { ...round('aa', 'closed'), key: roundKey('aa', '6'), round: '6', against: marker };
    for (const r of [closed, drawn, kept]) await kv.put('rounds', r.key, r);
    const s = new WebStore(info, kv);
    expect((await s.round('aa', '4'))?.stage).toBe('open');
    expect((await s.round('aa', '5'))?.stage).toBe('open');
    expect((await s.round('aa', '6'))?.stage).toBe('closed');
    expect((await s.round('aa', '4'))?.purchases).toHaveLength(16);
  });
});
