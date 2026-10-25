import { describe, expect, it } from 'vitest';
import { readChoice, resolveTheme, writeChoice, THEME_KEY } from './theme';

function store(init: Record<string, string> = {}) {
  const m = new Map(Object.entries(init));
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), m };
}

describe('theme (D139)', () => {
  it('is dark with no preference, follows the device, and the switch overrides it', () => {
    expect(resolveTheme('device', false)).toBe('dark');
    expect(resolveTheme('device', true)).toBe('light');
    expect(resolveTheme('dark', true)).toBe('dark');
    expect(resolveTheme('light', false)).toBe('light');
  });

  it('remembers the choice per browser', () => {
    const s = store();
    expect(readChoice(s)).toBe('device');
    writeChoice('light', s);
    expect(s.m.get(THEME_KEY)).toBe('light');
    expect(readChoice(s)).toBe('light');
  });

  it('ignores a bad stored value and broken storage', () => {
    expect(readChoice(store({ [THEME_KEY]: 'neon' }))).toBe('device');
    const broken = {
      getItem: (): string | null => {
        throw new Error('denied');
      },
    };
    expect(readChoice(broken)).toBe('device');
  });
});
