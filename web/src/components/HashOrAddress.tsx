// Hash or address (spec 4): shortened mono, a copy button and, when the build names an explorer, a link
// to it (rule 1: every figure links to where it lives on-chain).
import { useState } from 'react';
import { cn } from '@/lib/cn';
import { explorerLink, type HashKind } from '@/lib/explorer';
import { shortHash } from '@/lib/format';

export interface HashOrAddressProps {
  value: string;
  kind: HashKind;
  explorer?: string;
  className?: string;
  testId?: string;
}

export function HashOrAddress({ value, kind, explorer, className, testId = 'hash' }: HashOrAddressProps) {
  const [copied, setCopied] = useState<'idle' | 'copied' | 'failed'>('idle');
  const href = explorerLink(explorer, kind, value);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied('copied');
    } catch {
      setCopied('failed');
    }
    setTimeout(() => setCopied('idle'), 1500);
  };
  return (
    <span
      data-testid={testId}
      data-value={value}
      className={cn('inline-flex items-center gap-1 font-mono text-[13px]', className)}
    >
      <span title={value}>{shortHash(value)}</span>
      <button
        type="button"
        data-testid={`${testId}-copy`}
        aria-label={`Copy ${kind}`}
        onClick={copy}
        className="inline-flex h-11 w-11 items-center justify-center rounded-lg text-graphite hover:text-ink"
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
          <rect x="8" y="8" width="12" height="12" rx="2" />
          <path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" />
        </svg>
      </button>
      {href && (
        <a
          href={href}
          target="_blank"
          rel="noreferrer noopener"
          data-testid={`${testId}-explorer`}
          aria-label={`Open ${kind} in the explorer`}
          className="inline-flex h-11 w-11 items-center justify-center rounded-lg"
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
      )}
      <span role="status" className="text-graphite">
        {copied === 'copied' ? 'Copied' : copied === 'failed' ? 'Copy failed' : ''}
      </span>
    </span>
  );
}
