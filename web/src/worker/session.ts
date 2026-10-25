// The page's one lottery session: the worker, started on the configured network and connected to its
// node. `boot` runs again after any worker restart, so the session survives a dead worker.
import type { Config } from '@/config';
import { setErrorTable, unwrap } from '@/lib/errors';
import { LotteryClient, type RawCall, type WorkerLike } from './client';

export function bootSession(config: Pick<Config, 'network' | 'nodeUrl'>) {
  return async (raw: RawCall): Promise<void> => {
    setErrorTable(await raw('errorTable'));
    unwrap(await raw('start', config.network));
    unwrap(await raw('nodeConnect', config.nodeUrl ?? null));
  };
}

export function createSession(config: Config): LotteryClient {
  const factory = (): WorkerLike =>
    new Worker(new URL('./lottery.worker.ts', import.meta.url), {
      type: 'module',
      name: 'lottery',
    }) as unknown as WorkerLike;
  return new LotteryClient(factory, bootSession(config));
}
