// One wallet request at a time (KasWare answers a second request while one is open with `4001 please request
// after current approval resolve`): requests queue and run in order.
export class Serial {
  private tail: Promise<unknown> = Promise.resolve();

  run<T>(f: () => Promise<T>): Promise<T> {
    const p = this.tail.then(f, f);
    this.tail = p.catch(() => undefined);
    return p;
  }
}
