// DEV ONLY (simnet dev run): the dev faucet button for the quick wallet (crates/simnet/src/faucet.rs pays 100
// KAS from the dev run's throwaway wallet). Imported behind import.meta.env.DEV with a configured faucet;
// scripts/check-bundle.mjs fails a production build on the faucet marker or path.
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { requestFaucet } from './faucet';

export default function DevFaucet({
  faucetUrl,
  address,
  onPaid,
}: {
  faucetUrl: string;
  address: string;
  onPaid: () => void;
}) {
  const [state, setState] = useState<'idle' | 'paying' | 'paid' | 'failed'>('idle');
  const [txid, setTxid] = useState<string | null>(null);
  return (
    <div className="flex flex-col gap-1">
      <Button
        data-testid="dev-faucet"
        data-state={state}
        disabled={state === 'paying'}
        onClick={() => {
          setState('paying');
          requestFaucet(faucetUrl, address)
            .then((p) => {
              setTxid(p.transactionId);
              setState('paid');
              onPaid();
            })
            .catch((e: unknown) => {
              console.warn('dev faucet', e);
              setState('failed');
            });
        }}
      >
        Dev faucet
      </Button>
      {txid && (
        <span data-testid="dev-faucet-tx" data-value={txid} className="font-mono text-[12px] text-graphite">
          {txid.slice(0, 12)}…
        </span>
      )}
    </div>
  );
}
