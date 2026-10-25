// Reading an indexer response as text with a size cap (spec 6.3, F2: the page caps what it parses, and
// parses it in the worker with parseIndexer*, never JSON.parse). The body stream is read up to the cap
// and abandoned beyond it, so a huge answer never sits in the page's memory.
import { PageError } from '@/lib/errors';

/** How long one indexer response may take, headers and body (F2/F3 review note: never wait forever). */
export const FETCH_TIMEOUT_MS = 15_000;

export interface TextResponse {
  status: number;
  text: string;
}

export async function fetchText(
  url: string,
  cap: number,
  init?: { signal?: AbortSignal; fetch?: typeof fetch; timeoutMs?: number },
): Promise<TextResponse> {
  const f = init?.fetch ?? fetch;
  const ms = init?.timeoutMs ?? FETCH_TIMEOUT_MS;
  const timer = new AbortController();
  const t = setTimeout(() => timer.abort(new DOMException(`no answer after ${ms / 1000} s`, 'TimeoutError')), ms);
  const signal = init?.signal ? AbortSignal.any([init.signal, timer.signal]) : timer.signal;
  try {
    return await read(f, url, cap, signal);
  } catch (e) {
    if (signal.aborted && !(e instanceof PageError && e.code === 'bytes_length')) {
      throw new PageError('indexer_unavailable', `${url}: ${String(signal.reason ?? e)}`);
    }
    throw e;
  } finally {
    clearTimeout(t);
  }
}

async function read(f: typeof fetch, url: string, cap: number, signal: AbortSignal): Promise<TextResponse> {
  let res: Response;
  try {
    res = await f(url, {
      method: 'GET',
      signal,
      credentials: 'omit',
      cache: 'no-store',
      redirect: 'error',
    });
  } catch (e) {
    throw new PageError('indexer_unavailable', `${url}: ${String(e)}`);
  }
  const declared = Number(res.headers.get('content-length') ?? 'NaN');
  if (Number.isFinite(declared) && declared > cap) {
    await res.body?.cancel().catch(() => undefined);
    throw new PageError('bytes_length', `${url}: ${declared} bytes declared, the cap is ${cap}`);
  }
  if (!res.body) return { status: res.status, text: '' };
  const reader = res.body.getReader();
  const chunks: Uint8Array[] = [];
  let n = 0;
  for (;;) {
    let r: ReadableStreamReadResult<Uint8Array>;
    try {
      r = await reader.read();
    } catch (e) {
      throw new PageError('indexer_unavailable', `${url}: ${String(e)}`);
    }
    if (r.done) break;
    n += r.value.byteLength;
    if (n > cap) {
      await reader.cancel().catch(() => undefined);
      throw new PageError('bytes_length', `${url}: more than ${cap} bytes`);
    }
    chunks.push(r.value);
  }
  const all = new Uint8Array(n);
  let o = 0;
  for (const c of chunks) {
    all.set(c, o);
    o += c.byteLength;
  }
  let text: string;
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(all);
  } catch {
    throw new PageError('input_format', `${url}: not UTF-8`);
  }
  return { status: res.status, text };
}
