// @vitest-environment node
// F6 part A (spec S5, 6.2, 6.3, 7; J5): the Vault page's money paths through the page's own code (vault/*) and the
// real wasm (src/wasm/dev) behind the worker's message path, on the stand-in node of the F4 tests (`Sim`). The
// objects are crates/web's fixtures (src/test/fixtures/page-inputs.json: the deposit and withdrawal fixtures, the
// settlements of every mode, the fixture vault at round 2). Covered: the page's figures only from objects the node
// holds and their states checked, another owner's objects never shown as the wallet's, a stale or lying indexer
// refused; each action's rows bound to its built transaction; the press signing exactly that build and accepted on
// its own outputs; a wallet that changes what it signs (or signs nothing) refused before anything is sent; amounts
// the vault does not take refused before anything is built.
import { beforeAll, describe, expect, it } from 'vitest';
import type { ApiObject, CovUtxo, DepositInput, QuickKey, SettleInput, Utxo, WithdrawInput } from '@wasm/web.js';
import { unwrap } from '@/lib/errors';
import { kasExact, sompiOfKas } from '@/lib/format';
import { MemoryKV } from '@/store/kv';
import fixtures from '@/test/fixtures/page-inputs.json';
import { ctxOf, fast, lottery, need, Sim } from '@/test/ticketWorld';
import { QuickKeyStore, QuickWalletAdapter } from '@/wallet/quick';
import type { WalletAdapter } from '@/wallet/types';
import { loadVault } from './load';
import { prepareDeposit, prepareSettle, prepareWithdraw, sendVault } from './flow';

const fx = fixtures as unknown as Record<string, unknown>;
const dep = fx['request_deposit'] as DepositInput;
const wpart = fx['request_withdraw 0 partial'] as WithdrawInput;
const ecdsa = fx['request_withdraw 1 partial'] as WithdrawInput;
const settles = ['settle refund', 'settle withdrawal pay', 'settle withdrawal accrue', 'settle deposit last'].map(
  (n) => [n, fx[n] as SettleInput] as const,
);
const vaultAt2 = fx['vault at 2'] as CovUtxo;

let key: QuickKey;
let quick: QuickWalletAdapter;
let spk: string;

beforeAll(async () => {
  key = unwrap(await lottery.call('quickWalletNew'));
  spk = unwrap(await lottery.call('scriptOf', key.address));
  const ks = new QuickKeyStore(new MemoryKV(), 'testnet-10');
  await ks.replace(null, key);
  const coins = dep.funds.coins.map((c) => ({ ...c, scriptPublicKey: spk, amount: '10000000000' }));
  quick = new QuickWalletAdapter(lottery, ks, 'testnet-10', () => Promise.resolve(coins), key);
}, 120_000);

function apiObject(role: ApiObject['role'], c: CovUtxo): ApiObject {
  return {
    role,
    transactionId: c.utxo.transactionId,
    index: c.utxo.index,
    amount: c.utxo.amount,
    blockDaaScore: c.utxo.blockDaaScore,
    scriptPublicKey: c.utxo.scriptPublicKey,
    state: c.state,
  };
}

interface Served {
  vault: CovUtxo;
  positions?: CovUtxo[];
  requests?: CovUtxo[];
  records?: CovUtxo[];
  /** Served by the indexer but not live on the node. */
  spent?: CovUtxo[];
  /** Objects whose served state is replaced. */
  lie?: { object: CovUtxo; state: string };
}

function world(s: Served) {
  const sim = new Sim(dep.covenantId);
  sim.vdaa = '3000000';
  const all = [s.vault, ...(s.positions ?? []), ...(s.requests ?? []), ...(s.records ?? [])];
  for (const c of all) sim.add(c.utxo);
  const served = (role: ApiObject['role'], c: CovUtxo) => {
    const o = apiObject(role, c);
    return s.lie && s.lie.object === c ? { ...o, state: s.lie.state } : o;
  };
  const extra = (s.spent ?? []).map((c) => served('position', c));
  const ctx = ctxOf(sim, {
    vault: () => Promise.resolve({ owner: undefined, objects: [served('vault', s.vault)] }),
    positions: (o) =>
      Promise.resolve({ owner: o, objects: [...(s.positions ?? []).map((c) => served('position', c)), ...extra] }),
    requests: (o) =>
      Promise.resolve({
        owner: o,
        objects: [
          ...(s.requests ?? []).map((c) => served('request', c)),
          ...(s.records ?? []).map((c) => served('record', c)),
        ],
      }),
  });
  return { sim, ctx };
}

const addressOf = async (scriptPublicKey: string) => unwrap(await lottery.call('addressOf', scriptPublicKey));

