// Reading an indexer response as text with a size cap (spec 6.3, F2: the page caps what it parses, and
// parses it in the worker with parseIndexer*, never JSON.parse). The body stream is read up to the cap
// and abandoned beyond it, so a huge answer never sits in the page's memory.
import { PageError } from '@/lib/errors';

export interface TextResponse {
  status: number;
  text: string;
}

export async function fetchText(
  url: string,
  cap: number,
  init?: { signal?: AbortSignal; fetch?: typeof fetch },
): Promise<TextResponse> {
  const f = init?.fetch ?? fetch;
  let res: Response;
  try {
    res = await f(url, {
      method: 'GET',
      signal: init?.signal,
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
