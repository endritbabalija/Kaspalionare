import { createContext, useContext } from 'react';
import type { Ctx } from '@/net/round';

/** The page's session: configuration, the lottery worker and the indexer client. */
export const SessionContext = createContext<Ctx | null>(null);

export function useSession(): Ctx {
  const c = useContext(SessionContext);
  if (!c) throw new Error('useSession outside SessionContext');
  return c;
}
