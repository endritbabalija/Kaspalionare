// S1 Play (spec 3): the live round card, then the buy card (picker, ticket, cost panel, buy, progress).
import { useMemo } from 'react';
import { useSession } from '@/app/context';
import { installDevHooks } from '@/dev/hooks';
import { useWallet } from '@/wallet/context';
import { BuyCard } from './play/BuyCard';
import { RoundCardView } from './play/RoundCard';
import { useRoundCard } from './play/useRoundCard';
import { usePlay } from './play/usePlay';

export function Play() {
  const ctx = useSession();
  const wallet = useWallet();
  const { load, retry } = useRoundCard(ctx);
  // DEV ONLY: the e2e suite's hooks (dev/hooks.ts); import.meta.env.DEV is false in a production build.
  const dev = useMemo(() => (import.meta.env.DEV ? installDevHooks() : undefined), []);
  const play = usePlay(ctx, wallet, dev);
  return (
    <>
      <RoundCardView load={load} onRetry={retry} />
      <BuyCard play={play} explorer={ctx.config.explorerUrl} walletConnected={!!wallet.address && !!wallet.adapter} />
    </>
  );
}
