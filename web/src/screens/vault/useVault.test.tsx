// F6 part A (spec S5, 7: what is signed is what is shown): a vault action's build belongs to one key (the action,
// its amount, the wallet and the live vault). A new amount or a vault the chain moved drops the old build at once
// (never shown, never signed), and an old build answering late is ignored; the press sends exactly the build shown;
// a press whose outcome is unknown keeps the press disabled while the same vault is live. The loads, builds and
// sends are stand-ins (vi.mock); the hook is the page's.
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PageError } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import type { VaultData } from '@/vault/load';
import type { Prepared } from '@/vault/flow';
import type { WalletSession } from '@/wallet/context';
import type { WalletAdapter } from '@/wallet/types';

const state = vi.hoisted(() => ({
  data: [] as unknown[],
  builds: [] as { amount: unknown; vault: string; resolve: (p: unknown) => void }[],
  sent: [] as unknown[],
  send: null as null | (() => Promise<string>),
}));

vi.mock('@/vault/load', async (orig) => ({
  ...(await orig<object>()),
  loadVault: vi.fn(() => Promise.resolve(state.data[0])),
}));
vi.mock('@/vault/flow', async (orig) => ({
  ...(await orig<object>()),
  prepareDeposit: vi.fn(
    (_env: unknown, d: { vault: { utxo: { transactionId: string } } }, amount: unknown) =>
      new Promise((resolve) => {
        state.builds.push({ amount, vault: d.vault.utxo.transactionId, resolve });
      }),
  ),
  sendVault: vi.fn((_env: unknown, p: unknown) => {
    state.sent.push(p);
    return state.send ? state.send() : Promise.resolve('id');
  }),
}));

import { VAULT_REFRESH_MS, useVault } from './useVault';

const ADDR = 'kaspatest:w';

function vault(tx: string): VaultData {
  return {
    page: { vault: { requestOpen: true }, positions: [], requests: [], foreign: 0 },
    vault: { utxo: { transactionId: tx, index: 0 } },
    owner: ADDR,
    clock: { virtualDaa: '1', observedAt: 0 },
  } as unknown as VaultData;
}

function prep(tag: string): Prepared {
  return { kind: 'deposit', built: { id: tag }, figures: { total: tag } } as unknown as Prepared;
}

const wallet = { address: ADDR, adapter: {} as WalletAdapter, kind: 'quick' } as unknown as WalletSession;
const ctx = {} as Ctx;

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  state.data = [vault('aa')];
  state.builds = [];
  state.sent = [];
  state.send = null;
});
afterEach(() => vi.useRealTimers());

async function settle() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(400);
  });
}

describe('useVault: a build is shown and signed only for the inputs it was made for', () => {
  it('a new amount drops the old build (its late answer ignored); the press sends the build shown', async () => {
    const { result } = renderHook(() => useVault(ctx, wallet));
    await waitFor(() => expect(result.current.data).toBeDefined());
    act(() => result.current.deposit.onAmount('10'));
    await settle();
    await waitFor(() => expect(state.builds).toHaveLength(1));
    expect(state.builds[0]?.amount).toBe('1000000000');
    act(() => result.current.deposit.onAmount('20'));
    await settle();
    await waitFor(() => expect(state.builds).toHaveLength(2));
    // The first build answers late: never shown.
    await act(async () => state.builds[0]?.resolve(prep('A')));
    expect(result.current.deposit.view.figures).toBeUndefined();
    await act(async () => state.builds[1]?.resolve(prep('B')));
    expect(result.current.deposit.view.figures).toEqual({ total: 'B' });
    await act(async () => result.current.deposit.onPress());
    expect(state.sent).toEqual([prep('B')]);
  });

  it('the vault moved by the chain: the shown build is dropped at once and rebuilt on the new vault', async () => {
    const { result } = renderHook(() => useVault(ctx, wallet));
    await waitFor(() => expect(result.current.data).toBeDefined());
    act(() => result.current.deposit.onAmount('10'));
    await settle();
    await waitFor(() => expect(state.builds).toHaveLength(1));
    await act(async () => state.builds[0]?.resolve(prep('A')));
    expect(result.current.deposit.view.figures).toEqual({ total: 'A' });
    state.data = [vault('bb')];
    await act(async () => {
      await vi.advanceTimersByTimeAsync(VAULT_REFRESH_MS);
    });
    expect(result.current.deposit.view.figures).toBeUndefined();
    await settle();
    await waitFor(() => expect(state.builds).toHaveLength(2));
    expect(state.builds[1]?.vault).toBe('bb');
    // A press now has nothing to sign until the new build is shown.
    await act(async () => result.current.deposit.onPress());
    expect(state.sent).toEqual([]);
  });

  it('an unknown outcome keeps the press disabled while the same vault is live', async () => {
    state.send = () => Promise.reject(new PageError('submit_unknown', 'test'));
    const { result } = renderHook(() => useVault(ctx, wallet));
    await waitFor(() => expect(result.current.data).toBeDefined());
    act(() => result.current.deposit.onAmount('10'));
    await settle();
    await waitFor(() => expect(state.builds).toHaveLength(1));
    await act(async () => state.builds[0]?.resolve(prep('A')));
    await act(async () => result.current.deposit.onPress());
    expect(state.sent).toHaveLength(1);
    expect(result.current.last?.tx.failed).toBeTruthy();
    // Rebuilt for the same key: blocked.
    act(() => result.current.deposit.onAmount('10'));
    await settle();
    await waitFor(() => expect(state.builds).toHaveLength(2));
    await act(async () => state.builds[1]?.resolve(prep('A2')));
    expect(result.current.deposit.view.blocked).toBe(true);
    await act(async () => result.current.deposit.onPress());
    expect(state.sent).toHaveLength(1);
  });
});
