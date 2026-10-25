import { describe, expect, it } from 'vitest';
import type { StoredTicket, WebstoreInfo } from '@wasm/web.js';
import { MemoryKV } from './kv';
import { roundKey, ticketKey, WebStore } from './webstore';

const info: WebstoreInfo = {
  format: 'webstore/1',
  database: 'kaspalionare',
  version: 1,
  stores: ['meta', 'tickets', 'rounds'],
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

describe('local store webstore/1 (spec 6.3)', () => {
  it('writes its format once, keeps tickets and rounds per instance', async () => {
    const kv = new MemoryKV();
    const s = new WebStore(info, kv, () => 42);
    await s.putTicket(ticket('aa', '11'));
    await s.putTicket(ticket('bb', '22'));
    expect(kv.data.get('meta')?.get('format')).toEqual({ format: 'webstore/1', createdMs: '42' });
    expect((await s.tickets('aa')).map((t) => t.key)).toEqual(['11:1']);
    await s.putRound({
      key: roundKey('aa', '3'),
      covenantId: 'aa',
      network: 'simnet',
      round: '3',
      m: 2,
      k: 16,
      stage: 'open',
      purchases: Array<string>(16).fill(''),
      checkedMs: '1',
    });
    expect((await s.round('aa', '3'))?.purchases).toHaveLength(16);
    expect(await s.round('bb', '3')).toBeUndefined();
  });

  it('refuses a store of another format', async () => {
    const kv = new MemoryKV();
    await kv.put('meta', 'format', { format: 'webstore/9', createdMs: '1' });
    await expect(new WebStore(info, kv).tickets('aa')).rejects.toMatchObject({ code: 'store_format' });
  });
});
