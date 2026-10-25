// The page <-> worker message layer (spec 1.1 DX: no hand-written mirror of a Rust type). The call names
// are crates/web's export names; every argument and result type is taken from the generated web.d.ts
// (`typeof import('@wasm/web.js')`), so a change on the Rust side is a type error here.
import type * as Wasm from '@wasm/web.js';

/** The exports the page may call (functions of crates/web; the wasm-bindgen glue's own are not listed). */
export const CALLS = [
  'start',
  'errorTable',
  'indexerLimits',
  'jackpotOdds',
  'decodeState',
  'decodeRedeem',
  'verifyState',
  'checkRoots',
  'winningNumbers',
  'feeFor',
  'wMin',
  'addressOf',
  'scriptOf',
  'buildBuy',
  'buildClaim',
  'buildReturnDeposit',
  'buildClose',
  'buildDraw',
  'buildRequestDeposit',
  'buildRequestWithdraw',
  'buildSettle',
  'assemble',
  'parseIndexerStatus',
  'parseIndexerRounds',
  'parseIndexerRound',
  'parseIndexerShard',
  'parseIndexerPurchases',
  'parseIndexerAnchorProof',
  'parseIndexerTickets',
  'parseIndexerObjects',
  'parseIndexerError',
  'checkIndexerInstance',
  'checkIndexerObjects',
  'checkIndexerRound',
  'quickWalletNew',
  'quickWalletImport',
  'quickWalletSign',
  'nodeConnect',
  'nodeDisconnect',
  'nodeInfo',
  'nodeEntries',
  'nodeCoins',
  'nodeFeeRate',
  'nodeAnchorBlock',
  'nodeSubmit',
] as const satisfies readonly (keyof typeof Wasm)[];

export type CallName = (typeof CALLS)[number];
type Api = typeof Wasm;
export type CallArgs<K extends CallName> = Parameters<Api[K]>;
export type CallResult<K extends CallName> = Awaited<ReturnType<Api[K]>>;

export type { ApiError, ErrorName, Outcome } from '@wasm/web.js';

/** Page to worker. */
export interface Request {
  id: number;
  name: CallName;
  args: unknown[];
}

/** Worker to page: the call's value, or a crash (a thrown exception: the wasm trapped, e.g. a panic). */
export type Response = { id: number; ok: true; value: unknown } | { id: number; ok: false; crash: string };

export function isCall(name: unknown): name is CallName {
  return typeof name === 'string' && (CALLS as readonly string[]).includes(name);
}
