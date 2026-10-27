// The terms of a vault request as figure rows (spec 3 S5, 5 rule 6: a fact the player needs is a value, not a
// sentence): revocable or not, the draw whose price converts it, and whether it can come back as a refund. Every
// value is the wasm's (`depositFigures`, `withdrawFigures`).
import { FigureRow } from './FigureRow';

export function VaultTerms({
  revocable,
  round,
  refundable,
  testId,
}: {
  revocable: boolean;
  round: string;
  /** A deposit: it can come back as a refund (refund mode). Absent for a withdrawal. */
  refundable?: boolean;
  testId: string;
}) {
  return (
    <>
      <FigureRow name="Revocable" testId={`${testId}-revocable-row`}>
        <span data-testid={`${testId}-revocable`} data-value={String(revocable)}>
          {revocable ? 'Yes' : 'No'}
        </span>
      </FigureRow>
      <FigureRow name="Price" testId={`${testId}-price-row`}>
        <span data-testid={`${testId}-price`} data-value={round}>
          Round {round} draw
        </span>
      </FigureRow>
      {refundable !== undefined && (
        <FigureRow name="Refund" testId={`${testId}-refund-row`}>
          <span data-testid={`${testId}-refund`} data-value={String(refundable)}>
            {refundable ? 'Possible' : 'No'}
          </span>
        </FigureRow>
      )}
    </>
  );
}
