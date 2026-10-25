// The states of the claim row and the ticket card (spec 4), read from the wasm's figures and the store
// (no arithmetic here).
import type { ClaimFigures, ClaimView, StoredTicket, TicketCard } from '@wasm/web.js';
import type { TxStep } from './TxProgress';

export type ClaimRowState = 'claimable' | 'claiming' | 'claimed' | 'accrued' | 'fee-exceeds';

export function claimRowState(p: {
  claim?: ClaimView;
  claimed?: boolean;
  figures?: ClaimFigures;
  tx?: { step: TxStep; failed?: string };
}): ClaimRowState {
  if (p.claimed || p.tx?.step === 'accepted') return 'claimed';
  if (p.tx && !p.tx.failed) return 'claiming';
  if (p.figures?.feeExceedsPrize) return 'fee-exceeds';
  if (p.claim?.mode === 'accrue') return 'accrued';
  return 'claimable';
}

export type TicketCardState =
  | 'open'
  | 'win'
  | 'no-win'
  | 'partly-claimed'
  | 'claimed'
  | 'void'
  | 'returnable'
  | 'returned'
  | 'spent'
  | 'incomplete';

export function ticketCardState(p: { stored: StoredTicket; card?: TicketCard; spent?: boolean }): TicketCardState {
  const { stored, card } = p;
  if (stored.ended) return stored.ended.kind === 'returned' ? 'returned' : 'claimed';
  if (p.spent || !card) return 'spent';
  if (card.returnable && !card.backWithLastClaim) return 'returnable';
  if (card.stage === 'void') return 'void';
  if (card.stage === 'open') return 'open';
  const winning = card.combos.some((c) => c.status === 'claimable');
  // A winning tier whose pool the copy does not show: neither "no win" nor "claimed" can be said (F4 review M2).
  if (!winning && card.combos.some((c) => c.status === 'unknown')) return 'incomplete';
  if (card.claimed > 0) return winning ? 'partly-claimed' : 'claimed';
  return winning ? 'win' : 'no-win';
}
