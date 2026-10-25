// One request to the wasm, as the worker runs it (spec 6.1): the call name must be an export the page may
// call (protocol.ts CALLS); a thrown exception means the wasm trapped (it aborts on a panic), reported as a
// crash so the client replaces the worker. Shared by lottery.worker.ts and the in-process test worker
// (src/test/wasmWorker.ts), so the post-sign attack tests run the page's own message path.
import { isCall, type Request, type Response } from './protocol';

export type WasmModule = Record<string, unknown>;

export async function handle(wasm: WasmModule, ready: Promise<unknown>, req: Request): Promise<Response> {
  const { id, name, args } = req;
  try {
    await ready;
    if (!isCall(name)) throw new Error(`not an export: ${String(name)}`);
    const fn = wasm[name] as (...a: unknown[]) => unknown;
    return { id, ok: true, value: await fn(...args) };
  } catch (err) {
    return { id, ok: false, crash: err instanceof Error ? `${err.name}: ${err.message}` : String(err) };
  }
}