describe('the Vault page (spec S5, 6.3)', () => {
  it('shows only node-checked objects of the wallet; a stale or lying indexer is refused', async () => {
    const owner = await addressOf(wpart.owner.coin.scriptPublicKey);
    // Another owner's position (the fixtures share outpoints: this one is moved to its own).
    const other = {
      ...ecdsa,
      position: { ...ecdsa.position, utxo: { ...ecdsa.position.utxo, transactionId: 'cd'.repeat(32) } },
    };
    const spent: CovUtxo = { ...wpart.position, utxo: { ...wpart.position.utxo, transactionId: 'ab'.repeat(32) } };
    const w = world({ vault: wpart.vault, positions: [wpart.position, other.position], spent: [spent] });
    const d = await loadVault(w.ctx, owner);
    expect(d.page.positions.map((p) => p.owner)).toEqual([owner]);
    expect(d.page.foreign).toBe(1);
    expect(d.page.positions[0]?.partial).toBeTruthy();
    expect(d.vault).toEqual(wpart.vault);
    // Without a wallet: the vault's figures only.
    const none = await loadVault(w.ctx, null);
    expect(none.page.positions).toEqual([]);
    expect(none.page.vault).toEqual(d.page.vault);
    // A vault the node does not hold (an indexer behind the chain): nothing shown as checked.
    const stale = world({ vault: wpart.vault });
    stale.sim.spend(wpart.vault.utxo);
    await expect(loadVault(stale.ctx, owner)).rejects.toMatchObject({ code: 'chain_mismatch' });
    // A position served with another state than its UTXO's.
    const lying = world({
      vault: wpart.vault,
      positions: [wpart.position],
      lie: { object: wpart.position, state: other.position.state },
    });
    await expect(loadVault(lying.ctx, owner)).rejects.toMatchObject({ code: 'chain_mismatch' });
    // An indexer serving only another owner's objects under this wallet: none is shown as the wallet's.
    const swapped = world({ vault: wpart.vault, positions: [other.position] });
    const sd = await loadVault(swapped.ctx, owner);
    expect(sd.page.positions).toEqual([]);
    expect(sd.page.foreign).toBe(1);
  }, 120_000);
});

describe('deposit request (spec S5, 7)', () => {
  it('its rows are the built transaction; the press signs it; a wallet changing it is refused', async () => {
    const w = world({ vault: dep.vault });
    const env = { ctx: w.ctx, wallet: quick, address: key.address };
    const d = await loadVault(w.ctx, key.address);
    expect(d.page.vault.requestOpen).toBe(true);
    const p = await prepareDeposit(env, d, '2500000000');
    if (p.kind !== 'deposit') throw new Error('not a deposit');
    const f = p.figures;
    expect(f.amount).toBe('2500000000');
    expect(f.owner).toBe(key.address);
    expect(f.reserve).toBe(d.page.vault.reserve);
    expect(f.round).toBe(d.page.vault.nextDraw);
    expect(f.revocable).toBe(false);
    expect(f.refundable).toBe(true);
    expect(BigInt(f.amount) + BigInt(f.reserve) + BigInt(f.fee)).toBe(BigInt(f.total));
    expect(BigInt(p.built.summary.walletIn) - BigInt(p.built.summary.walletBack)).toBe(BigInt(f.total));
    // A wallet that moves the request elsewhere (its value) is refused before anything is sent.
    const thief: WalletAdapter = {
      kind: 'quick',
      connect: () => quick.connect(),
      network: () => quick.network(),
      address: () => quick.address(),
      utxos: () => quick.utxos(),
      signInputs: async (b, ix) => {
        const tx = JSON.parse(await quick.signInputs(b, ix)) as { outputs: { value: string }[] };
        need(tx.outputs[1]).value = String(BigInt(need(tx.outputs[1]).value) + 1n);
        return JSON.stringify(tx);
      },
    };
    await expect(sendVault({ ...env, wallet: thief, ...fast }, p, () => undefined)).rejects.toMatchObject({
      code: 'signed_changed',
    });
    expect(w.sim.submitted).toEqual([]);
    // The honest press: accepted once the request is live on the node.
    const events: string[] = [];
    const id = await sendVault({ ...env, ...fast }, p, (e) => events.push(e.kind === 'step' ? e.step : 'checking'));
    expect(id).toBe(p.built.id);
    expect(w.sim.submitted.map((s) => s.id)).toEqual([p.built.id]);
    expect(events).toEqual(['built', 'signed', 'sent', 'accepted']);
    expect(w.sim.live.has(`${p.built.id}:${f.request.index}`)).toBe(true);
    // Below MIN_DEPOSIT nothing is built.
    await expect(prepareDeposit(env, d, String(BigInt(d.page.vault.minDeposit) - 1n))).rejects.toMatchObject({
      code: 'refused',
    });
  }, 120_000);
});

