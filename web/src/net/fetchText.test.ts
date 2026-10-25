import { describe, expect, it } from 'vitest';
import { PageError } from '@/lib/errors';
import { fetchText } from './fetchText';

function respond(body: BodyInit, init: ResponseInit = {}, length?: string): typeof fetch {
  return (async () => {
    const r = new Response(body, init);
    if (length) r.headers.set('content-length', length);
    return r;
  }) as unknown as typeof fetch;
}

describe('fetchText (page-side size caps, F2)', () => {
  it('reads a body under the cap', async () => {
    expect(await fetchText('http://x/v1/status', 100, { fetch: respond('{"a":1}') })).toEqual({
      status: 200,
      text: '{"a":1}',
    });
  });

  it('refuses a body over the cap, declared or streamed', async () => {
    await expect(fetchText('http://x', 100, { fetch: respond('x'.repeat(101)) })).rejects.toMatchObject({
      code: 'bytes_length',
    });
    await expect(fetchText('http://x', 100, { fetch: respond('x', {}, '1000') })).rejects.toMatchObject({
      code: 'bytes_length',
    });
  });

  it('reports an unreachable indexer and a non-UTF-8 body with their codes', async () => {
    const down = (async () => {
      throw new TypeError('Failed to fetch');
    }) as unknown as typeof fetch;
    const e: unknown = await fetchText('http://x', 100, { fetch: down }).catch((x: unknown) => x);
    expect(e).toBeInstanceOf(PageError);
    expect((e as PageError).code).toBe('indexer_unavailable');
    await expect(
      fetchText('http://x', 100, { fetch: respond(new Uint8Array([0xff, 0xfe]).buffer) }),
    ).rejects.toMatchObject({
      code: 'input_format',
    });
  });
});
