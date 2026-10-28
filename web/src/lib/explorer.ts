// Links to the testnet block explorer named at build time (spec 6.5).

export type HashKind = 'transaction' | 'address' | 'block';

const path: Record<HashKind, string> = { transaction: 'txs', address: 'addresses', block: 'blocks' };

// F6 part F: kaspa.stream (tn10.kaspa.stream, the testnet-10 explorer the beta links to since
// explorer-tn10.kaspa.org answers "DEPLOYMENT_DISABLED") names transaction pages /transactions/<id>;
// its address and block pages match the kaspa.org layout.
const kaspaStream: Record<HashKind, string> = { ...path, transaction: 'transactions' };

function layout(base: string): Record<HashKind, string> {
  try {
    const host = new URL(base).hostname;
    return host === 'kaspa.stream' || host.endsWith('.kaspa.stream') ? kaspaStream : path;
  } catch {
    return path;
  }
}

/** The explorer page of a transaction, address or block; undefined when the build names no explorer. */
export function explorerLink(base: string | undefined, kind: HashKind, value: string): string | undefined {
  if (!base) return undefined;
  return `${base.replace(/\/$/, '')}/${layout(base)[kind]}/${encodeURIComponent(value)}`;
}
