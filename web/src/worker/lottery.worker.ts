// The Web Worker that runs crates/web (spec 6.1): the lottery code and the node client, off the page's
// thread. One wasm instance per worker; a thrown exception means the wasm trapped (it aborts on a panic),
// so the worker reports a crash and the page replaces the worker (worker/client.ts).
import init, * as wasm from '@wasm/web.js';
import { isCall, type Request, type Response } from './protocol';

// Panic messages go to the console (rusty-kaspa's hook, compiled into crates/web): a trap then says why.
const ready = init().then(() => wasm.initConsolePanicHook());
const scope = self as unknown as DedicatedWorkerGlobalScope;

scope.onmessage = async (e: MessageEvent<Request>) => {
  const { id, name, args } = e.data;
  let reply: Response;
  try {
    await ready;
    if (!isCall(name)) throw new Error(`not an export: ${String(name)}`);
    const fn = wasm[name] as unknown as (...a: unknown[]) => unknown;
    reply = { id, ok: true, value: await fn(...args) };
  } catch (err) {
    reply = { id, ok: false, crash: err instanceof Error ? `${err.name}: ${err.message}` : String(err) };
  }
  scope.postMessage(reply);
};
