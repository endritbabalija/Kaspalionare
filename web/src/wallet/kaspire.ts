// Kaspire (spec 6.2): `window.kaspire.signPskt({sender, txJsonString, options: {signInputs}})` (its docs,
// kaspire.kaslab.space/developers/extension, read 2026-10-25). UNTESTED: F0 could not run it (not installed);
// whether Kaspire accepts unknown covenant scripts is unknown. The same rules as KasWare: the explicit index
// list, no push (the page submits after `assemble`), network `testnet-10` only, one request at a time.
// WalletConnect `kaspa_signPskt` (Android) is not built.
import type { BuiltTx, Utxo } from '@wasm/web.js';
import { PageError } from '@/lib/errors';
import { rejected } from './kasware';
import { Serial } from './serial';
import { signItems, type WalletAdapter } from './types';

export interface KaspireApi {
  requestAccounts(): Promise<string[]>;
  getNetwork(): Promise<string>;
  signPskt(a: {
    sender: string;
    txJsonString: string;
    options: { signInputs: { index: number; sighashType: number }[] };
  }): Promise<string>;
}

export function kaspireApi(w: unknown = globalThis): KaspireApi | undefined {
  const k = (w as { kaspire?: unknown }).kaspire;
  if (!k || typeof k !== 'object') return undefined;
  const api = k as Partial<KaspireApi>;
  return typeof api.requestAccounts === 'function' &&
    typeof api.signPskt === 'function' &&
    typeof api.getNetwork === 'function'
    ? (api as KaspireApi)
    : undefined;
}

export class KaspireAdapter implements WalletAdapter {
  readonly kind = 'kaspire' as const;
  private addr: string | null = null;
  private readonly serial = new Serial();

  constructor(
    private readonly api: KaspireApi,
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
        `Kaspire reports ${net}, the page needs ${this.expected ?? 'the quick wallet'}`,
      );
    }
    // The address itself is checked by the session (`scriptOf`: an address of the page's network).
    const a = accounts[0];
    if (typeof a !== 'string') throw new PageError('wallet_rejected', 'Kaspire returned no account');
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
    if (!this.addr) return Promise.reject(new PageError('wallet_missing', 'Kaspire is not connected'));
    return Promise.resolve(this.addr);
  }

  async utxos(): Promise<Utxo[]> {
    return this.coins(await this.address());
  }

  async signInputs(built: BuiltTx, indices: readonly number[]): Promise<string> {
    const signInputs = signItems(built, indices);
    const sender = await this.address();
    // As KasWare: the network again before each signature (F2/F3 review note).
    const net = await this.network();
    if (net !== this.expected) throw new PageError('wallet_network', `Kaspire switched to ${net}`);
    const out = await this.serial
      .run(() => this.api.signPskt({ sender, txJsonString: built.transaction, options: { signInputs } }))
      .catch((e: unknown) => {
        throw rejected('signPskt', e);
      });
    if (typeof out !== 'string') throw new PageError('wallet_rejected', 'signPskt returned no transaction');
    return out;
  }
}
