// F5 review S6: the caller panel's build belongs to one (round, transition, wallet, live objects). When the chain
// moves the round's objects, or another transition is due, the old build's figures are dropped at once (never
// shown, never signed) and the due one is rebuilt; an old build answering late is ignored. The round loads and the
// builds are stand-ins (vi.mock); the hook is the page's.
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Due, PreparedCaller } from '@/rounds/caller';
import type { DetailData } from '@/rounds/load';
import type { WalletSession } from '@/wallet/context';
import type { WalletAdapter } from '@/wallet/types';
import type { Ctx } from '@/net/round';

const state = vi.hoisted(() => ({
  data: [] as unknown[],
  due: new Map<unknown, unknown>(),
  builds: [] as { data: unknown; resolve: (p: unknown) => void }[],
}));

vi.mock('@/rounds/load', async (orig) => ({
  ...(await orig<object>()),
  loadDetail: vi.fn(() => Promise.resolve(state.data[0])),
}));
vi.mock('@/rounds/caller', async (orig) => ({
  ...(await orig<object>()),
  dueOf: vi.fn((d: unknown) => state.due.get(d)),
  prepareCaller: vi.fn(
    (_env: unknown, data: unknown) =>
      new Promise((resolve) => {
        state.builds.push({ data, resolve });
      }),
  ),
}));

import { DETAIL_REFRESH_MS, useRoundDetail } from './useRoundDetail';

function detail(objects: string[]): DetailData {
  return {
    checked: { objects: objects.map((t) => ({ object: { utxo: { transactionId: t, index: 0 } } })) },
  } as unknown as DetailData;
}

function prep(tag: string): PreparedCaller {
  return { action: 'draw', round: '2', built: { id: tag }, figures: { reward: tag } } as unknown as PreparedCaller;
}

const wallet = { address: 'kaspatest:w', adapter: {} as WalletAdapter, kind: 'quick' } as unknown as WalletSession;
const ctx = {} as Ctx;
const draw: Due = { kind: 'due', action: 'draw', from: '1' };

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  state.data = [];
  state.due.clear();
  state.builds = [];
});
afterEach(() => vi.useRealTimers());

async function refresh(next: DetailData, due: Due) {
  state.data = [next];
  state.due.set(next, due);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(DETAIL_REFRESH_MS);
  });
}

describe('useRoundDetail: a build is shown only for the state it was made for (F5 review S6)', () => {
  it('drops the build when the chain moves the objects or the transition changes; ignores a late old build', async () => {
    const a = detail(['aa']);
    state.data = [a];
    state.due.set(a, draw);
    const { result } = renderHook(() => useRoundDetail(ctx, wallet, '2'));
    await waitFor(() => expect(state.builds).toHaveLength(1));
    await act(async () => state.builds[0]?.resolve(prep('A')));
    expect(result.current.caller.prep?.built.id).toBe('A');

    // The chain moves the round's objects: the old build is gone at once, a new one starts.
    const b = detail(['bb']);
    await refresh(b, draw);
    await waitFor(() => expect(state.builds).toHaveLength(2));
    expect(result.current.caller.prep).toBeUndefined();
    expect(result.current.caller.estimating).toBe(true);

    // Another state (the draw no longer due, e.g. the round was drawn elsewhere): nothing is shown, and the
    // pending build answering late is ignored.
    const c = detail(['bb']);
    await refresh(c, { kind: 'later', action: 'draw', from: '9' });
    await act(async () => state.builds[1]?.resolve(prep('B')));
    expect(result.current.caller.prep).toBeUndefined();
    expect(result.current.caller.estimating).toBe(false);

    // Due again with the same objects: rebuilt, and that build is shown.
    const e = detail(['bb']);
    await refresh(e, draw);
    await waitFor(() => expect(state.builds).toHaveLength(3));
    await act(async () => state.builds[2]?.resolve(prep('C')));
    expect(result.current.caller.prep?.built.id).toBe('C');
  });
});
