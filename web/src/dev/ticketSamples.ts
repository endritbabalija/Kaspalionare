// DEV ONLY (the catalogue): sample ticket cards and claim figures in every state of spec 4. Sample values,
// shaped as the wasm returns them (the generated types); no lottery arithmetic.
import type { ClaimFigures, ClaimView, ComboCard, StoredTicket, TicketCard } from '@wasm/web.js';

const COV = '9f3c0a11223344556677889900aabbccddeeff00112233445566778899b71e21';
const PAYOUT = 'kaspatest:qqdasdj3abcdefghijklmnopqrstuvwxyz0123456789abcdefghijphyfvu';
const TX = '7a1c0a11223344556677889900aabbccddeeff00112233445566778899b70011';

export const sampleStored = (round: string, ended?: StoredTicket['ended']): StoredTicket => ({
  key: `${TX}:1`,
  covenantId: COV,
  network: 'testnet-10',
  round,
  shardIndex: 3,
  ticket: {
    utxo: {
      transactionId: TX,
      index: 1,
      amount: '26000000',
      scriptPublicKey: '0000aa',
      blockDaaScore: '9000',
      isCoinbase: false,
      covenantId: COV,
    },
    state: '00',
  },
  bundle: [
    { main: [3, 7, 12, 19, 28], bonus: 2 },
    { main: [1, 5, 9, 14, 30], bonus: 1 },
  ],
  payout: PAYOUT,
  ended,
  savedMs: '1',
});

export const claimView = (mode: ClaimView['mode'], prize: string, h = '1'): ClaimView => ({
  tier: mode === 'accrue' ? 4 : 1,
  tierName: mode === 'accrue' ? '3 + bonus' : '5',
  slice: prize,
  h,
  sharedWith: String(Number(h) - 1),
  prize,
  mode,
  belowPMin: mode === 'accrue',
});

const combo = (index: number, status: ComboCard['status'], claim?: ClaimView): ComboCard => ({
  index,
  combination: index === 0 ? { main: [3, 7, 12, 19, 28], bonus: 2 } : { main: [1, 5, 9, 14, 30], bonus: 1 },
  matchedMain:
    status === 'open' ? undefined : index === 0 ? [true, true, true, true, true] : [false, false, false, false, true],
  bonusMatched: status === 'open' ? undefined : false,
  tier: claim?.tier,
  tierName: claim?.tierName,
  status,
  claim,
});

export function sampleCard(kind: 'open' | 'win' | 'no-win' | 'partly' | 'void' | 'returnable'): TicketCard {
  const base: TicketCard = {
    round: '12',
    deadline: '13662',
    b: 2,
    value: '26000000',
    payout: PAYOUT,
    stage: 'drawn',
    winning: { main: [3, 7, 12, 19, 28], bonus: 4 },
    claimDeadline: '6100000',
    combos: [],
    claimed: 0,
    backWithLastClaim: false,
    returnFrom: '6100000',
    returnable: undefined,
  };
  switch (kind) {
    case 'open':
      return {
        ...base,
        stage: 'open',
        winning: undefined,
        claimDeadline: undefined,
        combos: [combo(0, 'open'), combo(1, 'open')],
        returnFrom: '12109662',
      };
    case 'win':
      return {
        ...base,
        combos: [combo(0, 'claimable', claimView('pay', '1240000000', '2')), combo(1, 'noWin')],
        backWithLastClaim: true,
      };
    case 'no-win':
      return { ...base, combos: [combo(0, 'noWin'), combo(1, 'noWin')] };
    case 'partly':
      return {
        ...base,
        claimed: 1,
        combos: [
          combo(0, 'claimed', { ...claimView('pay', '1240000000'), tier: 1 }),
          combo(1, 'claimable', claimView('accrue', '2240000')),
        ],
        backWithLastClaim: true,
      };
    case 'void':
      return {
        ...base,
        stage: 'void',
        winning: undefined,
        claimDeadline: undefined,
        combos: [combo(0, 'noWin'), combo(1, 'noWin')],
        returnFrom: '12109662',
      };
    case 'returnable':
      return {
        ...base,
        combos: [combo(0, 'noWin'), combo(1, 'noWin')],
        returnable: { path: 'alone', pool: undefined },
      };
  }
}

export function sampleFigures(c: ClaimView, fee: string, valueBack = '0'): ClaimFigures {
  const prize = BigInt(c.prize);
  return {
    round: '12',
    index: 0,
    combination: { main: [3, 7, 12, 19, 28], bonus: 2 },
    claim: c,
    payout: PAYOUT,
    paidNow: c.mode === 'accrue' ? '0' : c.prize,
    valueBack,
    accrued: c.mode === 'accrue' ? c.prize : '0',
    fee,
    feeExceedsPrize: BigInt(fee) > prize,
    claimDeadline: '6100000',
    ticketNext: undefined,
    poolNext: { index: 0, value: '1', state: '00', scriptPublicKey: '0000aa' },
  };
}

export const SAMPLE_TX = TX;
