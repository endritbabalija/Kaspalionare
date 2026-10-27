// S5 Vault's state (spec 3 S5; J5; F6 part A): the vault and the wallet's positions and requests read and checked
// every 10 s (`loadVault`); with a wallet connected, each action is built as soon as its inputs are known (the
// deposit's amount, the withdrawal's whole or part, a settle-due request), so its rows show the fee and the total
// before anything is signed; a press signs exactly that build. A build belongs to one key (the action, its inputs,
// the wallet and the live objects it spends): another key drops it at once (its late answer too), as the round
// page's caller panel does (F5 review S6). One action runs at a time (one wallet request at a time). An action
// whose outcome the page does not learn (submit_unknown) keeps its press disabled while the objects it spent are
// still the live ones (a later press would only be refused by the node, or would be a second, deliberate request).
import { useCallback, useEffect, useRef, useState } from 'react';
import type { DepositFigures, PositionCard, RequestCard, SettleFigures, WithdrawFigures } from '@wasm/web.js';
import type { ActionView, VaultTx } from '@/components/vaultStates';
import type { WithdrawMode } from '@/components/WithdrawPanel';
import { describe } from '@/lib/errors';
import { sompiOfKas } from '@/lib/format';
import type { Ctx } from '@/net/round';
import { loadVault, type VaultData } from '@/vault/load';
import { prepareDeposit, prepareSettle, prepareWithdraw, sendVault, type Prepared } from '@/vault/flow';
import type { WalletSession } from '@/wallet/context';

export const VAULT_REFRESH_MS = 10_000;
/** The pause after a keystroke before an amount is built. */
const DEBOUNCE_MS = 300;

export type VaultLoad =
  | { kind: 'loading' }
  | { kind: 'error'; code: string | null; message: string }
  | { kind: 'ready'; data: VaultData; stale: boolean };

const opKey = (u: { transactionId: string; index: number }) => `${u.transactionId.toLowerCase()}:${u.index}`;

interface Slot {
  key: string;
  view: ActionView<unknown>;
  prep?: Prepared;
}

export interface WithdrawForm {
  position: string | null;
  mode: WithdrawMode;
  amount: string;
}

