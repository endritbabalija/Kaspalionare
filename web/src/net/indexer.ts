// The indexer API client (spec 6.3, F2). Every response is read as text under its kind's cap (the caps
// come from the wasm: indexerLimits) and parsed by the worker's parseIndexer* call; nothing it says is
// used before the worker has checked it against the node (net/round.ts). Unreachable, or an error status,
// is indexer_unavailable (the API's own error body goes into the detail).
import type {
  ApiAnchorProof,
  ApiObjects,
  ApiPurchases,
  ApiRound,
  ApiRounds,
  ApiStatus,
  ApiTickets,
  Limits,
} from '@wasm/web.js';
import { PageError, unwrap } from '@/lib/errors';
import type { LotteryClient } from '@/worker/client';
import { fetchText } from './fetchText';

/** A round number as the page asks for it: a decimal without sign or leading zeros. */
const ROUND = /^(0|[1-9][0-9]*)$/;

/**
 * F5 review S2: an answer that carries a round must be for the round asked for (an indexer could
 * otherwise serve another round's chain-checked objects under this number).
 */
function sameRound(what: string, asked: string, answered: string): void {
  if (!ROUND.test(asked) || !ROUND.test(answered) || BigInt(asked) !== BigInt(answered)) {
    throw new PageError('chain_mismatch', `${what}: asked for round ${asked}, the indexer answered round ${answered}`);
  }
}

export class Indexer {
  private limits: Limits | null = null;

  constructor(
    readonly base: string,
    private readonly lottery: LotteryClient,
    private readonly fetchImpl?: typeof fetch,
    /** One response's deadline (fetchText's default unless set; F6 part E tests). */
    private readonly timeoutMs?: number,
  ) {}

  private async caps(): Promise<Limits> {
    this.limits ??= await this.lottery.call('indexerLimits');
    return this.limits;
  }

  /** GET a path; a 2xx body, or null for a 404 when `missingOk`. */
  private async get(path: string, cap: number, missingOk = false): Promise<string | null> {
    const url = new URL(path, this.base).toString();
    const r = await fetchText(url, cap, { fetch: this.fetchImpl, timeoutMs: this.timeoutMs });
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
    const a = unwrap(
      await this.lottery.call('parseIndexerRound', (await this.get(`rounds/${encodeURIComponent(r)}`, c.round)) ?? ''),
    );
    sameRound('rounds', r, a.round);
    return a;
  }

  async purchases(r: string): Promise<ApiPurchases> {
    const c = await this.caps();
    const p = unwrap(
      await this.lottery.call(
        'parseIndexerPurchases',
        (await this.get(`rounds/${encodeURIComponent(r)}/purchases`, c.purchases)) ?? '',
      ),
    );
    sameRound('purchases', r, p.round);
    return p;
  }

  /**
   * The instance's live prize pools, oldest claim deadline first (F5 review S1). Only where to look: a draw reads
   * each from the node by its outpoint and keeps those `absorbablePools` accepts.
   */
  async pools(): Promise<ApiObjects> {
    const c = await this.caps();
    return unwrap(await this.lottery.call('parseIndexerObjects', (await this.get('pools', c.objects)) ?? ''));
  }

  /** The live tickets paying out to `payout`, and its purchase history (J3; checked against the node by the caller). */
  async tickets(payout: string): Promise<ApiTickets> {
    const c = await this.caps();
    return unwrap(
      await this.lottery.call(
        'parseIndexerTickets',
        (await this.get(`tickets?payout=${encodeURIComponent(payout)}`, c.objects)) ?? '',
      ),
    );
  }

  /** The vault, the newest controller, the drawer and the live settlement records (S5; checked by the caller). */
  async vault(): Promise<ApiObjects> {
    const c = await this.caps();
    return unwrap(await this.lottery.call('parseIndexerObjects', (await this.get('vault', c.objects)) ?? ''));
  }

  /** The share positions of `owner` (S5; each checked against the node and its owner by the caller). */
  async positions(owner: string): Promise<ApiObjects> {
    return this.owned('positions', owner);
  }

  /** The vault requests of `owner` and the live records of their rounds (S5; checked by the caller). */
  async requests(owner: string): Promise<ApiObjects> {
    return this.owned('requests', owner);
  }

  private async owned(what: 'positions' | 'requests', owner: string): Promise<ApiObjects> {
    const c = await this.caps();
    const o = unwrap(
      await this.lottery.call(
        'parseIndexerObjects',
        (await this.get(`${what}?owner=${encodeURIComponent(owner)}`, c.objects)) ?? '',
      ),
    );
    if (o.owner !== owner) throw new PageError('chain_mismatch', `${what}: asked for ${owner}, answered ${o.owner}`);
    return o;
  }

  /** The round's anchor proof (the spec 6.1 fallback), or null when the API has none. */
  async anchorProof(r: string): Promise<ApiAnchorProof | null> {
    const c = await this.caps();
    const t = await this.get(`rounds/${encodeURIComponent(r)}/anchor-proof`, c.small, true);
    if (t === null) return null;
    const p = unwrap(await this.lottery.call('parseIndexerAnchorProof', t));
    sameRound('anchor proof', r, p.round);
    return p;
  }
}
