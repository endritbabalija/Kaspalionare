// The wallet adapter interface (spec 6.2): connect, network, address, utxos, signInputs. Every adapter takes
// its coins from a public node by address (`nodeCoins`), never from the wallet's own word or the indexer, and
// signs only the listed inputs (the built transaction's `sign` items); the page then runs the post-sign check
// (`assemble`) before anything is sent. One wallet request at a time (wallet/serial.ts).
import type { BuiltTx, Utxo } from '@wasm/web.js';

export type WalletKind = 'kasware' | 'kaspire' | 'quick';

export interface WalletAdapter {
  readonly kind: WalletKind;
  /** Ask for the account (the wallet's approval); resolves to its address. */
  connect(): Promise<string>;
  /** The network as the wallet names it (KasWare: kaspa_testnet_10). */
  network(): Promise<string>;
  /** The connected address. */
  address(): Promise<string>;
  /** The coins to build from: the node's plain entries at the address. */
  utxos(): Promise<Utxo[]>;
  /** Sign the listed inputs of `built` (indices of its `sign` items); resolves to the wallet's Safe JSON. */
  signInputs(built: BuiltTx, indices: readonly number[]): Promise<string>;
}

/** What a wallet must report on the page's network (spec 6.2, 6.6); none on the simnet (quick wallet only). */
export function walletNetworkFor(page: string, kind: WalletKind): string | null {
  if (page !== 'testnet-10') return kind === 'quick' ? page : null;
  if (kind === 'kasware') return 'kaspa_testnet_10';
  if (kind === 'kaspire') return 'testnet-10';
  return page;
}

/** The `signInputs` items for the listed indices: exactly the built transaction's own items. */
export function signItems(built: BuiltTx, indices: readonly number[]): { index: number; sighashType: number }[] {
  const items = built.sign.filter((s) => indices.includes(s.index));
  if (items.length === 0 || items.length !== indices.length) {
    throw new Error(
      `sign indices ${indices.join(',')} are not the built transaction's (${built.sign.map((s) => s.index).join(',')})`,
    );
  }
  return items.map((s) => ({ index: s.index, sighashType: s.sighashType }));
}