describe('withdrawal request (spec S5, design 5.4, D86)', () => {
  it('the whole position or a part the vault takes; rows bound; a wallet that signs nothing is refused', async () => {
    const owner = await addressOf(wpart.owner.coin.scriptPublicKey);
    const w = world({ vault: wpart.vault, positions: [wpart.position] });
    const d = await loadVault(w.ctx, owner);
    const pos = need(d.page.positions[0]);
    // The owner's wallet stands in: its coin is the fixture owner coin; it signs nothing.
    const owners: WalletAdapter = {
      kind: 'quick',
      connect: () => Promise.resolve(owner),
      network: () => Promise.resolve('testnet-10'),
      address: () => Promise.resolve(owner),
      utxos: () => Promise.resolve([wpart.owner.coin as Utxo]),
      signInputs: (b) => Promise.resolve(b.transaction),
    };
    const env = { ctx: w.ctx, wallet: owners, address: owner };
    const whole = await prepareWithdraw(env, d, pos, null);
    if (whole.kind !== 'withdraw') throw new Error('not a withdrawal');
    expect(whole.figures.whole).toBe(true);
    expect(whole.figures.shares).toBe(pos.shares);
    expect(whole.figures.reserve).toBe('0');
    expect(whole.figures.total).toBe(whole.built.fee);
    const part = await prepareWithdraw(env, d, pos, '2000000000');
    if (part.kind !== 'withdraw') throw new Error('not a withdrawal');
    expect(part.figures.whole).toBe(false);
    expect(part.figures.shares).toBe(part.choice.shares);
    expect(BigInt(part.figures.remainder)).toBe(BigInt(pos.shares) - BigInt(part.figures.shares));
    expect(BigInt(part.figures.total)).toBe(BigInt(part.built.fee) + BigInt(d.page.vault.reserve));
    expect(part.figures.positionNext?.index).toBe(2);
    // Amounts the vault does not take: below wmin's worth, or leaving less than wmin.
    await expect(prepareWithdraw(env, d, pos, '500000000')).rejects.toMatchObject({ code: 'vault_amount' });
    // Least and Most as the panel prints them, typed back (F6 part A review S1): both taken.
    const range = need(pos.partial);
    for (const shown of [range.leastShown, range.mostShown, range.leastAmount, range.mostAmount]) {
      const typed = need(sompiOfKas(kasExact(shown)));
      await expect(prepareWithdraw(env, d, pos, typed)).resolves.toMatchObject({ kind: 'withdraw' });
    }
    await expect(prepareWithdraw(env, d, pos, String(BigInt(range.leastAmount) - 1n))).rejects.toMatchObject({
      code: 'vault_amount',
    });
    const most = BigInt(need(pos.partial).mostAmount);
    await expect(prepareWithdraw(env, d, pos, String(most + 200_000_000n))).rejects.toMatchObject({
      code: 'vault_amount',
    });
    // A wallet that returns the transaction unsigned: refused by the post-sign check, nothing sent.
    await expect(sendVault({ ...env, ...fast }, part, () => undefined)).rejects.toMatchObject({
      code: 'signed_missing',
    });
    expect(w.sim.submitted).toEqual([]);
    // Another wallet than the position's owner builds nothing.
    await expect(prepareWithdraw({ ...env, address: key.address, wallet: quick }, d, pos, null)).rejects.toThrow();
    // An ECDSA owner's position (made by other tools) is listed but not withdrawable from the page.
    const eowner = await addressOf(ecdsa.owner.coin.scriptPublicKey);
    const e = world({ vault: ecdsa.vault, positions: [ecdsa.position] });
    const ed = await loadVault(e.ctx, eowner);
    expect(ed.page.positions.length).toBe(1);
  }, 120_000);
});

describe('settlement (spec S5, M5 A6.1)', () => {
  it.each(settles)(
    '%s: rows from the covenant reference, bound; accepted on its own outputs',
    async (_name, s) => {
      const owner = s.funds.change;
      const w = world({ vault: vaultAt2, requests: [s.request], records: [s.record] });
      const d = await loadVault(w.ctx, owner);
      const r = need(d.page.requests[0]);
      expect(r.status).toBe('settleDue');
      const env = { ctx: w.ctx, wallet: quick, address: key.address };
      const p = await prepareSettle(env, r);
      if (p.kind !== 'settle') throw new Error('not a settlement');
      expect(p.figures.settle).toEqual(r.settle);
      // Anyone may settle (D77): the payout goes to the request's owner, not to this wallet.
      expect(p.figures.payoutToWallet).toBe(false);
      expect(BigInt(p.built.summary.walletIn) - BigInt(p.built.summary.walletBack)).toBe(BigInt(p.figures.total));
      const id = await sendVault({ ...env, ...fast }, p, () => undefined);
      expect(id).toBe(p.built.id);
      expect(w.sim.submitted.map((x) => x.id)).toEqual([p.built.id]);
    },
    120_000,
  );

  it('a request whose record the indexer does not list is not settle due', async () => {
    const [, s] = need(settles[0]);
    const w = world({ vault: vaultAt2, requests: [s.request] });
    const d = await loadVault(w.ctx, s.funds.change);
    expect(need(d.page.requests[0]).status).toBe('unknown');
    await expect(
      prepareSettle({ ctx: w.ctx, wallet: quick, address: key.address }, need(d.page.requests[0])),
    ).rejects.toThrow();
  }, 120_000);
});
