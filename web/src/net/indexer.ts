// The indexer API client (spec 6.3, F2). Every response is read as text under its kind's cap (the caps
// come from the wasm: indexerLimits) and parsed by the worker's parseIndexer* call; nothing it says is
// used before the worker has checked it against the node (net/round.ts). Unreachable, or an error status,
// is indexer_unavailable (the API's own error body goes into the detail).
import type { ApiAnchorProof, ApiPurchases, ApiRound, ApiRounds, ApiStatus, Limits } from '@wasm/web.js';
import { PageError, unwrap } from '@/lib/errors';
import type { LotteryClient } from '@/worker/client';
import { fetchText } from './fetchText';

export class Indexer {
  private limits: Limits | null = null;

  constructor(
    readonly base: string,
    private readonly lottery: LotteryClient,
    private readonly fetchImpl?: typeof fetch,
  ) {}

  private async caps(): Promise<Limits> {
    this.limits ??= await this.lottery.call('indexerLimits');
    return this.limits;
  }

  /** GET a path; a 2xx body, or null for a 404 when `missingOk`. */
  private async get(path: string, cap: number, missingOk = false): Promise<string | null> {
    const url = new URL(path, this.base).toString();
    const r = await fetchText(url, cap, { fetch: this.fetchImpl });
    if (r.status >= 200 && r.status < 300) return r.text;
    if (r.status === 404 && missingOk) return null;
    const small = (await this.caps()).small;
    let detail = `${url}: HTTP ${r.status}`;
    if (r.text.length && r.text.length <= small) {
      const e = await this.lottery.call('parseIndexerError', r.text);
      if ('ok' in e) detail += ` ${e.ok.error.code}: ${e.ok.error.detail}`;
    }
    throw new PageError('indexer_unavailable', detail);
  }

  async status(): Promise<ApiStatus> {
    const c = await this.caps();
    return unwrap(await this.lottery.call('parseIndexerStatus', (await this.get('status', c.small)) ?? ''));
  }

  /** The launch bundle as text (checkIndexerInstance hashes these exact bytes). */
  async instanceText(): Promise<string> {
    const c = await this.caps();
    return (await this.get('instance', c.instance)) ?? '';
  }

  async rounds(): Promise<ApiRounds> {
    const c = await this.caps();
    return unwrap(await this.lottery.call('parseIndexerRounds', (await this.get('rounds', c.round)) ?? ''));
  }

  async round(r: string): Promise<ApiRound> {
    const c = await this.caps();
    return unwrap(
      await this.lottery.call('parseIndexerRound', (await this.get(`rounds/${encodeURIComponent(r)}`, c.round)) ?? ''),
    );
  }

  async purchases(r: string): Promise<ApiPurchases> {
    const c = await this.caps();
    return unwrap(
      await this.lottery.call(
        'parseIndexerPurchases',
        (await this.get(`rounds/${encodeURIComponent(r)}/purchases`, c.purchases)) ?? '',
      ),
    );
  }

  /** The round's anchor proof (the spec 6.1 fallback), or null when the API has none. */
  async anchorProof(r: string): Promise<ApiAnchorProof | null> {
    const c = await this.caps();
    const t = await this.get(`rounds/${encodeURIComponent(r)}/anchor-proof`, c.small, true);
    return t === null ? null : unwrap(await this.lottery.call('parseIndexerAnchorProof', t));
  }
}
