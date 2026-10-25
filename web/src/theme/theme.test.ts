import { describe, expect, it } from 'vitest';
import { readChoice, resolveTheme, writeChoice, THEME_KEY } from './theme';

function store(init: Record<string, string> = {}) {
  const m = new Map(Object.entries(init));
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), m };
}

describe('theme (D139, amended 2026-10-26)', () => {
  it('device follows the device setting; dark and light override it', () => {
    expect(resolveTheme('device', false)).toBe('dark');
    expect(resolveTheme('device', true)).toBe('light');
    expect(resolveTheme('dark', true)).toBe('dark');
    expect(resolveTheme('light', false)).toBe('light');
  });

  it('a first-time visitor sees dark, whatever the device says', () => {
    expect(readChoice(store())).toBe('dark');
    expect(resolveTheme(readChoice(store()), true)).toBe('dark');
    expect(resolveTheme(readChoice(store()), false)).toBe('dark');
  });

  it('remembers the choice per browser', () => {
    const s = store();
    expect(readChoice(s)).toBe('dark');
    writeChoice('device', s);
    expect(readChoice(s)).toBe('device');
    writeChoice('light', s);
    expect(s.m.get(THEME_KEY)).toBe('light');
    expect(readChoice(s)).toBe('light');
  });

  it('ignores a bad stored value and broken storage', () => {
    expect(readChoice(store({ [THEME_KEY]: 'neon' }))).toBe('dark');
    const broken = {
      getItem: (): string | null => {
        throw new Error('denied');
      },
    };
    expect(readChoice(broken)).toBe('dark');
  });
});
