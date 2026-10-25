import { lazy, Suspense } from 'react';
import type { Ctx } from '@/net/round';
import { Play } from '@/screens/Play';
import { Placeholder } from '@/screens/Placeholder';
import { useTheme } from '@/theme/useTheme';
import { SessionContext } from './context';
import { Shell } from './Shell';
import { useRoute } from './useRoute';

// Development builds only: import.meta.env.DEV is false in a production build, so this import (and the
// catalogue's chunk) is dropped there; scripts/check-bundle.mjs checks that it is.
const Catalogue = import.meta.env.DEV ? lazy(() => import('@/dev/Catalogue')) : null;

export function App({ ctx }: { ctx: Ctx }) {
  const config = ctx.config;
  const route = useRoute();
  const { choice, setChoice } = useTheme();

  let screen;
  switch (route) {
    case 'play':
      screen = <Play />;
      break;
    case 'tickets':
      screen = <Placeholder title="My tickets" testId="tickets" />;
      break;
    case 'rounds':
      screen = <Placeholder title="Rounds" testId="rounds" />;
      break;
    case 'vault':
      screen = <Placeholder title="Vault" testId="vault" />;
      break;
    case 'how':
      screen = <Placeholder title="How it works" testId="how" />;
      break;
    case 'backup':
      screen = <Placeholder title="Backup" testId="backup" />;
      break;
    case 'wallet':
      screen = <Placeholder title="Wallet" testId="wallet" />;
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
    <SessionContext.Provider value={ctx}>
      <Shell route={route} network={config.network} theme={choice} onTheme={setChoice} wide={route === 'catalogue'}>
        {screen}
      </Shell>
    </SessionContext.Provider>
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
