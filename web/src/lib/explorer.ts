// Links to the testnet block explorer named at build time (spec 6.5).

export type HashKind = 'transaction' | 'address' | 'block';

const path: Record<HashKind, string> = { transaction: 'txs', address: 'addresses', block: 'blocks' };

/** The explorer page of a transaction, address or block; undefined when the build names no explorer. */
export function explorerLink(base: string | undefined, kind: HashKind, value: string): string | undefined {
  if (!base) return undefined;
  return `${base.replace(/\/$/, '')}/${path[kind]}/${encodeURIComponent(value)}`;
}
