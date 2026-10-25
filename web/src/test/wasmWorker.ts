// TEST-ONLY: the real wasm (src/wasm/dev, the dev package the page loads) behind the page's own message path,
// in-process: a WorkerLike that runs each request through worker/handler.ts exactly as lottery.worker.ts does,
// so tests call the wasm through LotteryClient (size caps, typed calls, crash handling) and not a
// reimplementation. No node: the session boots with `errorTable` and `start(network)` only.
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import * as wasm from '@wasm/web.js';
import { setErrorTable, unwrap } from '@/lib/errors';
import { LotteryClient, type WorkerLike } from '@/worker/client';
import { handle } from '@/worker/handler';
import type { Request, Response } from '@/worker/protocol';

let loaded: Promise<void> | null = null;

function load(): Promise<void> {
  loaded ??= Promise.resolve().then(() => {
    const bytes = readFileSync(fileURLToPath(new URL('../wasm/dev/web_bg.wasm', import.meta.url)));
    wasm.initSync({ module: bytes });
  });
  return loaded;
}

class InProcessWorker implements WorkerLike {
  onmessage: ((e: MessageEvent<Response>) => void) | null = null;
  onerror: ((e: ErrorEvent) => void) | null = null;

  postMessage(m: Request): void {
    void handle(wasm as unknown as Record<string, unknown>, load(), structuredClone(m)).then((r) =>
      this.onmessage?.({ data: structuredClone(r) } as MessageEvent<Response>),
    );
  }

  terminate(): void {}
}

/** A LotteryClient on the real wasm, started on `network`. */
export function wasmClient(network: 'testnet-10' | 'simnet' = 'testnet-10'): LotteryClient {
  return new LotteryClient(
    () => new InProcessWorker(),
    async (raw) => {
      setErrorTable(await raw('errorTable'));
      unwrap(await raw('start', network));
    },
  );
}
