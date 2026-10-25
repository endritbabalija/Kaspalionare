// One wallet request at a time (KasWare answers a second request while one is open with `4001 please request
// after current approval resolve`): requests queue and run in order. A request that never settles (a wallet
// popup left open, an extension that hangs) is given up after `timeoutMs` with wallet_timeout, and the queue
// moves on (F2/F3 review note), so the page never waits forever.
import { PageError } from '@/lib/errors';

/** How long one wallet request may take: the player reads and approves it in the wallet. */
export const WALLET_TIMEOUT_MS = 300_000;

export class Serial {
  private tail: Promise<unknown> = Promise.resolve();

  constructor(private readonly timeoutMs: number = WALLET_TIMEOUT_MS) {}

  run<T>(f: () => Promise<T>): Promise<T> {
    const timed = () =>
      new Promise<T>((resolve, reject) => {
        const t = setTimeout(
          () => reject(new PageError('wallet_timeout', `no answer after ${this.timeoutMs / 1000} s`)),
          this.timeoutMs,
        );
        f().then(
          (v) => {
            clearTimeout(t);
            resolve(v);
          },
          (e: unknown) => {
            clearTimeout(t);
            reject(e instanceof Error ? e : new Error(String(e)));
          },
        );
      });
    const p = this.tail.then(timed, timed);
    this.tail = p.catch(() => undefined);
    return p;
  }
}
