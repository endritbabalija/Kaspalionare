// The local store (spec 6.3, format webstore/2): the tickets this browser bought or found by payout
// (outpoint, full state bytes R1, the node's entry), the copy of each round they are in (the compact purchase
// lists of all shards, checked against the chain; the frozen-roots object and the round result once closed
// and drawn), and the purchases whose outcome the page has not learnt yet (`pending`). The record types and
// the database layout come from crates/web (src/webstore.rs, `webstoreInfo`): no hand-written copy.
// Migration (F4): a webstore/1 store (F3) reads as webstore/2 unchanged (the new fields are optional); the
// database upgrade adds the `pending` store and the meta record's format is rewritten in place. An F3 copy
// at the closed or drawn stage has no marker or round result (F4 fields): the migration marks it open, so it
// is taken again from the chain (F4 review note); its lists are kept.
import type { StoreMeta, StoredPending, StoredRound, StoredTicket, WebstoreInfo } from '@wasm/web.js';
import { PageError } from '@/lib/errors';
import { IdbKV, type KV } from './kv';

export class WebStore {
  private ready: Promise<void> | null = null;

  constructor(
    readonly info: WebstoreInfo,
    private readonly kv: KV,
    private readonly now: () => number = Date.now,
  ) {}

  static browser(info: WebstoreInfo): WebStore {
    return new WebStore(info, new IdbKV(info.database, info.version, info.stores));
  }

  /**
   * The meta record: written on first use; a store of an earlier format this page reads (`info.reads`) is
   * migrated in place; any other format is refused (store_format).
   */
  private ensure(): Promise<void> {
    let migrated = false;
    this.ready ??= this.kv
      .update<StoreMeta>('meta', 'format', (m) => {
        if (!m) return { format: this.info.format, createdMs: String(this.now()) };
        if (m.format === this.info.format) return m;
        if (this.info.reads.includes(m.format)) {
          migrated = true;
          return { ...m, format: this.info.format };
        }
        throw new PageError('store_format', `the local store is ${m.format}, this page reads ${this.info.format}`);
      })
      .then(async () => {
        if (migrated) await this.migrateRounds();
      })
      .catch((e: unknown) => {
        this.ready = null;
        throw e;
      });
    return this.ready;
  }

  /** F3 copies past the open stage lack what F4 checks them with: marked open, so they are copied again. */
  private async migrateRounds(): Promise<void> {
    for (const r of await this.kv.all<StoredRound>('rounds')) {
      const lacks = (r.stage === 'closed' && !r.against) || (r.stage === 'drawn' && !r.result);
      if (lacks) await this.kv.put('rounds', r.key, { ...r, stage: 'open' });
    }
  }

  async format(): Promise<string | undefined> {
    await this.ensure();
    return (await this.kv.get<StoreMeta>('meta', 'format'))?.format;
  }

  async putTicket(t: StoredTicket): Promise<void> {
    await this.ensure();
    await this.kv.put('tickets', t.key, t);
  }

  async ticket(key: string): Promise<StoredTicket | undefined> {
    await this.ensure();
    return this.kv.get<StoredTicket>('tickets', key);
  }

  async tickets(covenantId: string): Promise<StoredTicket[]> {
    await this.ensure();
    return (await this.kv.all<StoredTicket>('tickets')).filter((t) => t.covenantId === covenantId);
  }

  async putRound(r: StoredRound): Promise<void> {
    await this.ensure();
    await this.kv.put('rounds', r.key, r);
  }

  async round(covenantId: string, round: string): Promise<StoredRound | undefined> {
    await this.ensure();
    return this.kv.get<StoredRound>('rounds', roundKey(covenantId, round));
  }

  async rounds(covenantId: string): Promise<StoredRound[]> {
    await this.ensure();
    return (await this.kv.all<StoredRound>('rounds')).filter((r) => r.covenantId === covenantId);
  }

  async putPending(p: StoredPending): Promise<void> {
    await this.ensure();
    await this.kv.put('pending', p.key, p);
  }

  async pending(covenantId: string): Promise<StoredPending[]> {
    await this.ensure();
    return (await this.kv.all<StoredPending>('pending')).filter((p) => p.covenantId === covenantId);
  }

  async delPending(key: string): Promise<void> {
    await this.ensure();
    await this.kv.del('pending', key);
  }
}

export function ticketKey(transactionId: string, index: number): string {
  return `${transactionId}:${index}`;
}

export function roundKey(covenantId: string, round: string): string {
  return `${covenantId}:${round}`;
}
