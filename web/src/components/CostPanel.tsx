// Cost panel shell (spec 4, 7): empty, estimating, ready, fee-too-high warning. Rows are figures with their
// names (rule 6: a cost row is data); the amounts come from the built transaction (part B fills them from
// BuiltTx), never from separate arithmetic.
import { useId, type ReactNode } from 'react';
import { Amount } from './Amount';

export type CostPanelState = 'empty' | 'estimating' | 'ready' | 'fee-too-high';

export interface CostRow {
  id: string;
  name: string;
  /** Sompi, when known. */
  sompi?: string;
  /** A fact shown as a value next to the amount (for example the deposit's return date). */
  note?: ReactNode;
}

export interface CostPanelProps {
  state: CostPanelState;
  rows: CostRow[];
  total?: string;
  /** The warning's text (an error message) in the fee-too-high state. */
  warning?: string;
}

export function CostPanel({ state, rows, total, warning }: CostPanelProps) {
  const title = useId();
  return (
    <section
      data-testid="cost-panel"
      data-state={state}
      aria-labelledby={title}
      aria-busy={state === 'estimating'}
      className="flex flex-col gap-1 rounded-[14px] border border-rule bg-surface p-4 text-sm"
    >
      <h2 id={title} className="m-0 mb-1 text-[17px] font-semibold">
        What you pay
      </h2>
      {rows.map((r) => (
        <div key={r.id} data-testid={`cost-row-${r.id}`} className="flex min-h-11 items-center justify-between gap-3">
          <span>{r.name}</span>
          <span className="flex items-center gap-2">
            {r.note && <span className="font-mono text-[13px] text-graphite">{r.note}</span>}
            <Value state={state} sompi={r.sompi} testId={`cost-${r.id}`} />
          </span>
        </div>
      ))}
      <div className="flex min-h-11 items-center justify-between border-t border-rule pt-1 font-semibold">
        <span>Total</span>
        <Value state={state} sompi={total} testId="cost-total" />
      </div>
      {state === 'fee-too-high' && warning && (
        <p role="alert" data-testid="cost-warning" className="m-0 text-amber">
          {warning}
        </p>
      )}
    </section>
  );
}

function Value({ state, sompi, testId }: { state: CostPanelState; sompi?: string; testId: string }) {
  if (state === 'estimating') {
    return (
      <span data-testid={testId} className="inline-block h-4 w-16 animate-pulse rounded bg-rule-soft">
        <span className="sr-only">Estimating</span>
      </span>
    );
  }
  if (state === 'empty' || sompi === undefined) {
    return (
      <span data-testid={testId} className="font-mono text-graphite">
        <span aria-hidden="true">–</span>
        <span className="sr-only">none</span>
      </span>
    );
  }
  return <Amount sompi={sompi} testId={testId} />;
}
