// A figure not built yet (spec 4 "Cost panel": empty, estimating): a dash, and "Estimating" for screen readers.
export function Estimating({ estimating, testId }: { estimating?: boolean; testId: string }) {
  return (
    <span data-testid={testId} aria-busy={estimating ? 'true' : undefined} className="text-graphite">
      <span aria-hidden="true">–</span>
      <span className="sr-only">{estimating ? 'Estimating' : 'none'}</span>
    </span>
  );
}
