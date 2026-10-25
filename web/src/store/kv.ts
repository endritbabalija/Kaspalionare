// A small key-value layer over IndexedDB (the local store, spec 6.3, and the quick wallet's key store).
// Records are structured-cloned as they are (every 64-bit figure is already a decimal string, so nothing is
// lost). `MemoryKV` is the same interface in memory, for unit tests; the e2e suite reads the real IndexedDB.
// Any IndexedDB failure is storage_unavailable.
import { PageError } from '@/lib/errors';

export interface KV {
  get<T>(store: string, key: string): Promise<T | undefined>;
  put<T>(store: string, key: string, value: T): Promise<void>;
  all<T>(store: string): Promise<T[]>;
  del(store: string, key: string): Promise<void>;
  /**
   * Read and write one record in a single transaction (compare and swap): `f` gets the current value and
   * returns the new one, or throws to leave it as it is. No other tab's write can come in between.
   */
  update<T>(store: string, key: string, f: (current: T | undefined) => T): Promise<void>;
}

function req<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(new PageError('storage_unavailable', String(r.error)));
  });
}

export class IdbKV implements KV {
  private db: Promise<IDBDatabase> | null = null;

  constructor(
    readonly name: string,
    readonly version: number,
    readonly stores: readonly string[],
  ) {}

  private open(): Promise<IDBDatabase> {
    this.db ??= new Promise<IDBDatabase>((resolve, reject) => {
      let r: IDBOpenDBRequest;
      try {
        r = indexedDB.open(this.name, this.version);
      } catch (e) {
        reject(new PageError('storage_unavailable', String(e)));
        return;
      }
      r.onupgradeneeded = () => {
        for (const s of this.stores) if (!r.result.objectStoreNames.contains(s)) r.result.createObjectStore(s);
      };
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(new PageError('storage_unavailable', String(r.error)));
      r.onblocked = () => reject(new PageError('storage_unavailable', `${this.name}: open blocked`));
    }).catch((e: unknown) => {
      this.db = null;
      throw e;
    });
    return this.db;
  }

  private async tx(store: string, mode: IDBTransactionMode): Promise<IDBObjectStore> {
    const db = await this.open();
    try {
      return db.transaction(store, mode).objectStore(store);
    } catch (e) {
      throw new PageError('storage_unavailable', String(e));
    }
  }

  async get<T>(store: string, key: string): Promise<T | undefined> {
    return (await req((await this.tx(store, 'readonly')).get(key))) as T | undefined;
  }

  async put<T>(store: string, key: string, value: T): Promise<void> {
    await req((await this.tx(store, 'readwrite')).put(value, key));
  }

  async all<T>(store: string): Promise<T[]> {
    return (await req((await this.tx(store, 'readonly')).getAll())) as T[];
  }

  async del(store: string, key: string): Promise<void> {
    await req((await this.tx(store, 'readwrite')).delete(key));
  }

  async update<T>(store: string, key: string, f: (current: T | undefined) => T): Promise<void> {
    const os = await this.tx(store, 'readwrite');
    await new Promise<void>((resolve, reject) => {
      const t = os.transaction;
      let thrown: unknown;
      const g = os.get(key);
      g.onsuccess = () => {
        try {
          os.put(f(g.result as T | undefined), key);
        } catch (e) {
          thrown = e;
          t.abort();
        }
      };
      t.oncomplete = () => resolve();
      t.onabort = () => reject(thrown ?? new PageError('storage_unavailable', String(t.error)));
      t.onerror = () => reject(thrown ?? new PageError('storage_unavailable', String(t.error)));
    });
  }
}

export class MemoryKV implements KV {
  readonly data = new Map<string, Map<string, unknown>>();

  private s(store: string): Map<string, unknown> {
    let m = this.data.get(store);
    if (!m) this.data.set(store, (m = new Map()));
    return m;
  }

  get<T>(store: string, key: string): Promise<T | undefined> {
    return Promise.resolve(structuredClone(this.s(store).get(key)) as T | undefined);
  }

  put<T>(store: string, key: string, value: T): Promise<void> {
    this.s(store).set(key, structuredClone(value));
    return Promise.resolve();
  }

  all<T>(store: string): Promise<T[]> {
    return Promise.resolve([...this.s(store).values()].map((v) => structuredClone(v) as T));
  }

  del(store: string, key: string): Promise<void> {
    this.s(store).delete(key);
    return Promise.resolve();
  }

  update<T>(store: string, key: string, f: (current: T | undefined) => T): Promise<void> {
    try {
      const next = f(structuredClone(this.s(store).get(key)) as T | undefined);
      this.s(store).set(key, structuredClone(next));
      return Promise.resolve();
    } catch (e) {
      return Promise.reject(e);
    }
  }
}
