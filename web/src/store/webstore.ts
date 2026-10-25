// The local store (spec 6.3, format webstore/1): the tickets this browser bought (outpoint, full state bytes
// R1, the node's entry) and the copy of each round they are in (the compact purchase lists of all shards,
// checked against the chain). The record types and the database layout come from crates/web
// (src/webstore.rs, `webstoreInfo`): no hand-written copy. F4 completes rounds after close(), adds results,
// backup and import.
import type { StoreMeta, StoredRound, StoredTicket, WebstoreInfo } from '@wasm/web.js';
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

  /** The meta record: written on first use; a store of another format is refused (store_format). */
  private ensure(): Promise<void> {
    this.ready ??= (async () => {
      const m = await this.kv.get<StoreMeta>('meta', 'format');
      if (!m) {
        await this.kv.put<StoreMeta>('meta', 'format', { format: this.info.format, createdMs: String(this.now()) });
      } else if (m.format !== this.info.format) {
        throw new PageError('store_format', `the local store is ${m.format}, this page reads ${this.info.format}`);
      }
    })().catch((e: unknown) => {
      this.ready = null;
      throw e;
    });
    return this.ready;
  }

  async putTicket(t: StoredTicket): Promise<void> {
    await this.ensure();
    await this.kv.put('tickets', t.key, t);
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
}

export function ticketKey(transactionId: string, index: number): string {
  return `${transactionId}:${index}`;
}

export function roundKey(covenantId: string, round: string): string {
  return `${covenantId}:${round}`;
}
