// @vitest-environment node
// F6 part E (owner, DECISIONS 2026-10-28 item 6): several indexer API URLs with fallback. The page tries them in
// order (the one that last answered in this browser first), and moves on when one is unreachable, hangs past
// its deadline, or serves answers the page's checks refuse (chain_mismatch). Each operation reads one indexer
// from start to end, so a check never mixes two indexers' answers. Errors that are not the indexer's (the node
// down, a refused transaction) are never retried on another indexer.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { PageError, unwrap } from '@/lib/errors';
import { checkInstance, type Ctx } from '@/net/round';
import { wasmClient } from '@/test/wasmWorker';
import { IndexerSet, INDEXER_KEY, parseIndexerUrls, withIndexer } from './indexers';

const lottery = wasmClient('testnet-10');
const M8 = readFileSync(path.resolve(process.cwd(), '../docs/launch/tn10-m8.json'), 'utf8');
const M8_COV = 'e9f4f810c86467e7768aa1b42e6c62182b8a0d934bda922bdbb2804f223988d6';
const A = 'https://a.example/v1/';
const B = 'https://b.example/v1/';
const C = 'https://c.example/v1/';

class Mem {
  m = new Map<string, string>();
  getItem(k: string) {
    return this.m.get(k) ?? null;
  }
  setItem(k: string, v: string) {
    this.m.set(k, v);
  }
}

type Serve = 'dead' | 'hang' | 'lie' | 'honest' | 'busy';

/** A fetch for three indexers: `how[base]` decides what each answers; `hits` counts requests per base. */
function net(how: Record<string, Serve>) {
  const hits: Record<string, number> = {};
  const f = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const base = [A, B, C].find((b) => url.startsWith(b)) ?? '?';
    hits[base] = (hits[base] ?? 0) + 1;
    const mode = how[base] ?? 'dead';
    if (mode === 'dead') throw new TypeError('Failed to fetch');
    if (mode === 'hang')
      return new Promise<Response>((_, reject) => {
        init?.signal?.addEventListener('abort', () => reject(init.signal?.reason));
      });
    if (mode === 'busy') return new Response('{"error":{"code":"busy","detail":"over the limit"}}', { status: 503 });
    if (url.endsWith('/instance')) return new Response(mode === 'honest' ? M8 : `${M8} `);
    return new Response('{}', { status: 404 });
  }) as unknown as typeof fetch;
  return { f, hits };
}

async function bundleHash(): Promise<string> {
  return unwrap(await lottery.call('checkIndexerInstance', { bundle: M8, covenantId: M8_COV })).bundleHash;
}

async function ctxWith(urls: string[], how: Record<string, Serve>, storage = new Mem(), timeoutMs = 200) {
  const { f, hits } = net(how);
  const set = new IndexerSet(urls, lottery, { fetch: f, storage, timeoutMs });
  const ctx = {
    config: { covenantId: M8_COV, bundleHash: await bundleHash() },
    lottery,
    indexer: set,
  } as unknown as Ctx;
  return { ctx, set, hits, storage };
}

