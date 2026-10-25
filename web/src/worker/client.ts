// The page's handle on the lottery worker (spec 6.1): typed calls (worker/protocol.ts), size caps on what
// goes in, and a restart when the worker dies (wasm aborts on a panic, so a trapped instance is never
// reused). After a restart the session is set up again (`boot`: start the network, connect the node)
// before the next call runs; calls in flight when it died reject with worker_stopped.
import { PageError } from '@/lib/errors';
import type { CallArgs, CallName, CallResult, Request, Response } from './protocol';

/** The most a call may carry into the worker (the largest Rust-side cap is 8 MiB of Safe JSON). */
export const ARGS_MAX_BYTES = 16 << 20;
/** Restarts allowed in RESTART_WINDOW_MS before the client gives up (a worker that dies while booting). */
export const RESTARTS_MAX = 3;
export const RESTART_WINDOW_MS = 60_000;

/** What the client needs of a Worker (tests pass a fake). */
export interface WorkerLike {
  postMessage(m: Request): void;
  terminate(): void;
  onmessage: ((e: MessageEvent<Response>) => void) | null;
  onerror: ((e: ErrorEvent) => void) | null;
  onmessageerror?: ((e: MessageEvent) => void) | null;
}

/** A raw call on the current worker (used by `boot`, which runs before ordinary calls). */
export type RawCall = <K extends CallName>(name: K, ...args: CallArgs<K>) => Promise<CallResult<K>>;

/** The size a call's arguments would have as JSON (the cap's measure). */
export function argsSize(args: readonly unknown[]): number {
  let n = 0;
  for (const a of args) {
    if (typeof a === 'string') n += a.length;
    else if (a !== undefined) n += JSON.stringify(a)?.length ?? 0;
    if (n > ARGS_MAX_BYTES) return n;
  }
  return n;
}

interface Pending {
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
}

export class LotteryClient {
  private worker: WorkerLike | null = null;
  private pending = new Map<number, Pending>();
  private nextId = 1;
  private booted: Promise<void> | null = null;
  private restarts: number[] = [];
  private failed: string | null = null;
  /** How many times the worker has been replaced (shown by the page as a status). */
  restartCount = 0;
  onRestart: ((count: number, reason: string) => void) | null = null;

  constructor(
    private readonly factory: () => WorkerLike,
    private readonly boot: (raw: RawCall) => Promise<void>,
    private readonly now: () => number = () => Date.now(),
  ) {}

  /** Call an export; resolves to its result (an Outcome for most), rejects with a PageError. */
  async call<K extends CallName>(name: K, ...args: CallArgs<K>): Promise<CallResult<K>> {
    await this.ensureBooted();
    return this.raw(name, ...args);
  }

  /**
   * After the client gave up (more than RESTARTS_MAX deaths in a minute), let a user action (Retry) try
   * again without a reload (F2/F3 review note): the count starts over and the next call boots a new worker.
   * Does nothing after `dispose`.
   */
  recover(): void {
    if (this.failed === 'disposed') return;
    this.failed = null;
    this.restarts = [];
  }

  /** Whether the client has given up until `recover`. */
  get stopped(): boolean {
    return this.failed !== null;
  }

  /** Stop the worker for good (page unload). */
  dispose(): void {
    this.kill('disposed');
    this.failed = 'disposed';
  }

  private ensureBooted(): Promise<void> {
    if (this.failed) return Promise.reject(new PageError('worker_stopped', this.failed));
    if (!this.booted) {
      const raw: RawCall = (name, ...args) => this.raw(name, ...args);
      this.booted = this.boot(raw).catch((e) => {
        this.booted = null;
        throw e;
      });
    }
    return this.booted;
  }

  private raw<K extends CallName>(name: K, ...args: CallArgs<K>): Promise<CallResult<K>> {
    if (this.failed) return Promise.reject(new PageError('worker_stopped', this.failed));
    const size = argsSize(args);
    if (size > ARGS_MAX_BYTES) {
      return Promise.reject(
        new PageError('bytes_length', `${name}: ${size} bytes of arguments, the cap is ${ARGS_MAX_BYTES}`),
      );
    }
    const w = this.ensureWorker();
    const id = this.nextId++;
    return new Promise<CallResult<K>>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      try {
        w.postMessage({ id, name, args });
      } catch (e) {
        this.pending.delete(id);
        reject(new PageError('worker_stopped', `postMessage: ${String(e)}`));
      }
    });
  }

  private ensureWorker(): WorkerLike {
    if (this.worker) return this.worker;
    const w = this.factory();
    w.onmessage = (e) => this.onMessage(w, e.data);
    w.onerror = (e) => {
      e.preventDefault?.();
      this.died(w, `worker error: ${e.message || 'unknown'}`);
    };
    w.onmessageerror = () => this.died(w, 'message could not be read');
    this.worker = w;
    return w;
  }

  private onMessage(w: WorkerLike, r: Response): void {
    if (w !== this.worker) return;
    const p = this.pending.get(r.id);
    if (!p) return;
    this.pending.delete(r.id);
    if (r.ok) p.resolve(r.value);
    else {
      p.reject(new PageError('worker_stopped', r.crash));
      this.died(w, r.crash);
    }
  }

  private kill(reason: string): void {
    const w = this.worker;
    this.worker = null;
    this.booted = null;
    if (w) {
      w.onmessage = null;
      w.onerror = null;
      try {
        w.terminate();
      } catch {
        // already gone
      }
    }
    const pending = [...this.pending.values()];
    this.pending.clear();
    for (const p of pending) p.reject(new PageError('worker_stopped', reason));
  }

  /** The worker died (a trap or an error event): drop it; the next call starts and boots a new one. */
  private died(w: WorkerLike, reason: string): void {
    if (w !== this.worker) return;
    this.kill(reason);
    const t = this.now();
    this.restarts = this.restarts.filter((x) => t - x < RESTART_WINDOW_MS);
    this.restarts.push(t);
    this.restartCount++;
    if (this.restarts.length > RESTARTS_MAX) {
      this.failed = `the worker stopped ${this.restarts.length} times in a minute: ${reason}`;
    }
    this.onRestart?.(this.restartCount, reason);
  }
}
