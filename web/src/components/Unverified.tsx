// A status word (spec 5 rule 6): the figure beside it is the indexer's, which no live chain object fixes.
export function Unverified({ testId }: { testId: string }) {
  return (
    <span
      data-testid={testId}
      className="inline-flex h-6 items-center rounded-full border border-control-line px-2 font-mono text-[11px] text-graphite"
    >
      Unverified
    </span>
  );
}