export function useVault(ctx: Ctx, wallet: WalletSession) {
  const address = wallet.address;
  const adapter = wallet.adapter;
  const [load, setLoad] = useState<VaultLoad>({ kind: 'loading' });
  const [depositAmount, setDepositAmount] = useState('');
  const [withdraw, setWithdraw] = useState<WithdrawForm>({ position: null, mode: 'whole', amount: '' });
  /** Each action's build and progress, by action name ("deposit", "withdraw", "settle:<request>"). */
  const [slots, setSlots] = useState<Record<string, Slot>>({});
  /** Actions whose last press has an unknown outcome: the build key they were sent for. */
  const [blocked, setBlocked] = useState<Record<string, string>>({});
  const busy = useRef(false);
  const [running, setRunning] = useState<string | null>(null);
  /** The last press's progress, shown on its own (a whole withdrawal's position card or a settled request's
   * card leaves the page once the chain moves on). */
  const [last, setLast] = useState<{ name: string; tx: VaultTx } | null>(null);
  const wanted = useRef<Record<string, string>>({});
  const [gen, setGen] = useState(0);

  const reload = useCallback(async () => {
    try {
      const data = await loadVault(ctx, address ?? null);
      setLoad({ kind: 'ready', data, stale: false });
    } catch (e) {
      const d = describe(e);
      if (d.detail) console.warn(`vault: ${d.code ?? 'error'}: ${d.detail}`);
      setLoad((l) =>
        l.kind === 'ready' ? { ...l, stale: true } : { kind: 'error', code: d.code, message: d.message },
      );
    }
  }, [ctx, address]);

  useEffect(() => {
    setLoad({ kind: 'loading' });
    setSlots({});
    void reload();
    const t = setInterval(() => {
      if (!busy.current) void reload();
    }, VAULT_REFRESH_MS);
    return () => clearInterval(t);
  }, [reload]);

  const data = load.kind === 'ready' && load.data.owner === (address ?? null) ? load.data : undefined;
  const vaultOp = data ? opKey(data.vault.utxo) : '';

  // The key each action's build belongs to (empty: nothing to build).
  const depositSompi = sompiOfKas(depositAmount);
  const position = data?.page.positions.find((p) => opKey(p.object.utxo) === withdraw.position);
  const withdrawSompi = withdraw.mode === 'part' ? sompiOfKas(withdraw.amount) : null;
  const keys: Record<string, string> = {};
  if (data && address && adapter) {
    if (depositSompi) keys.deposit = `${vaultOp}|${address}|${depositSompi}`;
    if (position && position.ownerKey && (withdraw.mode === 'whole' || withdrawSompi)) {
      keys.withdraw = `${vaultOp}|${opKey(position.object.utxo)}|${address}|${withdraw.mode}|${withdrawSompi ?? ''}`;
    }
    for (const r of data.page.requests) {
      if (r.status === 'settleDue' && r.record)
        keys[`settle:${opKey(r.object.utxo)}`] = `${opKey(r.object.utxo)}|${opKey(r.record.utxo)}|${address}`;
    }
  }
  // `gen` moves after every press, so the spent build is rebuilt even for the same key.
  const keysText = `${JSON.stringify(keys)}#${gen}`;
  const keysNow = useRef(keys);
  keysNow.current = keys;

  // Build every action whose key changed (debounced); drop builds of keys no longer wanted.
  useEffect(() => {
    const want = keysNow.current;
    wanted.current = want;
    setSlots((s) => {
      const next: Record<string, Slot> = {};
      for (const [name, slot] of Object.entries(s)) {
        // Keep a slot with progress (the press's outcome) or of a key still wanted.
        if (slot.view.tx || want[name] === slot.key) next[name] = slot;
      }
      return next;
    });
    if (!data || !adapter || !address || busy.current) return;
    const env = { ctx, wallet: adapter, address };
    const t = setTimeout(() => {
      for (const [name, key] of Object.entries(want)) {
        setSlots((s) => {
          const cur = s[name];
          if (cur && cur.key === key && (cur.prep || cur.view.estimating || cur.view.error)) return s;
          return { ...s, [name]: { key, view: { estimating: true } } };
        });
        const build = (): Promise<Prepared> => {
          if (name === 'deposit') return prepareDeposit(env, data, depositSompi ?? '');
          if (name === 'withdraw' && position) return prepareWithdraw(env, data, position, withdrawSompi);
          const r = data.page.requests.find((x) => `settle:${opKey(x.object.utxo)}` === name);
          if (!r) return Promise.reject(new Error(`no request for ${name}`));
          return prepareSettle(env, r);
        };
        void build().then(
          (prep) =>
            setSlots((s) =>
              wanted.current[name] === key && !s[name]?.view.tx
                ? { ...s, [name]: { key, prep, view: { figures: prep.figures } } }
                : s,
            ),
          (e: unknown) => {
            const d = describe(e);
            if (d.detail) console.warn(`vault ${name}: ${d.code ?? 'error'}: ${d.detail}`);
            setSlots((s) =>
              wanted.current[name] === key && !s[name]?.view.tx
                ? { ...s, [name]: { key, view: { error: d.message } } }
                : s,
            );
          },
        );
      }
    }, DEBOUNCE_MS);
    return () => clearTimeout(t);
    // keysText fixes every input of the builds.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [keysText]);

  const press = useCallback(
    async (name: string) => {
      const slot = slots[name];
      const p = slot?.prep;
      if (!p || !adapter || !address || busy.current || blocked[name] === slot.key) return;
      if (wanted.current[name] !== slot.key) return;
      busy.current = true;
      setRunning(name);
      const setTx = (f: (tx: VaultTx) => VaultTx) => {
        setLast((l) => ({ name, tx: f(l?.name === name ? l.tx : { step: 'built' }) }));
        setSlots((s) => {
          const cur = s[name];
          return cur ? { ...s, [name]: { ...cur, view: { ...cur.view, tx: f(cur.view.tx ?? { step: 'built' }) } } } : s;
        });
      };
      setLast(null);
      setTx(() => ({ step: 'built' }));
      try {
        const id = await sendVault({ ctx, wallet: adapter, address }, p, (e) =>
          setTx((tx) => (e.kind === 'step' ? { ...tx, step: e.step } : { ...tx, checking: e.on })),
        );
        setTx(() => ({ step: 'accepted', id, checking: false }));
        if (name === 'deposit') setDepositAmount('');
      } catch (e) {
        const d = describe(e);
        if (d.detail) console.warn(`vault ${name}: ${d.code ?? 'error'}: ${d.detail}`);
        if (d.code === 'submit_unknown') setBlocked((b) => ({ ...b, [name]: slot.key }));
        setTx((tx) => ({ ...tx, checking: false, failed: d.message }));
        if (d.code === 'wallet_changed' && wallet.kind === 'quick') void wallet.openQuick();
      } finally {
        // The build is spent either way: a press again builds afresh from the chain.
        setSlots((s) => {
          const cur = s[name];
          return cur ? { ...s, [name]: { key: '', view: cur.view } } : s;
        });
        busy.current = false;
        setRunning(null);
        setGen((g) => g + 1);
        void reload();
      }
    },
    [slots, blocked, adapter, address, ctx, reload, wallet],
  );

  const view = <F>(name: string): ActionView<F> & { blocked: boolean } => {
    const s = slots[name];
    const current = s && (s.key === keys[name] || s.view.tx);
    const v = (current ? s.view : {}) as ActionView<F>;
    return { ...v, blocked: !!keys[name] && blocked[name] === keys[name] };
  };

  return {
    load,
    data,
    running,
    last,
    walletConnected: !!address && !!adapter,
    address,
    deposit: {
      amount: depositAmount,
      onAmount: (t: string) => {
        setDepositAmount(t);
        setSlots((s) => (s.deposit?.view.tx ? { ...s, deposit: { key: '', view: {} } } : s));
      },
      view: view<DepositFigures>('deposit'),
      onPress: () => void press('deposit'),
    },
    withdraw: {
      form: withdraw,
      open: (p: PositionCard) => {
        setWithdraw({ position: opKey(p.object.utxo), mode: 'whole', amount: '' });
        setSlots((s) => (s.withdraw?.view.tx ? { ...s, withdraw: { key: '', view: {} } } : s));
      },
      close: () => setWithdraw({ position: null, mode: 'whole', amount: '' }),
      isOpen: (p: PositionCard) => withdraw.position === opKey(p.object.utxo),
      onMode: (mode: WithdrawMode) => setWithdraw((w) => ({ ...w, mode })),
      onAmount: (amount: string) => setWithdraw((w) => ({ ...w, amount })),
      view: view<WithdrawFigures>('withdraw'),
      onPress: () => void press('withdraw'),
    },
    settle: (r: RequestCard) => {
      const name = `settle:${opKey(r.object.utxo)}`;
      return { view: view<SettleFigures>(name), onPress: () => void press(name) };
    },
    reload: () => void reload(),
  };
}
