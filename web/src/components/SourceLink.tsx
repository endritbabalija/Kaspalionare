// The on-chain source of a figure (spec 5 rule 1, S3 "every figure tappable through to its on-chain source"): a
// link to the transaction that holds it in the explorer the build names; nothing when it names none.
import { explorerLink } from '@/lib/explorer';

export function SourceLink({
  tx,
  name,
  explorer,
  testId,
}: {
  tx?: string;
  name: string;
  explorer?: string;
  testId: string;
}) {
  const href = tx ? explorerLink(explorer, 'transaction', tx) : undefined;
  if (!href) return null;
  return (
    <a
      href={href}
      target="_blank"
      rel="noreferrer noopener"
      data-testid={testId}
      aria-label={`${name} in the explorer`}
      className="inline-flex h-11 w-11 shrink-0 items-center justify-center rounded-lg"
    >
      <svg
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        aria-hidden="true"
      >
        <path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" />
      </svg>
    </a>
  );
}
