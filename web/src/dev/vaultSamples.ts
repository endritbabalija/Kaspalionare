// Sample figures for the component catalogue only (S5; F6): a vault, a position, requests in each state and the
// rows of built requests and a settlement. Development builds only (imported by Catalogue.tsx).
import type {
  CovUtxo,
  DepositFigures,
  PositionCard,
  RequestCard,
  SettleFigures,
  SettleView,
  VaultSummary,
  WithdrawFigures,
} from '@wasm/web.js';

const TX = '7a1c0a11223344556677889900aabbccddeeff00112233445566778899c42e10';
const ADDR = 'kaspatest:qqdasdj3abcdefghijklmnopqrstuvwxyz0123456789abcdefghijphyfvu';
const OBJ: CovUtxo = {
  utxo: {
    transactionId: TX,
    index: 1,
    amount: '169000000',
    scriptPublicKey: '0000',
    blockDaaScore: '12000',
    isCoinbase: false,
    covenantId: TX,
  },
  state: '',
};
const OUT = { index: 1, value: '169000000', state: '', scriptPublicKey: '0000', address: ADDR };

export const SAMPLE_VAULT: VaultSummary = {
  round: '11',
  nextDraw: '12',
  value: '103500000000',
  totalAssets: '100000000000',
  totalShares: '100000000000',
  sharePrice: '0.99999999',
  liability: '10400000000',
  pendingDeposits: '2500000000',
  pendingWithdrawals: '1200000000',
  pendingWithdrawalsWorth: '1199999999',
  reserve: '169000000',
  requestFrom: '13300',
  requestOpen: false,
  minDeposit: '1000000000',
  wMin: '1000000000',
  wMinWorth: '999999999',
  source: { transactionId: TX, index: 0 },
};

export const SAMPLE_POSITION: PositionCard = {
  object: OBJ,
  round: '9',
  shares: '2500000000',
  worth: '2499999999',
  owner: ADDR,
  ownerKey: '11'.repeat(32),
  partial: { least: '1000000000', most: '1500000000', leastWorth: '999999999', mostWorth: '1499999999' },
  source: { transactionId: TX, index: 1 },
};

const SETTLE: SettleView = {
  mode: 'convert',
  shares: '2500000000',
  payout: '0',
  converted: '0',
  left: '0',
  accrued: '0',
  requestValue: '169000000',
  drained: true,
  recordRefund: '169000000',
  recordFunder: ADDR,
};

export const SAMPLE_REQUESTS: Record<'pending' | 'due' | 'unknown', RequestCard> = {
  pending: {
    object: OBJ,
    kind: 'deposit',
    amount: '2500000000',
    worth: '2500000000',
    round: '12',
    value: '169000000',
    owner: ADDR,
    status: 'pending',
    record: undefined,
    settle: undefined,
    source: { transactionId: TX, index: 1 },
  },
  due: {
    object: OBJ,
    kind: 'deposit',
    amount: '2500000000',
    worth: '2500000000',
    round: '11',
    value: '169000000',
    owner: ADDR,
    status: 'settleDue',
    record: OBJ,
    settle: SETTLE,
    source: { transactionId: TX, index: 1 },
  },
  unknown: {
    object: OBJ,
    kind: 'withdrawal',
    amount: '1200000000',
    worth: '1199999999',
    round: '10',
    value: '169000000',
    owner: ADDR,
    status: 'unknown',
    record: undefined,
    settle: undefined,
    source: { transactionId: TX, index: 1 },
  },
};

export const SAMPLE_SETTLE: SettleFigures = {
  round: '11',
  kind: 'deposit',
  settle: SETTLE,
  owner: ADDR,
  payoutToWallet: false,
  recordRefundToWallet: true,
  fee: '3662400',
  total: '-165337600',
  recordNext: undefined,
  positionNext: OUT,
  requestNext: undefined,
  payoutIndex: undefined,
};

export const SAMPLE_PAY: SettleView = {
  ...SETTLE,
  mode: 'pay',
  shares: '0',
  payout: '499999999',
  converted: '500000',
  left: '500000',
  drained: false,
  recordRefund: '0',
  recordFunder: undefined,
};

export const SAMPLE_DEPOSIT: DepositFigures = {
  round: '12',
  amount: '2500000000',
  reserve: '169000000',
  fee: '3879000',
  total: '2672879000',
  owner: ADDR,
  requestFrom: '13300',
  revocable: false,
  refundable: true,
  request: OUT,
  vaultNext: { ...OUT, index: 0 },
};

export const SAMPLE_WITHDRAW: WithdrawFigures = {
  round: '12',
  shares: '1200000000',
  positionShares: '2500000000',
  whole: false,
  remainder: '1300000000',
  worth: '1199999999',
  remainderWorth: '1299999999',
  reserve: '169000000',
  fee: '4854600',
  total: '173854600',
  owner: ADDR,
  requestFrom: '13300',
  revocable: false,
  request: OUT,
  positionNext: { ...OUT, index: 2 },
  vaultNext: { ...OUT, index: 0 },
};
