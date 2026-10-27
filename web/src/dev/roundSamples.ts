// Sample figures for the component catalogue only (S3, S4; F5): a verify result with every result, and the rows
// of a built close() and draw(). Development builds only (imported by Catalogue.tsx).
import type { CallerFigures, RoundVerify } from '@wasm/web.js';

const TX = '9f3c0a11223344556677889900aabbccddeeff00112233445566778899b71e21';

export const SAMPLE_VERIFY: RoundVerify = {
  round: '12',
  consistent: false,
  anchorBound: false,
  winning: undefined,
  items: [
    {
      kind: 'state',
      result: 'match',
      role: 'pool',
      outpoint: { transactionId: TX, index: 3 },
      shard: undefined,
      code: undefined,
      detail: '',
    },
    {
      kind: 'state',
      result: 'notCheckable',
      role: 'marker',
      outpoint: { transactionId: TX, index: 0 },
      shard: undefined,
      code: undefined,
      detail: 'spent',
    },
    {
      kind: 'status',
      result: 'match',
      role: undefined,
      outpoint: undefined,
      shard: undefined,
      code: undefined,
      detail: '',
    },
    {
      kind: 'roots',
      result: 'match',
      role: undefined,
      outpoint: undefined,
      shard: undefined,
      code: undefined,
      detail: '',
    },
    { kind: 'tree', result: 'match', role: undefined, outpoint: undefined, shard: 0, code: undefined, detail: '' },
    {
      kind: 'tree',
      result: 'mismatch',
      role: undefined,
      outpoint: undefined,
      shard: 1,
      code: 'tree_mismatch',
      detail: 'sample',
    },
    {
      kind: 'numbers',
      result: 'match',
      role: undefined,
      outpoint: undefined,
      shard: undefined,
      code: undefined,
      detail: '',
    },
    {
      kind: 'anchor',
      result: 'notCheckable',
      role: undefined,
      outpoint: undefined,
      shard: undefined,
      code: undefined,
      detail: 'sample',
    },
  ],
};

export const SAMPLE_CLOSE: CallerFigures = {
  kind: 'close',
  round: '12',
  reward: '2410000000',
  fee: '43400000',
  reserve: '169000000',
  reserves: 1,
  reserveBackWithDraw: true,
  pools: 0,
  reserveBackFrom: undefined,
  record: false,
  absorbed: 0,
  refund: '0',
  callerOutput: 19,
  total: '-2197600000',
  feeAboveReward: false,
};

export const SAMPLE_DRAW_LOW: CallerFigures = {
  kind: 'draw',
  round: '12',
  reward: '5000000',
  fee: '21000000',
  reserve: '338000000',
  reserves: 2,
  reserveBackWithDraw: false,
  pools: 2,
  reserveBackFrom: '6100000',
  record: false,
  absorbed: 3,
  refund: '169000000',
  callerOutput: 9,
  total: '185000000',
  feeAboveReward: true,
};
