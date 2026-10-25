// The jackpot block's state (spec 4): live before T, deadline passed from T, stale when the data is old.
export type JackpotState = 'live' | 'passed' | 'stale';

export function jackpotState(deadlineMs: number, now: number, stale: boolean): JackpotState {
  if (stale) return 'stale';
  return now >= deadlineMs ? 'passed' : 'live';
}
