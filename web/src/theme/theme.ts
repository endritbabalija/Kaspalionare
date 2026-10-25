// The theme switch (spec 5, D139): dark, light or device; remembered per browser (localStorage key
// kl.theme); "device" follows prefers-color-scheme, and with no preference the page is dark.
// public/theme-boot.js applies the same rule before first paint.

export type ThemeChoice = 'dark' | 'light' | 'device';
export type Theme = 'dark' | 'light';

export const THEME_KEY = 'kl.theme';
export const THEME_CHOICES: readonly ThemeChoice[] = ['dark', 'light', 'device'];

export function isChoice(v: unknown): v is ThemeChoice {
  return v === 'dark' || v === 'light' || v === 'device';
}

/** The remembered choice; "device" when none (or storage is unavailable). */
export function readChoice(storage: Pick<Storage, 'getItem'> | undefined = safeStorage()): ThemeChoice {
  try {
    const v = storage?.getItem(THEME_KEY);
    return isChoice(v) ? v : 'device';
  } catch {
    return 'device';
  }
}

export function writeChoice(c: ThemeChoice, storage: Pick<Storage, 'setItem'> | undefined = safeStorage()): void {
  try {
    storage?.setItem(THEME_KEY, c);
  } catch {
    // Private windows may refuse storage; the choice then lasts for this page only.
  }
}

/** The theme a choice shows: the device setting decides only for "device", and only light opts out of dark. */
export function resolveTheme(c: ThemeChoice, prefersLight: boolean): Theme {
  if (c === 'device') return prefersLight ? 'light' : 'dark';
  return c;
}

export function prefersLight(): boolean {
  try {
    return window.matchMedia('(prefers-color-scheme: light)').matches;
  } catch {
    return false;
  }
}

export function applyTheme(t: Theme, root: HTMLElement = document.documentElement): void {
  root.dataset.theme = t;
}

function safeStorage(): Storage | undefined {
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}
