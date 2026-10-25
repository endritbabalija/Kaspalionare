// The quick testnet wallet's part of the wallet sheet (spec S7): create, use, import and export the key; the
// permanent line that the key is kept in this browser; the faucet link (testnet-10) or, on the simnet dev
// run, the dev faucet. Loaded only in a build with the quick wallet (wallet/quickEnabled.ts).
// Importing another key while one is stored first shows the stored key (exported) and asks for Replace; only
// that press replaces it, and only if it is still the key shown (F2/F3 review S1: no key is lost silently).
import { lazy, Suspense, useId, useState } from 'react';
import { useSession } from '@/app/context';
import { Button } from '@/components/ui/button';
import { describe } from '@/lib/errors';
import { useWallet } from '@/wallet/context';

// DEV ONLY: the simnet dev faucet (never in a production build: check-bundle.mjs).
const DevFaucet = import.meta.env.DEV ? lazy(() => import('@/dev/DevFaucet')) : null;

const card = 'flex flex-col gap-2 rounded-[14px] border border-rule bg-surface p-4';

export default function QuickWalletPanel({ onFunded }: { onFunded: () => void }) {
  const ctx = useSession();
  const w = useWallet();
  const title = useId();
  const importId = useId();
  const [secret, setSecret] = useState('');
  const [exported, setExported] = useState<{ address: string; secretKey: string } | null>(null);
  /** The stored key the player must see before an import replaces it (its address). */
  const [replacing, setReplacing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const isQuick = w.state === 'quick';

  const doExport = async () => {
    try {
      const k = await w.exportQuick();
      setExported(k);
      setError(null);
      return k;
    } catch (e) {
      setError(describe(e).message);
      return null;
    }
  };

  const doImport = async () => {
    if (!w.quickStored) {
      if (await w.importQuick(secret, null)) setSecret('');
      return;
    }
    if (replacing === null) {
      // Show the stored key first; the import waits for Replace.
      const k = await doExport();
      if (k) setReplacing(k.address);
      return;
    }
    if (await w.importQuick(secret, replacing)) {
      setSecret('');
      setExported(null);
    }
    setReplacing(null);
  };

  return (
    <section aria-labelledby={title} data-testid="quick-wallet" className={card}>
      <h2 id={title} className="m-0 text-[17px] font-semibold">
        Quick testnet wallet
      </h2>
      <p data-testid="quick-wallet-warning" className="m-0 text-[13px] text-amber">
        Testnet key stored in this browser; anyone with this device can spend it
      </p>
      {ctx.config.faucetLink && (
        <a
          href={ctx.config.faucetLink}
          target="_blank"
          rel="noreferrer noopener"
          data-testid="faucet-link"
          className="flex min-h-11 items-center text-[13px] font-medium"
        >
          Faucet
        </a>
      )}
      {!isQuick && (
        <Button
          variant="primary"
          data-testid="quick-create"
          disabled={w.state === 'connecting'}
          onClick={() => void (w.quickStored ? w.openQuick() : w.createQuick())}
        >
          {w.quickStored ? 'Use quick wallet' : 'Create quick wallet'}
        </Button>
      )}
      {isQuick && DevFaucet && ctx.config.faucetUrl && w.address && (
        <Suspense fallback={null}>
          <DevFaucet faucetUrl={ctx.config.faucetUrl} address={w.address} onPaid={onFunded} />
        </Suspense>
      )}
      {w.quickStored && (
        <Button data-testid="quick-export" onClick={() => void doExport()}>
          Export key
        </Button>
      )}
      {exported && (
        <div className="flex min-h-11 items-center justify-between gap-2">
          <code data-testid="quick-exported" className="font-mono text-[12px] break-all">
            {exported.secretKey}
          </code>
          <Button variant="ghost" data-testid="quick-exported-hide" onClick={() => setExported(null)}>
            Hide
          </Button>
        </div>
      )}
      <form
        className="flex flex-col gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void doImport();
        }}
      >
        <label htmlFor={importId} className="text-[13px] text-graphite">
          Import key
        </label>
        <input
          id={importId}
          data-testid="quick-import-input"
          value={secret}
          onChange={(e) => {
            setSecret(e.target.value);
            setReplacing(null);
          }}
          autoComplete="off"
          spellCheck={false}
          inputMode="text"
          className="min-h-11 rounded-[10px] border border-control-line bg-ground px-3 font-mono text-[13px] text-ink"
        />
        <Button
          type="submit"
          data-testid="quick-import"
          data-replacing={replacing ?? undefined}
          variant={replacing ? 'primary' : undefined}
          disabled={!secret.trim() || w.state === 'connecting'}
        >
          {replacing ? 'Replace' : 'Import'}
        </Button>
      </form>
      {error && (
        <p role="alert" data-testid="quick-error" className="m-0 text-amber">
          {error}
        </p>
      )}
    </section>
  );
}
