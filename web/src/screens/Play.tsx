// S1 Play (spec 3). Part A: the live round card. The number picker, ticket builder, cost panel and buy
// button come in part B.
import { useSession } from '@/app/context';
import { RoundCardView } from './play/RoundCard';
import { useRoundCard } from './play/useRoundCard';

export function Play() {
  const ctx = useSession();
  const { load, retry } = useRoundCard(ctx);
  return <RoundCardView load={load} onRetry={retry} />;
}
