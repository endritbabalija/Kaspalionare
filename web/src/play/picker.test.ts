import { describe, expect, it } from 'vitest';
import { EMPTY, combination, complete, quickPick, toggleBonus, toggleMain } from './picker';

describe('number picker (spec S1)', () => {
  it('picks 5 of 1..30 in order, at most five, toggles off', () => {
    let p = EMPTY;
    for (const n of [30, 2, 17, 9, 4]) p = toggleMain(p, n);
    expect(p.main).toEqual([2, 4, 9, 17, 30]);
    expect(toggleMain(p, 5).main).toEqual([2, 4, 9, 17, 30]);
    expect(toggleMain(p, 9).main).toEqual([2, 4, 17, 30]);
    expect(toggleMain(EMPTY, 0)).toBe(EMPTY);
    expect(toggleMain(EMPTY, 31)).toBe(EMPTY);
  });

  it('takes a bonus in 1..m only, and is complete with five and a bonus', () => {
    let p = EMPTY;
    for (const n of [1, 2, 3, 4, 5]) p = toggleMain(p, n);
    expect(complete(p, 2)).toBe(false);
    expect(toggleBonus(p, 3, 2)).toBe(p);
    p = toggleBonus(p, 2, 2);
    expect(complete(p, 2)).toBe(true);
    expect(combination(p)).toEqual({ main: [1, 2, 3, 4, 5], bonus: 2 });
    expect(toggleBonus(p, 2, 2).bonus).toBeNull();
  });

  it('quick pick gives five distinct ascending numbers and a bonus in range', () => {
    for (let i = 0; i < 200; i++) {
      const p = quickPick(3);
      expect(new Set(p.main).size).toBe(5);
      expect([...p.main].sort((a, b) => a - b)).toEqual(p.main);
      expect(p.main.every((n) => n >= 1 && n <= 30)).toBe(true);
      expect(complete(p, 3)).toBe(true);
    }
    expect(quickPick(2, () => 0)).toEqual({ main: [1, 2, 3, 4, 5], bonus: 1 });
  });
});
