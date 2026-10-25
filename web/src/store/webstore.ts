// The local store (spec 6.3, format webstore/2): the tickets this browser bought or found by payout
// (outpoint, full state bytes R1, the node's entry), the copy of each round they are in (the compact purchase
// lists of all shards, checked against the chain; the frozen-roots object and the round result once closed
// and drawn), and the purchases whose outcome the page has not learnt yet (`pending`). The record types and
// the database layout come from crates/web (src/webstore.rs, `webstoreInfo`): no hand-written copy.
// Migration (F4): a webstore/1 store (F3) reads as webstore/2 unchanged (the new fields are optional); the
// database upgrade adds the `pending` store and the meta record's format is rewritten in place.
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
    this.ready ??= this.kv
      .update<StoreMeta>('meta', 'format', (m) => {
        if (!m) return { format: this.info.format, createdMs: String(this.now()) };
        if (m.format === this.info.format) return m;
        if (this.info.reads.includes(m.format)) return { ...m, format: this.info.format };
        throw new PageError('store_format', `the local store is ${m.format}, this page reads ${this.info.format}`);
      })
      .catch((e: unknown) => {
        this.ready = null;
        throw e;
      });
    return this.ready;
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

const STAGE_ORDER = { open: 0, closed: 1, drawn: 2 } as const;

/** Whether `next` is a later stage of a round's copy than `prev` (a copy never moves back). */
export function laterStage(prev: StoredRound | undefined, next: StoredRound): boolean {
  return !prev || STAGE_ORDER[next.stage] >= STAGE_ORDER[prev.stage];
}
