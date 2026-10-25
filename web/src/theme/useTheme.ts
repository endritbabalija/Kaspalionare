import { useCallback, useEffect, useState } from 'react';
import { applyTheme, prefersLight, readChoice, resolveTheme, writeChoice, type Theme, type ThemeChoice } from './theme';

/** The current choice and theme; follows device changes while the choice is "device". */
export function useTheme(): { choice: ThemeChoice; theme: Theme; setChoice: (c: ThemeChoice) => void } {
  const [choice, setChoiceState] = useState<ThemeChoice>(() => readChoice());
  const [light, setLight] = useState<boolean>(() => prefersLight());

  useEffect(() => {
    let mq: MediaQueryList | undefined;
    try {
      mq = window.matchMedia('(prefers-color-scheme: light)');
    } catch {
      return;
    }
    const on = (e: MediaQueryListEvent) => setLight(e.matches);
    mq.addEventListener('change', on);
    return () => mq?.removeEventListener('change', on);
  }, []);

  const theme = resolveTheme(choice, light);
  useEffect(() => applyTheme(theme), [theme]);

  const setChoice = useCallback((c: ThemeChoice) => {
    writeChoice(c);
    setChoiceState(c);
  }, []);

  return { choice, theme, setChoice };
}
