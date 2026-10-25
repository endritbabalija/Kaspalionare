// Errors as typed codes with one message each (spec 1.1 AX). The codes and their messages are the one
// table in Rust (lottery::web::Code), loaded from the worker at start (`errorTable`); the page raises
// codes of the same table for what only it can see (indexer_unavailable, worker_stopped, bytes_length).
import type { ApiError, ErrorEntry, ErrorName, Outcome } from '@wasm/web.js';

const table = new Map<string, { number: number; message: string }>();

export function setErrorTable(rows: readonly ErrorEntry[]): void {
  table.clear();
  for (const r of rows) table.set(r.code, { number: r.number, message: r.message });
}

/** The code's one message (the code's name until the table is loaded). */
export function messageOf(code: ErrorName): string {
  return table.get(code)?.message ?? code;
}

export class PageError extends Error {
  readonly code: ErrorName;
  readonly detail: string;
  constructor(code: ErrorName, detail = '') {
    super(messageOf(code));
    this.name = 'PageError';
    this.code = code;
    this.detail = detail;
  }
}

export function fromApiError(e: ApiError): PageError {
  return new PageError(e.code, e.detail);
}

/** The value of an Outcome, or a PageError with its code. */
export function unwrap<T>(o: Outcome<T>): T {
  if ('ok' in o) return o.ok;
  throw fromApiError(o.error);
}

/** What the page shows for any failure: a PageError's code and message, else the raw text (a bug). */
export function describe(e: unknown): { code: ErrorName | null; message: string; detail: string } {
  if (e instanceof PageError) return { code: e.code, message: e.message, detail: e.detail };
  return { code: null, message: e instanceof Error ? e.message : String(e), detail: '' };
}
