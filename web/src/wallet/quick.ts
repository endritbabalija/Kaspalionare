// The quick testnet wallet (spec 6.2, S7): a key made in this browser by the wasm (`quickWalletNew`), kept in
// IndexedDB (database `kaspalionare-wallet`, never part of the local store or its backup), exportable and
// importable. It signs with the wasm (`quickWalletSign`: only the listed inputs it owns), and its answer goes
// through the same post-sign check as any wallet's (`assemble`). Absent from a mainnet page build
// (wallet/quickEnabled.ts; scripts/check-mainnet.mjs) as from a mainnet wasm (W4).
import type { BuiltTx, QuickKey, StoredQuickKey, Utxo, WebstoreInfo } from '@wasm/web.js';
import { PageError, unwrap } from '@/lib/errors';
import type { LotteryClient } from '@/worker/client';
import { IdbKV, type KV } from '@/store/kv';
import { Serial } from './serial';
import { signItems, type WalletAdapter } from './types';

/** Searched for by scripts/check-mainnet.mjs (absent from a mainnet page build). */
export const QUICK_WALLET_MARKER = 'KASPALIONARE_PAGE_QUICK_WALLET';

const KEYS = 'keys';
const QUICK = 'quick';

/** The quick wallet's key record, in its own database. */
export class QuickKeyStore {
  constructor(
    private readonly kv: KV,
    private readonly network: string,
    private readonly now: () => number = Date.now,
  ) {}

  static browser(info: WebstoreInfo, network: string): QuickKeyStore {
    return new QuickKeyStore(new IdbKV(info.walletDatabase, 1, [KEYS]), network);
  }

  async load(): Promise<QuickKey | undefined> {
    const r = await this.kv.get<StoredQuickKey>(KEYS, QUICK);
    return r && r.network === this.network ? r.key : undefined;
  }

  /**
   * Store `key` only while the stored key is still the one at address `expected` (null: none stored), in one
   * transaction (F2/F3 review S1): a key is never overwritten unless the player was shown it in this very flow,
   * and another tab's write in between is wallet_changed. A record of another network counts as a stored key.
   */
  async replace(expected: string | null, key: QuickKey): Promise<void> {
    await this.kv.update<StoredQuickKey>(KEYS, QUICK, (cur) => {
      const now = cur ? cur.key.address : null;
      if (now !== expected) {
        throw new PageError(
          'wallet_changed',
          `the stored key is ${now ? `${cur?.network} …${now.slice(-6)}` : 'none'}, expected ${expected ? `…${expected.slice(-6)}` : 'none'}`,
        );
      }
      return { network: this.network, key, createdMs: String(this.now()) };
    });
  }
}

/** A new key, stored only when this browser holds none (another tab may have made one: wallet_changed). */
export async function createQuickKey(lottery: LotteryClient, store: QuickKeyStore): Promise<QuickKey> {
  const k = unwrap(await lottery.call('quickWalletNew'));
  await store.replace(null, k);
  return k;
}

/**
 * Import a key. The same key as the stored one is simply used. Another key replaces the stored one only when
 * `replacing` names the stored key's address, the one the player was shown (exported) in this flow; else
 * wallet_changed, and the stored key stays (F2/F3 review S1).
 */
export async function importQuickKey(
  lottery: LotteryClient,
  store: QuickKeyStore,
  secretKey: string,
  replacing: string | null,
): Promise<QuickKey> {
  const k = unwrap(await lottery.call('quickWalletImport', secretKey.trim().toLowerCase()));
  const cur = await store.load();
  if (cur?.address === k.address) return cur;
  await store.replace(replacing, k);
  return k;
}

/**
 * The quick wallet as an adapter, pinned to the key it connected with (F2/F3 review S1): its address (the
 * session's payout and change) and every signature are that key's. Before signing it checks that this
 * browser still holds that key; if another tab replaced it, nothing is signed (wallet_changed).
 */
export class QuickWalletAdapter implements WalletAdapter {
  readonly kind = 'quick' as const;
  readonly marker = QUICK_WALLET_MARKER;
  private readonly serial = new Serial();

  constructor(
    private readonly lottery: LotteryClient,
    private readonly store: QuickKeyStore,
    private readonly pageNetwork: string,
    private readonly coins: (address: string) => Promise<Utxo[]>,
    private readonly key: QuickKey,
  ) {}

  connect(): Promise<string> {
    return Promise.resolve(this.key.address);
  }

  network(): Promise<string> {
    return Promise.resolve(this.pageNetwork);
  }

  address(): Promise<string> {
    return Promise.resolve(this.key.address);
  }

  async utxos(): Promise<Utxo[]> {
    return this.coins(this.key.address);
  }

  async signInputs(built: BuiltTx, indices: readonly number[]): Promise<string> {
    signItems(built, indices);
    return this.serial.run(async () => {
      const stored = await this.store.load();
      if (stored?.address !== this.key.address) {
        throw new PageError(
          'wallet_changed',
          `connected …${this.key.address.slice(-6)}, stored ${stored ? `…${stored.address.slice(-6)}` : 'none'}`,
        );
      }
      const other = built.sign.find((s) => s.address && s.address !== this.key.address);
      if (other) throw new PageError('wallet_changed', `input ${other.index} is not this key's`);
      return unwrap(await this.lottery.call('quickWalletSign', { built, secretKey: this.key.secretKey }));
    });
  }
}
