// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { wasmClient } from '@/test/wasmWorker';
import { EMPTY, combination, complete, quickPick, toggleBonus, toggleMain, type Shape } from './picker';

const lottery = wasmClient('testnet-10');

describe('number picker (spec S1)', () => {
  it('takes the game shape from the wasm (spec 1: no TypeScript copy)', async () => {
    expect(await lottery.call('gameShape')).toEqual({ mainMax: 30, mainPicks: 5, bMax: 4, coinsMax: 8 });
  });

  it('picks 5 of 1..30 in order, at most five, toggles off', async () => {
    const s: Shape = await lottery.call('gameShape');
    let p = EMPTY;
    for (const n of [30, 2, 17, 9, 4]) p = toggleMain(p, n, s);
    expect(p.main).toEqual([2, 4, 9, 17, 30]);
    expect(toggleMain(p, 5, s).main).toEqual([2, 4, 9, 17, 30]);
    expect(toggleMain(p, 9, s).main).toEqual([2, 4, 17, 30]);
    expect(toggleMain(EMPTY, 0, s)).toBe(EMPTY);
    expect(toggleMain(EMPTY, 31, s)).toBe(EMPTY);
  });

  it('takes a bonus in 1..m only, and is complete with five and a bonus', async () => {
    const s: Shape = await lottery.call('gameShape');
    let p = EMPTY;
    for (const n of [1, 2, 3, 4, 5]) p = toggleMain(p, n, s);
    expect(complete(p, 2, s)).toBe(false);
    expect(toggleBonus(p, 3, 2)).toBe(p);
    p = toggleBonus(p, 2, 2);
    expect(complete(p, 2, s)).toBe(true);
    expect(combination(p, s)).toEqual({ main: [1, 2, 3, 4, 5], bonus: 2 });
    expect(toggleBonus(p, 2, 2).bonus).toBeNull();
  });

  it('quick pick gives five distinct ascending numbers and a bonus in range', async () => {
    const s: Shape = await lottery.call('gameShape');
    for (let i = 0; i < 200; i++) {
      const p = quickPick(3, s);
      expect(new Set(p.main).size).toBe(5);
      expect([...p.main].sort((a, b) => a - b)).toEqual(p.main);
      expect(p.main.every((n) => n >= 1 && n <= 30)).toBe(true);
      expect(complete(p, 3, s)).toBe(true);
    }
    expect(quickPick(2, s, () => 0)).toEqual({ main: [1, 2, 3, 4, 5], bonus: 1 });
  });
});
