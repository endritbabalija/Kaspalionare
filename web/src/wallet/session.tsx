// The page's wallet session (spec 6.2, S7): which wallet is connected, its address and its state
// (disconnected, connecting, wrong network, connected, quick wallet). Every address a wallet reports is
// checked by the wasm as an address of the page's network (`scriptOf`) before use; the coins always come
// from the node. The quick wallet's code is loaded only when this build has it (wallet/quickEnabled.ts).
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { QuickKey } from '@wasm/web.js';
import { describe, PageError, unwrap } from '@/lib/errors';
import type { Ctx } from '@/net/round';
import { kaspireApi, KaspireAdapter } from './kaspire';
import type * as QuickMod from './quick';
import { kaswareApi, KasWareAdapter } from './kasware';
import { QUICK_WALLET } from './quickEnabled';
import { WalletContext, type WalletSession, type WalletState } from './context';
import { walletNetworkFor, type WalletAdapter, type WalletKind } from './types';

/** The wallet kind this browser used last (only the quick wallet reconnects by itself). */
const WALLET_KEY = 'kl.wallet';

function remember(kind: WalletKind | null): void {
  try {
    if (kind) localStorage.setItem(WALLET_KEY, kind);
    else localStorage.removeItem(WALLET_KEY);
  } catch {
    // storage refused: nothing is remembered
  }
}

function remembered(): string | null {
  try {
    return localStorage.getItem(WALLET_KEY);
  } catch {
    return null;
  }
}

type QuickModule = typeof QuickMod;

export function WalletProvider({ ctx, children }: { ctx: Ctx; children: ReactNode }) {
  const [s, setS] = useState<Pick<WalletSession, 'state' | 'kind' | 'address' | 'adapter' | 'error'>>({
    state: 'disconnected',
  });
  const [quickStored, setQuickStored] = useState(false);
  const quick = useRef<Promise<{ mod: QuickModule; store: InstanceType<QuickModule['QuickKeyStore']> }> | null>(null);
  const { lottery, config } = ctx;

  const coins = useCallback(async (address: string) => unwrap(await lottery.call('nodeCoins', address)), [lottery]);

  const quickParts = useCallback(() => {
    if (!QUICK_WALLET) return Promise.reject(new PageError('wallet_missing', 'this build has no quick wallet'));
    quick.current ??= (async () => {
      const mod = await import('./quick');
      const info = await lottery.call('webstoreInfo');
      return { mod, store: mod.QuickKeyStore.browser(info, config.network) };
    })();
    return quick.current;
  }, [lottery, config.network]);

  const fail = useCallback((e: unknown, state: WalletState = 'disconnected') => {
    const d = describe(e);
    if (d.detail) console.warn(`wallet: ${d.code ?? 'error'}: ${d.detail}`);
    setS({ state: d.code === 'wallet_network' ? 'wrong-network' : state, error: { code: d.code, message: d.message } });
  }, []);

  /** The address checked by the wasm as one of the page's network (else wallet_network). */
  const checked = useCallback(
    async (address: string) => {
      const r = await lottery.call('scriptOf', address);
      if ('error' in r) throw new PageError('wallet_network', `${address.slice(0, 16)}…: ${r.error.code}`);
      return address;
    },
    [lottery],
  );

  const connect = useCallback(
    async (kind: 'kasware' | 'kaspire') => {
      setS({ state: 'connecting', kind });
      try {
        const expected = walletNetworkFor(config.network, kind);
        let adapter: WalletAdapter;
        if (kind === 'kasware') {
          const api = kaswareApi();
          if (!api) throw new PageError('wallet_missing', 'window.kasware is not there');
          adapter = new KasWareAdapter(api, expected, coins);
        } else {
          const api = kaspireApi();
          if (!api) throw new PageError('wallet_missing', 'window.kaspire is not there');
          adapter = new KaspireAdapter(api, expected, coins);
        }
        const address = await checked(await adapter.connect());
        remember(kind);
        setS({ state: 'connected', kind, address, adapter });
      } catch (e) {
        fail(e);
      }
    },
    [config.network, coins, checked, fail],
  );

  const adoptKey = useCallback(
    async (k: QuickKey) => {
      const { mod, store } = await quickParts();
      const adapter = new mod.QuickWalletAdapter(lottery, store, config.network, coins);
      const address = await checked(k.address);
      remember('quick');
      setQuickStored(true);
      setS({ state: 'quick', kind: 'quick', address, adapter });
    },
    [quickParts, lottery, config.network, coins, checked],
  );

  const openQuick = useCallback(async () => {
    setS({ state: 'connecting', kind: 'quick' });
    try {
      const { store } = await quickParts();
      const k = await store.load();
      if (!k) throw new PageError('wallet_missing', 'no quick wallet key in this browser');
      await adoptKey(k);
    } catch (e) {
      fail(e);
    }
  }, [quickParts, adoptKey, fail]);

  const createQuick = useCallback(async () => {
    setS({ state: 'connecting', kind: 'quick' });
    try {
      const { mod, store } = await quickParts();
      await adoptKey(await mod.createQuickKey(lottery, store));
    } catch (e) {
      fail(e);
    }
  }, [quickParts, adoptKey, lottery, fail]);

  const importQuick = useCallback(
    async (secretKey: string) => {
      setS({ state: 'connecting', kind: 'quick' });
      try {
        const { mod, store } = await quickParts();
        await adoptKey(await mod.importQuickKey(lottery, store, secretKey));
      } catch (e) {
        fail(e);
      }
    },
    [quickParts, adoptKey, lottery, fail],
  );

  const exportQuick = useCallback(async () => {
    const { store } = await quickParts();
    const k = await store.load();
    if (!k) throw new PageError('wallet_missing', 'no quick wallet key in this browser');
    return k.secretKey;
  }, [quickParts]);

  const disconnect = useCallback(() => {
    remember(null);
    setS({ state: 'disconnected' });
  }, []);

  // The quick wallet reconnects by itself when this browser used it last and still holds its key.
  useEffect(() => {
    if (!QUICK_WALLET) return;
    let alive = true;
    void (async () => {
      try {
        const { store } = await quickParts();
        const k = await store.load();
        if (!alive) return;
        setQuickStored(!!k);
        if (k && remembered() === 'quick') await adoptKey(k);
      } catch (e) {
        if (alive) console.warn('quick wallet', describe(e));
      }
    })();
    return () => {
      alive = false;
    };
  }, [quickParts, adoptKey]);

  const value = useMemo<WalletSession>(
    () => ({
      ...s,
      quickAvailable: QUICK_WALLET,
      quickStored,
      connect,
      openQuick,
      createQuick,
      importQuick,
      exportQuick,
      disconnect,
    }),
    [s, quickStored, connect, openQuick, createQuick, importQuick, exportQuick, disconnect],
  );
  return <WalletContext.Provider value={value}>{children}</WalletContext.Provider>;
}
