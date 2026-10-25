// Referral links (spec 7, D134): `?ref=<address>` names the referrer. The value is validated by the wasm as an
// address of the page's network with a standard script (`scriptOf`: a kaspa: or any other network's address is
// address_network), never inserted as HTML, and remembered in this browser (localStorage kl.ref) until another
// valid link replaces it. With none stored, the referrer is the site owner's address from the build
// configuration (spec 6.5). The builder pays it only if the referral reaches M_REF (decided in Rust).
import { describe as describeError, unwrap } from '@/lib/errors';
import type { LotteryClient } from '@/worker/client';

export const REF_KEY = 'kl.ref';
/** Longer than any address; anything longer is refused before it reaches the wasm. */
export const REF_MAX = 128;

type Get = Pick<Storage, 'getItem'>;
type Set = Pick<Storage, 'setItem'>;

function storage(): Storage | undefined {
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}

/** The `ref` parameter of a location search string, if any. */
export function refParam(search: string): string | null {
  try {
    const v = new URLSearchParams(search).get('ref');
    return v === null ? null : v.trim();
  } catch {
    return null;
  }
}

/** Whether the wasm accepts `address` as an address of the started network (a standard script). */
export async function validReferrer(lottery: LotteryClient, address: string): Promise<boolean> {
  if (!address || address.length > REF_MAX) return false;
  try {
    unwrap(await lottery.call('scriptOf', address));
    return true;
  } catch {
    return false;
  }
}

export type Captured = { kind: 'none' } | { kind: 'stored'; address: string } | { kind: 'refused'; reason: string };

/** A `?ref=` in `search`: stored if valid (replacing any earlier one), refused otherwise (the stored one stays). */
export async function captureReferral(
  lottery: LotteryClient,
  search: string,
  s: Set | undefined = storage(),
): Promise<Captured> {
  const v = refParam(search);
  if (v === null) return { kind: 'none' };
  if (!(await validReferrer(lottery, v))) {
    let reason = 'not an address of this network';
    try {
      unwrap(await lottery.call('scriptOf', v.slice(0, REF_MAX)));
    } catch (e) {
      reason = describeError(e).message;
    }
    return { kind: 'refused', reason };
  }
  try {
    s?.setItem(REF_KEY, v);
  } catch {
    // private windows: the link then lasts for this page only
  }
  return { kind: 'stored', address: v };
}

export function storedReferral(s: Get | undefined = storage()): string | undefined {
  try {
    return s?.getItem(REF_KEY) ?? undefined;
  } catch {
    return undefined;
  }
}

/** The referrer for a purchase: the remembered link's (if still valid here), else the site owner's. */
export async function referrerFor(
  lottery: LotteryClient,
  siteOwner: string | undefined,
  s: Get | undefined = storage(),
): Promise<string | undefined> {
  const r = storedReferral(s);
  if (r && (await validReferrer(lottery, r))) return r;
  if (siteOwner && (await validReferrer(lottery, siteOwner))) return siteOwner;
  return undefined;
}

/** The link that names `address` as referrer (J7): this page, `?ref=<address>`, no route. */
export function referralLink(address: string, href: string): string {
  const u = new URL(href);
  u.hash = '';
  u.search = `?ref=${encodeURIComponent(address)}`;
  return u.toString();
}
