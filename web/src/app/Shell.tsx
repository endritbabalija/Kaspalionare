// The app shell (spec 3): header with the name, the network badge (every page) and the menu; the screen;
// bottom navigation Play · My tickets · Rounds. On desktop the same column, centred and wider.
import type { ReactNode } from 'react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/cn';
import { referralLink } from '@/referral/referral';
import { isChoice, type ThemeChoice } from '@/theme/theme';
import { MENU, NAV, href, type Route } from './routes';

export interface ShellProps {
  route: Route;
  network: string;
  theme: ThemeChoice;
  onTheme: (c: ThemeChoice) => void;
  /** The connected wallet's address, for the referral link (J7); none before a wallet is connected. */
  referralAddress?: string;
  /** A wider column (the dev catalogue shows both themes side by side). */
  wide?: boolean;
  children: ReactNode;
}

/** Spec 7 honesty: every page shows the network, and that its coins have no value, as the badge's value. */
export function NetworkBadge({ network }: { network: string }) {
  return (
    <span
      data-testid="network-badge"
      className="inline-flex h-6 items-center rounded-full bg-status-closing-bg px-2 font-mono text-[11px] text-status-closing-fg uppercase"
    >
      {network} · no value
    </span>
  );
}

export function Shell({ route, network, theme, onTheme, referralAddress, wide, children }: ShellProps) {
  return (
    <div className={cn('mx-auto flex min-h-dvh w-full flex-col', wide ? 'max-w-[1200px]' : 'max-w-[560px]')}>
      <header className="sticky top-0 z-10 flex items-center justify-between border-b border-rule bg-ground px-4 py-2">
        <div className="flex items-center gap-2">
          <a
            href={href('play')}
            className="inline-flex min-h-11 items-center text-lg font-semibold tracking-tight text-ink no-underline"
            data-testid="brand"
          >
            Kaspalionare
          </a>
          <NetworkBadge network={network} />
        </div>
        <div className="flex items-center gap-2">
          <Button asChild variant="secondary" data-testid="connect-button">
            <a
              href={href('wallet')}
              className="font-mono no-underline"
              aria-label={referralAddress ? `Wallet ${referralAddress}` : 'Connect'}
            >
              {referralAddress ? `…${referralAddress.slice(-6)}` : 'Connect'}
            </a>
          </Button>
          {/* Non-modal: a modal menu sets aria-hidden on the page while it keeps focusable elements (axe aria-hidden-focus). */}
          <DropdownMenu modal={false}>
            <DropdownMenuTrigger asChild>
              <Button variant="secondary" size="icon" aria-label="Menu" data-testid="menu-button" className="shrink-0">
                <svg
                  width="18"
                  height="18"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.8"
                  aria-hidden="true"
                >
                  <path d="M4 7h16M4 12h16M4 17h16" />
                </svg>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" data-testid="menu">
              {MENU.map((m) => (
                <DropdownMenuItem key={m.route} asChild data-testid={`menu-${m.route}`}>
                  <a href={href(m.route)} className="text-ink no-underline">
                    {m.label}
                  </a>
                </DropdownMenuItem>
              ))}
              <DropdownMenuSeparator />
              <DropdownMenuLabel id="theme-label">Theme</DropdownMenuLabel>
              <DropdownMenuRadioGroup
                value={theme}
                onValueChange={(v) => isChoice(v) && onTheme(v)}
                aria-labelledby="theme-label"
              >
                <DropdownMenuRadioItem value="dark" data-testid="theme-dark">
                  Dark
                </DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="light" data-testid="theme-light">
                  Light
                </DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="device" data-testid="theme-device">
                  Device
                </DropdownMenuRadioItem>
              </DropdownMenuRadioGroup>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                data-testid="menu-referral"
                disabled={!referralAddress}
                onSelect={() =>
                  referralAddress &&
                  void navigator.clipboard?.writeText(referralLink(referralAddress, window.location.href))
                }
              >
                Copy referral link
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </header>
      <main className="flex flex-1 flex-col gap-3.5 p-4 pb-24" data-testid={`screen-${route}`}>
        {children}
      </main>
      <nav
        aria-label="Main"
        className="fixed inset-x-0 bottom-0 z-10 mx-auto flex w-full max-w-[560px] border-t border-rule bg-ground pb-[env(safe-area-inset-bottom)]"
      >
        {NAV.map((n) => (
          <a
            key={n.route}
            href={href(n.route)}
            data-testid={`nav-${n.route}`}
            aria-current={route === n.route ? 'page' : undefined}
            className={cn(
              'flex min-h-14 flex-1 items-center justify-center text-sm font-medium no-underline',
              route === n.route ? 'text-ink' : 'text-graphite',
            )}
          >
            <span className={cn('border-b-2 py-1', route === n.route ? 'border-teal' : 'border-transparent')}>
              {n.label}
            </span>
          </a>
        ))}
      </nav>
    </div>
  );
}
