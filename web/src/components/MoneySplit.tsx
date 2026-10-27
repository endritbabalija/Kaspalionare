// Where a ticket's price goes (How it works; design 5.1): one bar split in proportion and one row per part, each
// a figure (name and exact KAS). The parts come from the Rust lottery code (the How it works model); the widths
// are only their share of the total for drawing. Meaning never by colour alone: every part is a named row.
import { Amount } from './Amount';
import { FigureRow } from './FigureRow';

export interface SplitPart {
  key: string;
  name: string;
  /** Sompi, a decimal string. */
  sompi: string;
  /** Tailwind background class of a token (bg-teal, bg-mint, …). */
  fill: string;
}

export function MoneySplit({ parts, testId }: { parts: SplitPart[]; testId: string }) {
  const total = parts.reduce((a, p) => a + Number(p.sompi), 0);
  return (
    <div data-testid={testId} className="flex flex-col gap-1">
      <div aria-hidden="true" className="flex h-3 w-full overflow-hidden rounded-full bg-rule-soft">
        {parts.map((p) => (
          <span
            key={p.key}
            className={`${p.fill} h-full border-r border-surface last:border-r-0`}
            style={{
              width: `${total > 0 ? (Number(p.sompi) / total) * 100 : 0}%`,
              minWidth: Number(p.sompi) > 0 ? 3 : 0,
            }}
          />
        ))}
      </div>
      {parts.map((p) => (
        <FigureRow key={p.key} name={p.name} testId={`${testId}-${p.key}`}>
          <span aria-hidden="true" className={`${p.fill} mr-2 inline-block size-2.5 rounded-sm`} />
          <Amount sompi={p.sompi} exact testId={`${testId}-${p.key}-amount`} />
        </FigureRow>
      ))}
    </div>
  );
}
