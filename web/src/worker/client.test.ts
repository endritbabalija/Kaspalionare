// The worker restart and the size cap (spec 6.1: wasm aborts on a panic, the page restarts the worker; F1
// part C open question 4: the page caps what it feeds the worker).
import { describe, expect, it } from 'vitest';
import { ARGS_MAX_BYTES, LotteryClient, RESTARTS_MAX, type WorkerLike } from './client';
import type { Request, Response } from './protocol';

type Behaviour = (r: Request) => Response | 'crash-event' | 'silent';

class FakeWorker implements WorkerLike {
  onmessage: ((e: MessageEvent<Response>) => void) | null = null;
  onerror: ((e: ErrorEvent) => void) | null = null;
  onmessageerror: ((e: MessageEvent) => void) | null = null;
  terminated = false;
  seen: Request[] = [];
  constructor(private readonly behave: Behaviour) {}
  postMessage(m: Request): void {
    this.seen.push(m);
    const r = this.behave(m);
    queueMicrotask(() => {
      if (this.terminated) return;
      if (r === 'silent') return;
      if (r === 'crash-event')
        this.onerror?.({ message: 'RuntimeError: unreachable', preventDefault() {} } as ErrorEvent);
      else this.onmessage?.({ data: r } as MessageEvent<Response>);
    });
  }
  terminate(): void {
    this.terminated = true;
  }
}

const ok = (m: Request, value: unknown): Response => ({ id: m.id, ok: true, value });

function at<T>(xs: T[], i: number): T {
  const x = xs[i];
  if (x === undefined) throw new Error(`no item ${i}`);
  return x;
}

function setup(perWorker: Behaviour[]) {
  const workers: FakeWorker[] = [];
  let boots = 0;
  const client = new LotteryClient(
    () => {
      const w = new FakeWorker(perWorker[workers.length] ?? at(perWorker, perWorker.length - 1));
      workers.push(w);
      return w;
    },
    async (raw) => {
      boots++;
      await raw('start', 'simnet');
    },
  );
  return { client, workers, boots: () => boots };
}

describe('LotteryClient', () => {
  it('boots once, then passes calls through', async () => {
    const { client, workers, boots } = setup([(m) => ok(m, m.name === 'jackpotOdds' ? { ok: '1425060' } : { ok: {} })]);
    expect(await client.call('jackpotOdds', 10)).toEqual({ ok: '1425060' });
    expect(await client.call('jackpotOdds', 10)).toEqual({ ok: '1425060' });
    expect(boots()).toBe(1);
    expect(at(workers, 0).seen.map((r) => r.name)).toEqual(['start', 'jackpotOdds', 'jackpotOdds']);
  });

  it('replaces a crashed worker: the call in flight fails with worker_stopped, the next runs on a booted new worker', async () => {
    const crashing: Behaviour = (m) =>
      m.name === 'buildBuy' ? { id: m.id, ok: false, crash: 'RuntimeError: unreachable' } : ok(m, { ok: {} });
    const { client, workers, boots } = setup([crashing, (m) => ok(m, { ok: 'fresh' })]);
    let restarted = 0;
    client.onRestart = (n) => (restarted = n);
    await expect(client.call('buildBuy', {} as never)).rejects.toMatchObject({ code: 'worker_stopped' });
    expect(at(workers, 0).terminated).toBe(true);
    expect(restarted).toBe(1);
    expect(await client.call('jackpotOdds', 2)).toEqual({ ok: 'fresh' });
    expect(workers).toHaveLength(2);
    expect(boots()).toBe(2);
    expect(at(workers, 1).seen.map((r) => r.name)).toEqual(['start', 'jackpotOdds']);
  });

  it('treats a worker error event as a crash and rejects every pending call', async () => {
    let n = 0;
    const { client, workers } = setup([
      (m) => (m.name === 'start' ? ok(m, { ok: {} }) : n++ === 0 ? 'silent' : 'crash-event'),
      (m) => ok(m, { ok: 1 }),
    ]);
    const a = client.call('nodeInfo');
    await Promise.resolve();
    const b = client.call('nodeFeeRate');
    await expect(a).rejects.toMatchObject({ code: 'worker_stopped' });
    await expect(b).rejects.toMatchObject({ code: 'worker_stopped' });
    expect(at(workers, 0).terminated).toBe(true);
    expect(await client.call('nodeFeeRate')).toEqual({ ok: 1 });
  });

  it('gives up after repeated deaths instead of looping', async () => {
    const { client } = setup([(m) => ({ id: m.id, ok: false, crash: 'boom' })]);
    for (let i = 0; i <= RESTARTS_MAX; i++)
      await expect(client.call('nodeInfo')).rejects.toMatchObject({ code: 'worker_stopped' });
    await expect(client.call('nodeInfo')).rejects.toMatchObject({
      code: 'worker_stopped',
      detail: expect.stringMatching(/times in a minute/),
    });
  });

  it('after giving up, a user action (recover) lets the next call boot a new worker (no reload)', async () => {
    let dead = true;
    const { client } = setup([(m) => (dead ? { id: m.id, ok: false, crash: 'boom' } : ok(m, { ok: {} }))]);
    for (let i = 0; i <= RESTARTS_MAX; i++)
      await expect(client.call('nodeInfo')).rejects.toMatchObject({ code: 'worker_stopped' });
    expect(client.stopped).toBe(true);
    dead = false;
    await expect(client.call('nodeInfo')).rejects.toMatchObject({ code: 'worker_stopped' });
    client.recover();
    expect(client.stopped).toBe(false);
    expect(await client.call('nodeInfo')).toEqual({ ok: {} });
  });

  it('refuses oversized arguments before posting them', async () => {
    const { client, workers } = setup([(m) => ok(m, { ok: {} })]);
    await expect(client.call('parseIndexerRound', 'x'.repeat(ARGS_MAX_BYTES + 1))).rejects.toMatchObject({
      code: 'bytes_length',
    });
    expect(at(workers, 0).seen.map((r) => r.name)).toEqual(['start']);
  });
});
