// The caller panel's state (spec 4 "Caller panel": not due, due with reward, reward below fee; then running, done).
import type { CallerFigures } from '@wasm/web.js';
import type { Due } from '@/rounds/caller';
import type { TxStep } from './TxProgress';

export type CallerPanelState = 'none' | 'not-due' | 'due' | 'reward-below-fee' | 'running' | 'done';

export function callerPanelState(p: {
  due: Due;
  figures?: CallerFigures;
  tx?: { step: TxStep; failed?: string };
}): CallerPanelState {
  if (p.tx?.step === 'accepted') return 'done';
  if (p.tx && !p.tx.failed) return 'running';
  if (p.due.kind === 'none') return 'none';
  if (p.due.kind !== 'due') return 'not-due';
  return p.figures?.feeAboveReward ? 'reward-below-fee' : 'due';
}
