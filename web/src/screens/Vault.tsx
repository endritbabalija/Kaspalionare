// S5 Vault (spec 3 S5, 4, 7; J5; F6 part A): the vault's figures, the connected wallet's positions and requests
// (each with its withdrawal or settlement), and the deposit request. States: loading, error, ready, stale (the
// last checked figures, when a later read failed), empty (no positions or requests: count rows of 0).
import { useSession } from '@/app/context';
import { DepositPanel } from '@/components/DepositPanel';
import { FigureRow } from '@/components/FigureRow';
import { HashOrAddress } from '@/components/HashOrAddress';
import { SettleRows } from '@/components/SettleRows';
import { TxProgress } from '@/components/TxProgress';
import { Button } from '@/components/ui/button';
import { VaultFigures } from '@/components/VaultFigures';
import { VaultPositionCard } from '@/components/VaultPositionCard';
import { WithdrawPanel } from '@/components/WithdrawPanel';
import { useWallet } from '@/wallet/context';
import { useVault } from './vault/useVault';

const ACTION: Record<string, string> = { deposit: 'Deposit', withdraw: 'Withdrawal' };

export function Vault() {
  const ctx = useSession();
  const wallet = useWallet();
  const v = useVault(ctx, wallet);
  const explorer = ctx.config.explorerUrl;
  if (v.load.kind === 'loading' || (v.load.kind === 'ready' && !v.data)) {
    return (
      <section
        data-testid="vault"
        data-state="loading"
        aria-busy="true"
        aria-label="Vault"
        className="flex flex-col gap-3"
      >
        <h1 className="m-0 text-[17px] font-semibold">Vault</h1>
        <span className="h-64 w-full animate-pulse rounded-[14px] bg-rule-soft" />
        <span className="sr-only">Loading</span>
      </section>
    );
  }
  if (v.load.kind === 'error') {
    return (
      <section data-testid="vault" data-state="error" aria-label="Vault" className="flex flex-col gap-3">
        <h1 className="m-0 text-[17px] font-semibold">Vault</h1>
        <p role="alert" data-testid="vault-error" data-code={v.load.code ?? ''} className="m-0 text-amber">
          {v.load.message}
        </p>
        <Button data-testid="vault-retry" onClick={v.reload}>
          Retry
        </Button>
      </section>
    );
  }
  const data = v.data;
  if (!data) return null;
  const { page, clock } = data;
  const s = page.vault;
  const busy = v.running !== null;
  return (
    <section
      data-testid="vault"
      data-state={v.load.stale ? 'stale' : 'ready'}
      data-request-open={String(s.requestOpen)}
      aria-label="Vault"
      className="flex flex-col gap-3.5"
    >
      <VaultFigures vault={s} clock={clock} explorer={explorer} stale={v.load.stale} />
      {v.last && (
        <section
          data-testid="vault-last"
          data-action={v.last.name.split(':')[0]}
          data-step={v.last.tx.step}
          aria-label="Last transaction"
          className="flex flex-col gap-1 rounded-[14px] border border-rule bg-surface p-4"
        >
          <h2 className="m-0 text-[15px] font-semibold">{ACTION[v.last.name] ?? 'Settlement'}</h2>
          <TxProgress step={v.last.tx.step} failed={v.last.tx.failed} checking={v.last.tx.checking} />
          {v.last.tx.id && (
            <HashOrAddress value={v.last.tx.id} kind="transaction" explorer={explorer} testId="vault-last-txid" />
          )}
        </section>
      )}
      {!v.walletConnected ? (
        <a href="#/wallet" data-testid="vault-connect" className="flex min-h-11 items-center font-medium">
          Connect
        </a>
      ) : (
        <>
          <section data-testid="vault-mine" aria-labelledby="vault-mine-title" className="flex flex-col gap-2.5">
            <h2 id="vault-mine-title" className="m-0 text-[15px] font-semibold">
              Positions
            </h2>
            {page.positions.length === 0 && (
              <FigureRow name="Positions" testId="vault-positions-none">
                <span data-testid="vault-positions-count">0</span>
              </FigureRow>
            )}
            {page.positions.map((p, i) => {
              const id = `vault-position-${i}`;
              const open = v.withdraw.isOpen(p);
              return (
                <VaultPositionCard
                  key={`${p.object.utxo.transactionId}:${p.object.utxo.index}`}
                  position={p}
                  testId={id}
                  explorer={explorer}
                >
                  {p.ownerKey && !open && (
                    <Button data-testid={`${id}-withdraw`} aria-expanded={false} onClick={() => v.withdraw.open(p)}>
                      Withdraw
                    </Button>
                  )}
                  {p.ownerKey && open && (
                    <WithdrawPanel
                      testId={`${id}-withdraw-panel`}
                      position={p}
                      mode={v.withdraw.form.mode}
                      onMode={v.withdraw.onMode}
                      amount={v.withdraw.form.amount}
                      onAmount={v.withdraw.onAmount}
                      {...v.withdraw.view}
                      requestOpen={s.requestOpen}
                      requestFrom={s.requestFrom}
                      clock={clock}
                      walletConnected={v.walletConnected}
                      disabled={busy}
                      onRequest={v.withdraw.onPress}
                      explorer={explorer}
                    />
                  )}
                </VaultPositionCard>
              );
            })}
            <h2 className="m-0 pt-1 text-[15px] font-semibold">Requests</h2>
            {page.requests.length === 0 && (
              <FigureRow name="Requests" testId="vault-requests-none">
                <span data-testid="vault-requests-count">0</span>
              </FigureRow>
            )}
            {page.requests.map((r, i) => {
              const id = `vault-request-${i}`;
              const st = v.settle(r);
              return (
                <VaultPositionCard
                  key={`${r.object.utxo.transactionId}:${r.object.utxo.index}`}
                  request={r}
                  testId={id}
                  explorer={explorer}
                >
                  {r.status === 'settleDue' && r.settle && (
                    <SettleRows
                      testId={`${id}-settle`}
                      view={r.settle}
                      round={r.round}
                      {...st.view}
                      walletConnected={v.walletConnected}
                      disabled={busy}
                      onSettle={st.onPress}
                      explorer={explorer}
                    />
                  )}
                </VaultPositionCard>
              );
            })}
          </section>
          <DepositPanel
            minDeposit={s.minDeposit}
            amount={v.deposit.amount}
            onAmount={v.deposit.onAmount}
            {...v.deposit.view}
            requestOpen={s.requestOpen}
            requestFrom={s.requestFrom}
            clock={clock}
            walletConnected={v.walletConnected}
            disabled={busy}
            onRequest={v.deposit.onPress}
            explorer={explorer}
          />
        </>
      )}
    </section>
  );
}
