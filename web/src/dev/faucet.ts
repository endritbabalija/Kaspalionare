// The dev faucet client (simnet dev run only): asks `simnet dev` (crates/simnet/src/faucet.rs) to pay the
// page's quick wallet. Imported only behind import.meta.env.DEV with a configured faucet (config.ts: never
// in a production build, which vite.config.ts also refuses to make with VITE_FAUCET_URL set);
// scripts/check-bundle.mjs checks that the marker below is absent from production bundles.

/** Searched for by scripts/check-bundle.mjs. */
export const FAUCET_MARKER = 'KASPALIONARE_DEV_FAUCET';

export interface FaucetPaid {
  transactionId: string;
  amount: string;
}

export async function requestFaucet(faucetUrl: string, address: string, f: typeof fetch = fetch): Promise<FaucetPaid> {
  if (!address.startsWith('kaspasim:')) throw new Error('the dev faucet pays kaspasim: addresses only');
  const url = `${faucetUrl}?address=${encodeURIComponent(address)}`;
  const res = await f(url, { method: 'POST', credentials: 'omit', cache: 'no-store' });
  const body: unknown = await res.json();
  if (!res.ok || typeof body !== 'object' || body === null) {
    const msg =
      typeof body === 'object' && body !== null && 'error' in body
        ? String((body as { error: unknown }).error)
        : `HTTP ${res.status}`;
    throw new Error(`faucet: ${msg}`);
  }
  const b = body as Partial<FaucetPaid>;
  if (typeof b.transactionId !== 'string' || !/^[0-9a-f]{64}$/.test(b.transactionId) || typeof b.amount !== 'string') {
    throw new Error('faucet: malformed answer');
  }
  return { transactionId: b.transactionId, amount: b.amount };
}
