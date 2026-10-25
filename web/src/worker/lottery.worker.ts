// The Web Worker that runs crates/web (spec 6.1): the lottery code and the node client, off the page's
// thread. One wasm instance per worker; a thrown exception means the wasm trapped (it aborts on a panic),
// so the worker reports a crash and the page replaces the worker (worker/client.ts).
import init, * as wasm from '@wasm/web.js';
import { handle } from './handler';
import type { Request } from './protocol';

// Panic messages go to the console (rusty-kaspa's hook, compiled into crates/web): a trap then says why.
const ready = init().then(() => wasm.initConsolePanicHook());
const scope = self as unknown as DedicatedWorkerGlobalScope;

scope.onmessage = async (e: MessageEvent<Request>) => {
  scope.postMessage(await handle(wasm as unknown as Record<string, unknown>, ready, e.data));
};