describe('several indexer URLs with fallback (F6 part E)', () => {
  it('reads the list: comma-separated, trimmed, a trailing slash added, duplicates and bad entries refused', () => {
    expect(parseIndexerUrls(' https://a.example/v1 , https://b.example/v1/ ')).toEqual({ urls: [A, B] });
    expect(parseIndexerUrls(A)).toEqual({ urls: [A] });
    expect(parseIndexerUrls('/api/v1')).toEqual({ urls: ['/api/v1/'] });
    expect(parseIndexerUrls('')).toEqual({ problems: ['VITE_INDEXER_URL is not set'] });
    expect('problems' in parseIndexerUrls(`${A},${A}`)).toBe(true);
    expect('problems' in parseIndexerUrls(`${A},ftp://x.example/`)).toBe(true);
    expect('problems' in parseIndexerUrls('https://u:p@x.example/v1/')).toBe(true);
    expect('problems' in parseIndexerUrls('api/v1')).toBe(true);
    expect('problems' in parseIndexerUrls('//other.example/v1/')).toBe(true);
    expect(
      'problems' in parseIndexerUrls(Array.from({ length: 9 }, (_, i) => `https://h${i}.example/v1/`).join(',')),
    ).toBe(true);
  });

  it('a dead first URL: the page uses the second, and remembers it in this browser', async () => {
    const { ctx, hits, storage } = await ctxWith([A, B], { [A]: 'dead', [B]: 'honest' });
    const i = await withIndexer(ctx, checkInstance);
    expect(i.covenantId).toBe(M8_COV);
    expect(hits[A]).toBe(1);
    expect(storage.getItem(INDEXER_KEY)).toBe(B);
    // A new page load in the same browser asks B first.
    const again = await ctxWith([A, B], { [A]: 'dead', [B]: 'honest' }, storage);
    await withIndexer(again.ctx, checkInstance);
    expect(again.hits[A]).toBeUndefined();
    expect(again.hits[B]).toBe(1);
  });

  it('a first URL that never answers: its deadline passes and the second answers', async () => {
    const { ctx, hits } = await ctxWith([A, B], { [A]: 'hang', [B]: 'honest' });
    expect((await withIndexer(ctx, checkInstance)).covenantId).toBe(M8_COV);
    expect([hits[A], hits[B]]).toEqual([1, 1]);
  });

  it('a lying first URL, whose answer the checks refuse: the page uses the second', async () => {
    const { ctx, hits, storage } = await ctxWith([A, B, C], { [A]: 'lie', [B]: 'honest', [C]: 'honest' });
    expect((await withIndexer(ctx, checkInstance)).covenantId).toBe(M8_COV);
    expect([hits[A], hits[B], hits[C]]).toEqual([1, 1, undefined]);
    expect(storage.getItem(INDEXER_KEY)).toBe(B);
  });

  it('a busy first URL (its error status): the second', async () => {
    const { ctx } = await ctxWith([A, B], { [A]: 'busy', [B]: 'honest' });
    expect((await withIndexer(ctx, checkInstance)).covenantId).toBe(M8_COV);
  });

  it('per request: a method of the set itself also moves on from a dead URL', async () => {
    const { set, hits } = await ctxWith([A, B], { [A]: 'dead', [B]: 'honest' });
    expect(await set.instanceText()).toBe(M8);
    expect([hits[A], hits[B]]).toEqual([1, 1]);
  });

  it('negative controls: one lying URL is refused; every URL down is indexer_unavailable; a lie wins over down', async () => {
    const one = await ctxWith([A], { [A]: 'lie' });
    await expect(withIndexer(one.ctx, checkInstance)).rejects.toMatchObject({ code: 'chain_mismatch' });
    const down = await ctxWith([A, B], { [A]: 'dead', [B]: 'hang' });
    await expect(withIndexer(down.ctx, checkInstance)).rejects.toMatchObject({ code: 'indexer_unavailable' });
    expect(down.storage.getItem(INDEXER_KEY)).toBeNull();
    const mixed = await ctxWith([A, B], { [A]: 'dead', [B]: 'lie' });
    await expect(withIndexer(mixed.ctx, checkInstance)).rejects.toMatchObject({ code: 'chain_mismatch' });
    // The honest one alone passes (the lie above is the bundle's, not the check's).
    const honest = await ctxWith([A], { [A]: 'honest' });
    expect((await withIndexer(honest.ctx, checkInstance)).covenantId).toBe(M8_COV);
  });

  it("an error that is not the indexer's is not retried on another indexer", async () => {
    const { ctx, hits } = await ctxWith([A, B], { [A]: 'honest', [B]: 'honest' });
    let runs = 0;
    await expect(
      withIndexer(ctx, async (c) => {
        runs++;
        await c.indexer.instanceText();
        throw new PageError('node_unavailable', 'the node is down');
      }),
    ).rejects.toMatchObject({ code: 'node_unavailable' });
    expect(runs).toBe(1);
    expect(hits[B]).toBeUndefined();
  });

  it('one operation reads one indexer: inside it, a pinned indexer, never the set', async () => {
    const { ctx } = await ctxWith([A, B], { [A]: 'lie', [B]: 'honest' });
    const seen: string[] = [];
    await withIndexer(ctx, async (c) => {
      expect(c.indexer).not.toBeInstanceOf(IndexerSet);
      seen.push((c.indexer as unknown as { base: string }).base);
      return checkInstance(c);
    });
    expect(seen).toEqual([A, B]);
    // Nested: an operation inside an operation keeps the outer one's indexer.
    const inner: string[] = [];
    await withIndexer(ctx, (c) =>
      withIndexer(c, async (d) => {
        inner.push((d.indexer as unknown as { base: string }).base);
        return checkInstance(d);
      }),
    );
    expect(inner).toEqual([B]);
  });

  it('one URL: as before, and a context without a set runs the operation as it is', async () => {
    const { ctx, hits } = await ctxWith([A], { [A]: 'honest' });
    expect((await withIndexer(ctx, checkInstance)).covenantId).toBe(M8_COV);
    expect(hits[A]).toBe(1);
    const plain = { ...ctx, indexer: { instanceText: async () => M8 } } as unknown as Ctx;
    expect((await withIndexer(plain, checkInstance)).covenantId).toBe(M8_COV);
  });

  it('a stored URL no longer in the list is ignored', async () => {
    const storage = new Mem();
    storage.setItem(INDEXER_KEY, 'https://gone.example/v1/');
    const { ctx, hits } = await ctxWith([A, B], { [A]: 'honest', [B]: 'honest' }, storage);
    await withIndexer(ctx, checkInstance);
    expect([hits[A], hits[B]]).toEqual([1, undefined]);
  });
});
