import type { Config } from '@/config';
import { IndexerSet } from '@/net/indexers';
import type { Ctx } from '@/net/round';
import { createSession } from '@/worker/session';

/** The page's one session (created once, outside React, so StrictMode's double render makes no second worker). */
export function createCtx(config: Config): Ctx {
  const lottery = createSession(config);
  return { config, lottery, indexer: new IndexerSet(config.indexerUrls, lottery) };
}
