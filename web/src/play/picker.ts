// The number picker's state (spec S1): `mainPicks` of 1..`mainMax` (5 of 1..30) and a bonus in 1..m, quick
// pick, clear; the ticket holds 1..B_MAX combinations (duplicates and shared main numbers allowed, design
// 4.3). The game's shape comes from the wasm (`gameShape`; spec 1: no TypeScript copy of game sizes). Only
// the player's choice lives here: whether a combination is valid for the round is checked by the wasm when
// it builds.
import type { Combination, GameShape } from '@wasm/web.js';

export interface Shape {
  mainMax: GameShape['mainMax'];
  mainPicks: GameShape['mainPicks'];
}

export interface Pick {
  main: number[];
  bonus: number | null;
}

export const EMPTY: Pick = { main: [], bonus: null };

export function toggleMain(p: Pick, n: number, s: Shape): Pick {
  if (n < 1 || n > s.mainMax) return p;
  if (p.main.includes(n)) return { ...p, main: p.main.filter((x) => x !== n) };
  if (p.main.length >= s.mainPicks) return p;
  return { ...p, main: [...p.main, n].sort((a, b) => a - b) };
}

export function toggleBonus(p: Pick, n: number, m: number): Pick {
  if (n < 1 || n > m) return p;
  return { ...p, bonus: p.bonus === n ? null : n };
}

export function complete(p: Pick, m: number, s: Shape): boolean {
  return p.main.length === s.mainPicks && p.bonus !== null && p.bonus >= 1 && p.bonus <= m;
}

export function combination(p: Pick, s: Shape): Combination | null {
  if (p.main.length !== s.mainPicks || s.mainPicks !== 5 || p.bonus === null) return null;
  const [a, b, c, d, e] = p.main as [number, number, number, number, number];
  return { main: [a, b, c, d, e], bonus: p.bonus };
}

/** A random pick: `rnd` returns integers in [0, n). */
export function quickPick(m: number, s: Shape, rnd: (n: number) => number = cryptoInt): Pick {
  const pool = Array.from({ length: s.mainMax }, (_, i) => i + 1);
  const main: number[] = [];
  for (let i = 0; i < s.mainPicks; i++) main.push(pool.splice(rnd(pool.length), 1)[0] as number);
  return { main: main.sort((a, b) => a - b), bonus: 1 + rnd(m) };
}

/** A uniform integer in [0, n) from the browser's random source. */
export function cryptoInt(n: number): number {
  const lim = Math.floor(0x1_0000_0000 / n) * n;
  const b = new Uint32Array(1);
  for (;;) {
    crypto.getRandomValues(b);
    if ((b[0] as number) < lim) return (b[0] as number) % n;
  }
}

export function sameCombination(a: Combination, b: Combination): boolean {
  return a.bonus === b.bonus && a.main.every((x, i) => x === b.main[i]);
}
