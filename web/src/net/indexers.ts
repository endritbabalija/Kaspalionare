// Several indexer API URLs with fallback (F6 part E; owner, DECISIONS 2026-10-28 item 6). VITE_INDEXER_URL holds
// one URL or a comma-separated list. The page asks them in order, the one that last answered in this browser
// first (localStorage kl.indexer), and moves on from one that is unreachable, hangs past its deadline, answers
// with an error status (busy included) or serves answers the page's checks refuse (chain_mismatch and the
// other codes in FALLBACK). Nothing an indexer says is trusted more for it: every answer is still checked
// against the node (spec 6.3); the list only keeps a dead or lying indexer from stopping the page.
//
// - `withIndexer(ctx, op)` runs a whole read-and-check operation on one indexer (so a check never mixes two
//   indexers' answers), then on the next one if it fails with a code of FALLBACK. Only read and build steps are
//   wrapped; nothing that signs or sends runs inside it.
// - The set itself (ctx.indexer) also falls back per request, for the call sites that read one answer.
import { PageError } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import type { LotteryClient } from '@/worker/client';
import { Indexer } from './indexer';

/** The indexer that last answered in this browser (a configured URL, exactly as the build lists it). */
export const INDEXER_KEY = 'kl.indexer';
/** At most this many URLs (each https origin goes into the CSP's connect-src). */
export const INDEXER_URLS_MAX = 8;

/** The codes that are the indexer's fault: another indexer may answer. Anything else is thrown at once. */
export const FALLBACK: ReadonlySet<string> = new Set([
  'indexer_unavailable',
  'chain_mismatch',
  'bytes_length',
  'input_format',
  'store_format',
  'anchor_invalid',
]);

/** The page's calls to an indexer (a single one, or the set). */
export type IndexerApi = Pick<
  Indexer,
  | 'status'
  | 'instanceText'
  | 'rounds'
  | 'round'
  | 'purchases'
  | 'pools'
  | 'tickets'
  | 'vault'
  | 'positions'
  | 'requests'
  | 'anchorProof'
>;

type Store = Pick<Storage, 'getItem' | 'setItem'>;

function browserStorage(): Store | undefined {
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}

/**
 * VITE_INDEXER_URL as a list: comma-separated, each an absolute http(s) URL without credentials or a same-origin
 * path ("/api/v1/", the Vercel proxy of docs/launch/site-deploy.md); a trailing slash is added; no duplicates.
 */
export function parseIndexerUrls(raw: string | undefined): { urls: string[] } | { problems: string[] } {
  const parts = (raw ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  if (!parts.length) return { problems: ['VITE_INDEXER_URL is not set'] };
  const problems: string[] = [];
  if (parts.length > INDEXER_URLS_MAX) problems.push(`VITE_INDEXER_URL lists more than ${INDEXER_URLS_MAX} URLs`);
  const urls: string[] = [];
  for (const p of parts) {
    const u = p.endsWith('/') ? p : `${p}/`;
    if (u.startsWith('/') && !u.startsWith('//')) {
      urls.push(u);
    } else {
      let ok = false;
      try {
        const x = new URL(u);
        ok = (x.protocol === 'https:' || x.protocol === 'http:') && !x.username && !x.password && x.host.length > 0;
      } catch {
        ok = false;
      }
      if (!ok) problems.push(`VITE_INDEXER_URL entry ${JSON.stringify(p)} is not an http(s) URL or a /path`);
      else urls.push(u);
    }
  }
  if (new Set(urls).size !== urls.length) problems.push('VITE_INDEXER_URL lists a URL twice');
  return problems.length ? { problems } : { urls };
}

function resolve(u: string): string {
  if (!u.startsWith('/')) return u;
  const here = typeof location !== 'undefined' ? location.href : undefined;
  if (!here) throw new PageError('indexer_unavailable', `${u}: a same-origin path needs a page`);
  return new URL(u, here).toString();
}

export class IndexerSet implements IndexerApi {
  private readonly entries: { url: string; indexer: Indexer }[];
  private readonly storage: Store | undefined;

  constructor(
    readonly urls: readonly string[],
    lottery: LotteryClient,
    opts: { fetch?: typeof fetch; storage?: Store; timeoutMs?: number } = {},
  ) {
    if (!urls.length) throw new PageError('indexer_unavailable', 'no indexer URL');
    this.entries = urls.map((url) => ({
      url,
      indexer: new Indexer(resolve(url), lottery, opts.fetch, opts.timeoutMs),
    }));
    this.storage = 'storage' in opts ? opts.storage : browserStorage();
  }

  /** The indexers in the order to ask them: the last good one in this browser, then the configured order. */
  order(): Indexer[] {
    let last: string | null = null;
    try {
      last = this.storage?.getItem(INDEXER_KEY) ?? null;
    } catch {
      last = null;
    }
    const i = this.entries.findIndex((e) => e.url === last);
    const all = this.entries.slice();
    if (i > 0) all.unshift(...all.splice(i, 1));
    return all.map((e) => e.indexer);
  }

  private good(ix: Indexer): void {
    const url = this.entries.find((e) => e.indexer === ix)?.url;
    if (!url) return;
    try {
      if (this.storage?.getItem(INDEXER_KEY) !== url) this.storage?.setItem(INDEXER_KEY, url);
    } catch {
      // A browser that refuses storage asks in the configured order every time.
    }
  }

  /** `op` on each indexer in order until one passes; an error outside FALLBACK is thrown at once. */
  async run<T>(op: (ix: Indexer) => Promise<T>): Promise<T> {
    const errors: PageError[] = [];
    for (const ix of this.order()) {
      try {
        const v = await op(ix);
        this.good(ix);
        return v;
      } catch (e) {
        if (!(e instanceof PageError) || !FALLBACK.has(e.code)) throw e;
        errors.push(e);
      }
    }
    // A refused answer says more than an unreachable one.
    throw errors.find((e) => e.code !== 'indexer_unavailable') ?? errors[0];
  }

  status: Indexer['status'] = () => this.run((ix) => ix.status());
  instanceText: Indexer['instanceText'] = () => this.run((ix) => ix.instanceText());
  rounds: Indexer['rounds'] = () => this.run((ix) => ix.rounds());
  round: Indexer['round'] = (r) => this.run((ix) => ix.round(r));
  purchases: Indexer['purchases'] = (r) => this.run((ix) => ix.purchases(r));
  pools: Indexer['pools'] = () => this.run((ix) => ix.pools());
  tickets: Indexer['tickets'] = (p) => this.run((ix) => ix.tickets(p));
  vault: Indexer['vault'] = () => this.run((ix) => ix.vault());
  positions: Indexer['positions'] = (o) => this.run((ix) => ix.positions(o));
  requests: Indexer['requests'] = (o) => this.run((ix) => ix.requests(o));
  anchorProof: Indexer['anchorProof'] = (r) => this.run((ix) => ix.anchorProof(r));
}

/**
 * Run a read-and-check operation on one indexer, falling back to the next on a code of FALLBACK. Inside `op`,
 * `ctx.indexer` is that one indexer (an operation nested in another keeps the outer one's). A context without a
 * set (unit tests' stand-ins) runs `op` as it is.
 */
export function withIndexer<C extends Ctx, T>(ctx: C, op: (ctx: C) => Promise<T>): Promise<T> {
  const set = ctx.indexer;
  if (!(set instanceof IndexerSet)) return op(ctx);
  return set.run((ix) => op({ ...ctx, indexer: ix }));
}
