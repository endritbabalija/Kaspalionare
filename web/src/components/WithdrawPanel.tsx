// Withdrawal request (spec 3 S5, 7; J5; design 5.4): the whole position, or a part w >= wmin that leaves a
// remainder >= wmin. The least and most a part may be are the wasm's (`vaultView`'s range: the amounts it takes,
// Least rounded up and Most rounded down to 0.01 KAS where the range allows, printed exactly so that typed back
// each is taken; F6 part A review S1); an amount outside them is refused by the wasm (`vault_amount`) and nothing
// is built. Once built, the rows of the very
// transaction the press signs (`withdrawFigures`, bound to it): the shares and their worth today, the position
// left, the reserve a part posts and when it comes back, the fee, the total and the terms.
import type { PositionCard, WithdrawFigures } from '@wasm/web.js';
import { Amount } from './Amount';
import { FigureRow } from './FigureRow';
import { Shares } from './Shares';
import { Button } from './ui/button';
import { VaultRequestRows } from './VaultRequestRows';
import type { ActionView } from './vaultStates';

export type WithdrawMode = 'whole' | 'part';

export interface WithdrawPanelProps extends ActionView<WithdrawFigures> {
  position: PositionCard;
  mode: WithdrawMode;
  onMode: (m: WithdrawMode) => void;
  amount: string;
  onAmount: (text: string) => void;
  requestOpen: boolean;
  requestFrom: string;
  clock: { virtualDaa: string; observedAt: number };
  walletConnected: boolean;
  disabled?: boolean;
  onRequest: () => void;
  explorer?: string;
  testId: string;
}

export function WithdrawPanel(p: WithdrawPanelProps) {
  const id = p.testId;
  const f = p.figures;
  const range = p.position.partial;
  return (
    <div data-testid={id} data-mode={p.mode} className="mt-1 flex flex-col gap-0.5 border-t border-rule-soft pt-1">
      <div role="group" aria-label="Withdraw" className="flex gap-2 py-1">
        <Button
          variant={p.mode === 'whole' ? 'primary' : 'secondary'}
          aria-pressed={p.mode === 'whole'}
          data-testid={`${id}-whole`}
          onClick={() => p.onMode('whole')}
        >
          Whole
        </Button>
        {range && (
          <Button
            variant={p.mode === 'part' ? 'primary' : 'secondary'}
            aria-pressed={p.mode === 'part'}
            data-testid={`${id}-part`}
            onClick={() => p.onMode('part')}
          >
            Part
          </Button>
        )}
      </div>
      {p.mode === 'part' && range && (
        <>
          <FigureRow name="Amount" testId={`${id}-amount-row`}>
            <label className="flex items-center gap-2">
              <span className="sr-only">Withdrawal amount in KAS</span>
              <input
                data-testid={`${id}-amount`}
                inputMode="decimal"
                autoComplete="off"
                value={p.amount}
                onChange={(e) => p.onAmount(e.target.value)}
                className="h-11 w-32 rounded-[10px] border border-control-line bg-ground px-3 text-right font-mono text-[15px]"
              />
              <span aria-hidden="true">KAS</span>
            </label>
          </FigureRow>
          <FigureRow name="Least" testId={`${id}-least-row`} part>
            <Amount sompi={range.leastShown} exact testId={`${id}-least`} />
          </FigureRow>
          <FigureRow name="Most" testId={`${id}-most-row`} part>
            <Amount sompi={range.mostShown} exact testId={`${id}-most`} />
          </FigureRow>
        </>
      )}
      <VaultRequestRows {...p} testId={id} figures={f} label="Request withdrawal" onRequest={p.onRequest}>
        {f && (
          <>
            <FigureRow name="Shares" testId={`${id}-shares-row`}>
              <Shares value={f.shares} testId={`${id}-shares`} />
            </FigureRow>
            <FigureRow name="Worth today" testId={`${id}-worth-row`} part>
              <Amount sompi={f.worth} testId={`${id}-worth`} />
            </FigureRow>
            {!f.whole && (
              <FigureRow name="Position left" testId={`${id}-left-row`}>
                <Shares value={f.remainder} testId={`${id}-left`} />
              </FigureRow>
            )}
          </>
        )}
      </VaultRequestRows>
    </div>
  );
}
