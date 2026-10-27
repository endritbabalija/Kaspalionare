// Deposit request (spec 3 S5, 7; J5): an amount of at least MIN_DEPOSIT, for the wallet's own address (the only
// owner the page can later withdraw for). Once built, the rows of the very transaction the press signs
// (`depositFigures`, bound to it): the deposit, the reserve R and when it comes back, the fee, the total, the owner
// and the terms (irrevocable; converts at the next draw's price; can come back as a refund).
import type { DepositFigures } from '@wasm/web.js';
import { Amount } from './Amount';
import { FigureRow } from './FigureRow';
import { HashOrAddress } from './HashOrAddress';
import { VaultRequestRows } from './VaultRequestRows';
import type { ActionView } from './vaultStates';

export interface DepositPanelProps extends ActionView<DepositFigures> {
  minDeposit: string;
  amount: string;
  onAmount: (text: string) => void;
  requestOpen: boolean;
  requestFrom: string;
  clock: { virtualDaa: string; observedAt: number };
  walletConnected: boolean;
  disabled?: boolean;
  onRequest: () => void;
  explorer?: string;
  testId?: string;
}

export function DepositPanel(p: DepositPanelProps) {
  const id = p.testId ?? 'deposit';
  const f = p.figures;
  return (
    <section
      data-testid={id}
      aria-labelledby={`${id}-title`}
      className="flex flex-col gap-0.5 rounded-[14px] border border-rule bg-surface p-4"
    >
      <h2 id={`${id}-title`} className="m-0 pb-1 text-[15px] font-semibold">
        Deposit
      </h2>
      <FigureRow name="Amount" testId={`${id}-amount-row`}>
        <label className="flex items-center gap-2">
          <span className="sr-only">Deposit amount in KAS</span>
          <input
            data-testid={`${id}-amount`}
            inputMode="decimal"
            autoComplete="off"
            value={p.amount}
            onChange={(e) => p.onAmount(e.target.value)}
            disabled={!p.walletConnected}
            className="h-11 w-32 rounded-[10px] border border-control-line bg-ground px-3 text-right font-mono text-[15px]"
          />
          <span aria-hidden="true">KAS</span>
        </label>
      </FigureRow>
      <FigureRow name="Least" testId={`${id}-least-row`} part>
        <Amount sompi={p.minDeposit} testId={`${id}-least`} />
      </FigureRow>
      <VaultRequestRows
        {...p}
        testId={id}
        figures={f}
        refundable={f?.refundable}
        label="Request deposit"
        onRequest={p.onRequest}
      >
        {f && (
          <>
            <FigureRow name="Deposit" testId={`${id}-deposit-row`}>
              <Amount sompi={f.amount} testId={`${id}-deposit`} />
            </FigureRow>
            <FigureRow name="Owner" testId={`${id}-owner-row`}>
              <HashOrAddress value={f.owner} kind="address" explorer={p.explorer} testId={`${id}-owner`} />
            </FigureRow>
          </>
        )}
      </VaultRequestRows>
    </section>
  );
}
