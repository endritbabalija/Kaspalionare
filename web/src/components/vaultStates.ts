// The Vault page's component states (spec 4 "Vault position card": position, pending request, settle due; F6),
// derived from the wasm's figures only.
import type { RequestCard } from '@wasm/web.js';
import type { TxStep } from './TxProgress';

export type VaultCardState = 'position' | 'pending' | 'settle-due' | 'unknown';

export function requestCardState(r: RequestCard): VaultCardState {
  if (r.status === 'settleDue') return 'settle-due';
  return r.status === 'pending' ? 'pending' : 'unknown';
}

export interface VaultTx {
  step: TxStep;
  failed?: string;
  checking?: boolean;
  id?: string;
}

/** One action of the page (a deposit, a withdrawal, a settlement): its built figures and its progress. */
export interface ActionView<F> {
  figures?: F;
  estimating?: boolean;
  /** A build failure: its one message. */
  error?: string;
  tx?: VaultTx;
  /** The outcome of the last press is not known: no second press. */
  blocked?: boolean;
}

export type ActionState = 'idle' | 'estimating' | 'ready' | 'running' | 'accepted' | 'failed';

export function actionState(a: ActionView<unknown>): ActionState {
  if (a.tx?.step === 'accepted') return 'accepted';
  if (a.tx?.failed) return 'failed';
  if (a.tx) return 'running';
  if (a.figures) return 'ready';
  if (a.estimating) return 'estimating';
  return a.error ? 'failed' : 'idle';
}
