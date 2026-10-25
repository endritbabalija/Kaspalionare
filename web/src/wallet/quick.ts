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

  async save(key: QuickKey): Promise<void> {
    await this.kv.put<StoredQuickKey>(KEYS, QUICK, { network: this.network, key, createdMs: String(this.now()) });
  }

  async forget(): Promise<void> {
    await this.kv.del(KEYS, QUICK);
  }
}

export async function createQuickKey(lottery: LotteryClient, store: QuickKeyStore): Promise<QuickKey> {
  const k = unwrap(await lottery.call('quickWalletNew'));
  await store.save(k);
  return k;
}

export async function importQuickKey(
  lottery: LotteryClient,
  store: QuickKeyStore,
  secretKey: string,
): Promise<QuickKey> {
  const k = unwrap(await lottery.call('quickWalletImport', secretKey.trim().toLowerCase()));
  await store.save(k);
  return k;
}

export class QuickWalletAdapter implements WalletAdapter {
  readonly kind = 'quick' as const;
  readonly marker = QUICK_WALLET_MARKER;
  private readonly serial = new Serial();

  constructor(
    private readonly lottery: LotteryClient,
    private readonly store: QuickKeyStore,
    private readonly pageNetwork: string,
    private readonly coins: (address: string) => Promise<Utxo[]>,
  ) {}

  private async key(): Promise<QuickKey> {
    const k = await this.store.load();
    if (!k) throw new PageError('wallet_missing', 'no quick wallet key in this browser');
    return k;
  }

  async connect(): Promise<string> {
    return (await this.key()).address;
  }

  network(): Promise<string> {
    return Promise.resolve(this.pageNetwork);
  }

  async address(): Promise<string> {
    return (await this.key()).address;
  }

  async utxos(): Promise<Utxo[]> {
    return this.coins(await this.address());
  }

  async signInputs(built: BuiltTx, indices: readonly number[]): Promise<string> {
    signItems(built, indices);
    return this.serial.run(async () => {
      const k = await this.key();
      return unwrap(await this.lottery.call('quickWalletSign', { built, secretKey: k.secretKey }));
    });
  }
}
