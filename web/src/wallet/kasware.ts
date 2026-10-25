// KasWare (spec 6.2; F0 result, DECISIONS 2026-10-26): `signPskt({txJsonString, options: {signInputs}})`
// always with the explicit index list (without one KasWare signs every input it can and overwrites the
// covenant signature scripts); never `pushPskt` (not exposed; the page submits itself after `assemble`).
// Network `kaspa_testnet_10` only. Requests are serialised (KasWare refuses overlapping ones).
import type { BuiltTx, Utxo } from '@wasm/web.js';
import { PageError } from '@/lib/errors';
import { Serial } from './serial';
import { signItems, type WalletAdapter } from './types';

/** The part of `window.kasware` the page uses (docs.kasware.xyz, read 2026-10-25; F0). */
export interface KasWareApi {
  requestAccounts(): Promise<string[]>;
  getAccounts(): Promise<string[]>;
  getNetwork(): Promise<string>;
  signPskt(a: {
    txJsonString: string;
    options: { signInputs: { index: number; sighashType: number }[] };
  }): Promise<string>;
}

export function kaswareApi(w: unknown = globalThis): KasWareApi | undefined {
  const k = (w as { kasware?: unknown }).kasware;
  if (!k || typeof k !== 'object') return undefined;
  const api = k as Partial<KasWareApi>;
  return typeof api.requestAccounts === 'function' &&
    typeof api.signPskt === 'function' &&
    typeof api.getNetwork === 'function'
    ? (api as KasWareApi)
    : undefined;
}

export function rejected(what: string, e: unknown): PageError {
  if (e instanceof PageError) return e;
  const o = e as { code?: unknown; message?: unknown };
  const msg = typeof o?.message === 'string' ? o.message : String(e);
  return new PageError('wallet_rejected', `${what}: ${o?.code !== undefined ? `${String(o.code)} ` : ''}${msg}`);
}

export class KasWareAdapter implements WalletAdapter {
  readonly kind = 'kasware' as const;
  private addr: string | null = null;
  private readonly serial = new Serial();

  constructor(
    private readonly api: KasWareApi,
    /** The network the wallet must report (walletNetworkFor). */
    private readonly expected: string | null,
    private readonly coins: (address: string) => Promise<Utxo[]>,
  ) {}

  async connect(): Promise<string> {
    const accounts = await this.serial
      .run(() => this.api.requestAccounts())
      .catch((e: unknown) => {
        throw rejected('requestAccounts', e);
      });
    const net = await this.network();
    if (this.expected === null || net !== this.expected) {
      throw new PageError(
        'wallet_network',
        `KasWare reports ${net}, the page needs ${this.expected ?? 'the quick wallet'}`,
      );
    }
    // The address itself is checked by the session (`scriptOf`: an address of the page's network).
    const a = accounts[0];
    if (typeof a !== 'string') throw new PageError('wallet_rejected', 'KasWare returned no account');
    this.addr = a;
    return a;
  }

  network(): Promise<string> {
    return this.serial
      .run(() => this.api.getNetwork())
      .catch((e: unknown) => {
        throw rejected('getNetwork', e);
      });
  }

  address(): Promise<string> {
    if (!this.addr) return Promise.reject(new PageError('wallet_missing', 'KasWare is not connected'));
    return Promise.resolve(this.addr);
  }

  async utxos(): Promise<Utxo[]> {
    return this.coins(await this.address());
  }

  async signInputs(built: BuiltTx, indices: readonly number[]): Promise<string> {
    const signInputs = signItems(built, indices);
    const net = await this.network();
    if (net !== this.expected) throw new PageError('wallet_network', `KasWare switched to ${net}`);
    const out = await this.serial
      .run(() => this.api.signPskt({ txJsonString: built.transaction, options: { signInputs } }))
      .catch((e: unknown) => {
        throw rejected('signPskt', e);
      });
    if (typeof out !== 'string') throw new PageError('wallet_rejected', 'signPskt returned no transaction');
    return out;
  }
}
