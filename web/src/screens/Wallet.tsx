// S7 Wallet sheet (spec 3, 4): disconnected, connecting, wrong network, connected, quick wallet. Connect
// KasWare or Kaspire, or create, import or export the quick testnet wallet; the address, the balance (the
// node's coins at the address) and the network.
import { lazy, Suspense, useCallback, useEffect, useId, useState } from 'react';
import { useSession } from '@/app/context';
import { Amount } from '@/components/Amount';
import { HashOrAddress } from '@/components/HashOrAddress';
import { Button } from '@/components/ui/button';
import { describe, unwrap } from '@/lib/errors';
import { QUICK_WALLET } from '@/wallet/quickEnabled';
import { useWallet, type WalletState } from '@/wallet/context';

// Loaded only in a build with the quick wallet (wallet/quickEnabled.ts).
const QuickWalletPanel = QUICK_WALLET ? lazy(() => import('./wallet/QuickWalletPanel')) : null;

const STATE_NAME: Record<WalletState, string> = {
  disconnected: 'Disconnected',
  connecting: 'Connecting',
  'wrong-network': 'Wrong network',
  connected: 'Connected',
  quick: 'Quick wallet',
};

const card = 'flex flex-col gap-2 rounded-[14px] border border-rule bg-surface p-4';

function Row({ name, children, testId }: { name: string; children: React.ReactNode; testId: string }) {
  return (
    <div data-testid={testId} className="flex min-h-11 items-center justify-between gap-3">
      <span className="text-[13px] text-graphite">{name}</span>
      <span className="flex items-center">{children}</span>
    </div>
  );
}

export function Wallet() {
  const ctx = useSession();
  const w = useWallet();
  const title = useId();
  const [balance, setBalance] = useState<{ sompi?: string; error?: string }>({});

  const refresh = useCallback(async () => {
    if (!w.address) return setBalance({});
    try {
      setBalance({ sompi: unwrap(await ctx.lottery.call('nodeBalance', w.address)) });
    } catch (e) {
      setBalance({ error: describe(e).message });
    }
  }, [ctx, w.address]);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 10_000);
    return () => clearInterval(t);
  }, [refresh]);

  const on = w.state === 'connected' || w.state === 'quick';
  return (
    <section aria-labelledby={title} data-testid="wallet-sheet" data-state={w.state} className="flex flex-col gap-3.5">
      <div className={card}>
        <h1 id={title} className="m-0 text-[17px] font-semibold">
          Wallet
        </h1>
        <Row name="Status" testId="wallet-status-row">
          <span data-testid="wallet-status" role="status" className="font-mono text-[13px]">
            {STATE_NAME[w.state]}
          </span>
        </Row>
        <Row name="Network" testId="wallet-network-row">
          <span data-testid="wallet-network" className="font-mono text-[13px]">
            {ctx.config.network}
          </span>
        </Row>
        {on && w.address && (
          <>
            <Row name="Address" testId="wallet-address-row">
              <HashOrAddress
                value={w.address}
                kind="address"
                explorer={ctx.config.explorerUrl}
                testId="wallet-address"
              />
            </Row>
            <Row name="Balance" testId="wallet-balance-row">
              {balance.sompi !== undefined ? (
                <Amount sompi={balance.sompi} testId="wallet-balance" />
              ) : (
                <span data-testid="wallet-balance" className="font-mono text-[13px] text-graphite">
                  {balance.error ?? '–'}
                </span>
              )}
            </Row>
          </>
        )}
        {w.error && (
          <p role="alert" data-testid="wallet-error" data-code={w.error.code ?? ''} className="m-0 text-amber">
            {w.error.message}
          </p>
        )}
      </div>
      <div className={card}>
        <Button
          data-testid="connect-kasware"
          disabled={w.state === 'connecting'}
          onClick={() => void w.connect('kasware')}
        >
          KasWare
        </Button>
        <Button
          data-testid="connect-kaspire"
          disabled={w.state === 'connecting'}
          onClick={() => void w.connect('kaspire')}
        >
          Kaspire
        </Button>
        {on && (
          <Button variant="ghost" data-testid="wallet-disconnect" onClick={w.disconnect}>
            Disconnect
          </Button>
        )}
      </div>
      {QuickWalletPanel && (
        <Suspense fallback={null}>
          <QuickWalletPanel onFunded={() => void refresh()} />
        </Suspense>
      )}
    </section>
  );
}
