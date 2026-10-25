// Hash routes (a static site on any host: no server rewrites). The dev catalogue exists in development
// builds only (main.tsx imports it behind import.meta.env.DEV, so a production bundle has no copy).

export type Route = 'play' | 'tickets' | 'rounds' | 'vault' | 'how' | 'backup' | 'wallet' | 'catalogue';

export const NAV: readonly { route: Route; label: string }[] = [
  { route: 'play', label: 'Play' },
  { route: 'tickets', label: 'My tickets' },
  { route: 'rounds', label: 'Rounds' },
];

export const MENU: readonly { route: Route; label: string }[] = [
  { route: 'vault', label: 'Vault' },
  { route: 'how', label: 'How it works' },
  { route: 'backup', label: 'Backup' },
  { route: 'wallet', label: 'Wallet' },
];

const ALL: readonly Route[] = ['play', 'tickets', 'rounds', 'vault', 'how', 'backup', 'wallet', 'catalogue'];

export function href(r: Route): string {
  return r === 'catalogue' ? '#/dev/catalogue' : `#/${r}`;
}

/** The route of a location hash; Play for anything unknown (and for the catalogue outside development). */
export function routeOf(hash: string, dev: boolean): Route {
  const h = hash.replace(/^#\/?/, '').split('?')[0] ?? '';
  if (h === 'dev/catalogue') return dev ? 'catalogue' : 'play';
  return (ALL as readonly string[]).includes(h) && h !== 'catalogue' ? (h as Route) : 'play';
}
