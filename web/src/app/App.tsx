import { lazy, Suspense, useEffect } from 'react';
import type { Ctx } from '@/net/round';
import { captureReferral } from '@/referral/referral';
import { Backup } from '@/screens/Backup';
import { MyTickets } from '@/screens/MyTickets';
import { Play } from '@/screens/Play';
import { Rounds } from '@/screens/Rounds';
import { Placeholder } from '@/screens/Placeholder';
import { Vault } from '@/screens/Vault';
import { Wallet } from '@/screens/Wallet';
import { useTheme } from '@/theme/useTheme';
import { useWallet } from '@/wallet/context';
import { WalletProvider } from '@/wallet/session';
import { SessionContext } from './context';
import { Shell } from './Shell';
import { useRoute } from './useRoute';

// Development builds only: import.meta.env.DEV is false in a production build, so this import (and the
// catalogue's chunk) is dropped there; scripts/check-bundle.mjs checks that it is.
const Catalogue = import.meta.env.DEV ? lazy(() => import('@/dev/Catalogue')) : null;

export function App({ ctx }: { ctx: Ctx }) {
  // Spec 7: a ?ref= link is validated by the wasm and remembered (once per page load).
  useEffect(() => {
    void captureReferral(ctx.lottery, window.location.search).then((r) => {
      if (r.kind === 'refused') console.info(`referral link refused: ${r.reason}`);
    });
  }, [ctx]);
  return (
    <SessionContext.Provider value={ctx}>
      <WalletProvider ctx={ctx}>
        <Screens ctx={ctx} />
      </WalletProvider>
    </SessionContext.Provider>
  );
}

function Screens({ ctx }: { ctx: Ctx }) {
  const config = ctx.config;
  const wallet = useWallet();
  const route = useRoute();
  const { choice, setChoice } = useTheme();

  let screen;
  switch (route) {
    case 'play':
      screen = <Play />;
      break;
    case 'tickets':
      screen = <MyTickets />;
      break;
    case 'rounds':
      screen = <Rounds />;
      break;
    case 'vault':
      screen = <Vault />;
      break;
    case 'how':
      screen = <Placeholder title="How it works" testId="how" />;
      break;
    case 'backup':
      screen = <Backup />;
      break;
    case 'wallet':
      screen = <Wallet />;
      break;
    case 'catalogue':
      screen = Catalogue ? (
        <Suspense fallback={null}>
          <Catalogue />
        </Suspense>
      ) : (
        <Play />
      );
      break;
  }

  return (
    <Shell
      route={route}
      network={config.network}
      theme={choice}
      onTheme={setChoice}
      wide={route === 'catalogue'}
      referralAddress={wallet.address}
    >
      {screen}
    </Shell>
  );
}

export function ConfigProblems({ problems }: { problems: string[] }) {
  return (
    <main className="mx-auto max-w-[560px] p-4" data-testid="config-problems">
      <h1 className="text-[17px] font-semibold">Kaspalionare</h1>
      <ul role="alert" className="text-amber">
        {problems.map((p) => (
          <li key={p} className="font-mono text-[13px]">
            {p}
          </li>
        ))}
      </ul>
    </main>
  );
}
