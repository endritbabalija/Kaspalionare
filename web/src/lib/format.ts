// Display formatting only (spec 3, 4): KAS with 2 decimals and exact sompi, DAA scores as clock times at
// 10 per second, shortened hashes. No lottery arithmetic lives here (spec 1): amounts come decoded from
// chain state or from the wasm as decimal strings, and are only printed.

export const SOMPI_PER_KAS = 100_000_000n;
/** Testnet-10 and the design's clock: 10 DAA scores per second (spec 4 "Time"). */
export const DAA_PER_SECOND = 10n;

function big(dec: string): bigint {
  if (!/^-?\d{1,30}$/.test(dec)) throw new Error(`not a decimal: ${dec.slice(0, 32)}`);
  return BigInt(dec);
}

/** Digits grouped by thousands ("1234567" -> "1,234,567"). */
export function groupDigits(digits: string): string {
  return digits.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

/** Sompi as KAS with 2 decimals, rounded half up ("4820000000000" -> "48,200.00"). */
export function kas2(sompi: string): string {
  const v = big(sompi);
  const neg = v < 0n;
  const a = neg ? -v : v;
  const cents = (a + 500_000n) / 1_000_000n;
  const whole = cents / 100n;
  const frac = (cents % 100n).toString().padStart(2, '0');
  return `${neg ? '-' : ''}${groupDigits(whole.toString())}.${frac}`;
}

/**
 * Sompi as KAS exactly, 2 to 8 decimals and no digit grouping ("2510000" -> "0.0251", "3000000" -> "0.03"): the
 * form the amount field reads back (`sompiOfKas`), for figures a player types in again (F6 part A review S1).
 */
export function kasExact(sompi: string): string {
  const v = big(sompi);
  const neg = v < 0n;
  const a = neg ? -v : v;
  const frac = (a % SOMPI_PER_KAS)
    .toString()
    .padStart(8, '0')
    .replace(/0{1,6}$/, '');
  return `${neg ? '-' : ''}${(a / SOMPI_PER_KAS).toString()}.${frac}`;
}

/** Exact sompi, grouped ("4820000000000" -> "4,820,000,000,000"). */
export function sompiExact(sompi: string): string {
  const v = big(sompi);
  return v < 0n ? `-${groupDigits((-v).toString())}` : groupDigits(v.toString());
}

/** A DAA score as a time (ms since the epoch): `observedAt` was when the node's virtual DAA was `virtualDaa`. */
export function daaToMs(daa: string, virtualDaa: string, observedAt: number): number {
  const d = big(daa) - big(virtualDaa);
  return observedAt + Number((d * 1000n) / DAA_PER_SECOND);
}

/** A duration as "05:42:10" (with "2d " before it past a day); 0 below zero. */
export function countdown(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const d = Math.floor(s / 86_400);
  const h = Math.floor((s % 86_400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const p = (x: number) => x.toString().padStart(2, '0');
  return `${d > 0 ? `${d}d ` : ''}${p(h)}:${p(m)}:${p(sec)}`;
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/** A local clock time: "14:32" today, else "2 Nov 14:32". */
export function clock(ms: number, now: number): string {
  const t = new Date(ms);
  const n = new Date(now);
  const hm = `${t.getHours().toString().padStart(2, '0')}:${t.getMinutes().toString().padStart(2, '0')}`;
  const sameDay = t.getFullYear() === n.getFullYear() && t.getMonth() === n.getMonth() && t.getDate() === n.getDate();
  return sameDay ? hm : `${t.getDate()} ${MONTHS[t.getMonth()]} ${hm}`;
}

/** A hash or address shortened to its first and last 6 characters (an address keeps its prefix). */
export function shortHash(v: string): string {
  const colon = v.indexOf(':');
  const prefix = colon >= 0 ? v.slice(0, colon + 1) : '';
  const body = colon >= 0 ? v.slice(colon + 1) : v;
  return body.length <= 14 ? v : `${prefix}${body.slice(0, 6)}…${body.slice(-6)}`;
}

/**
 * An amount typed in KAS ("12", "12.5", "0.00000001") as sompi, or null when it is not a plain decimal with at
 * most 8 decimals (input reading only: the wasm decides what the vault takes).
 */
export function sompiOfKas(text: string): string | null {
  const m = /^(\d{1,11})(?:\.(\d{1,8}))?$/.exec(text.trim());
  if (!m) return null;
  const whole = BigInt(m[1] as string);
  const frac = BigInt((m[2] ?? '').padEnd(8, '0'));
  return (whole * SOMPI_PER_KAS + frac).toString();
}
