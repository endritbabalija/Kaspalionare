// One example round (round 7), computed with the protocol's own arithmetic: a BigInt port of
// crates/lottery/src/game.rs (round_amounts, estimate/SizeRule::choose, JackpotRule/fix_jackpot,
// vault_settle, boost_after, convert, settle). Amounts are sompi (1 KAS = 10^8 sompi).
// The inputs (ticket counts, referrals, vault size, backer requests) are invented for the figure.
const KAS = 100_000_000n;
const BP = 10_000n;
const N = 142_506n;
const VS = 1_000n;
const VA = 1n;
const MIN_DEPOSIT = 10n * KAS;
const P_MIN = 5_000_000n;

// Controller parameters: the test instance of crates/lottery/src/close.rs (e, j, g, margin, cap, m range).
const PRM = { price: KAS, e: 1_500n, j: 1_000n, g: 100n, margin: 11_700n, cap: 2_000n, mMin: 2n, mMax: 50n };
const SH = { F: 500n, R: 1_000n, C: 50n, D: 50n, T: [400n, 300n, 300n] };
const sT = SH.T.reduce((a, b) => a + b, 0n);
const sH = BP - SH.F - SH.R - SH.C - SH.D - sT;
const wShare = BP - sT - SH.R - SH.C - SH.D; // = s_H + s_F

const muldiv = (x, s) => (x / BP) * s + ((x % BP) * s) / BP;
const mulfrac = (a, r, c) => (a * r) / c;
const minB = (a, b) => (a < b ? a : b);
const maxB = (a, b) => (a > b ? a : b);
const allowance = muldiv(PRM.price * sH, BP - PRM.e) / BP;
const whaleAllowance = (PRM.price * wShare) / PRM.margin;

// SizeRule::choose (close of round r picks m(r+1) from the jackpot estimate)
function chooseM(vRec, h, potLeft, bR) {
  const jHouse = muldiv(vRec + h, PRM.j);
  const jj = jHouse + potLeft + bR;
  const g2 = muldiv(jj, PRM.g);
  let lo = PRM.mMin;
  let hi = PRM.mMax;
  let any = true;
  for (const [c, s] of [[jHouse - g2, allowance * N - g2], [jj - g2, whaleAllowance * N - g2]]) {
    if (s > 0n) { if (c > 0n) lo = maxB(lo, (c - 1n) / s + 1n); }
    else if (c > 0n) any = false;
    else if (s < 0n) hi = minB(hi, -c / -s);
  }
  return any && lo <= hi ? lo : PRM.mMax;
}

// JackpotRule + fix_jackpot (draw of round r fixes J(r+1) from V' and O')
function fixJackpot(vPrime, oPrime, m) {
  const nm = N * m;
  const eLim = allowance * nm;
  const wLim = whaleAllowance * nm;
  const cv = muldiv(vPrime, PRM.cap);
  const pass = (x, y) => {
    if (x > eLim || x > cv || y > wLim - x) return false;
    const gg = muldiv(x + y, PRM.g);
    const d = m - 1n;
    return gg <= (eLim - x) / d && gg <= (wLim - x - y) / d && gg <= (cv - x) / d;
  };
  const largest = (lo, hi, pred) => { while (lo < hi) { const mid = lo + (hi - lo + 1n) / 2n; if (pred(mid)) lo = mid; else hi = mid - 1n; } return lo; };
  const xb = muldiv(vPrime, PRM.j);
  const x1 = pass(xb, 0n) ? xb : largest(0n, xb - 1n, (x) => pass(x, 0n));
  const y = largest(0n, oPrime, (yy) => pass(x1, yy));
  const top = maxB(minB(eLim, cv), x1);
  const x2 = largest(x1, top, (x) => pass(x, y));
  const J = x2 + y;
  const g2 = muldiv(J, PRM.g);
  return { jHouse: x2, jBoost: y, J, g2, L: x2 + g2 * (m - 1n), base: xb };
}

// ---------------------------------------------------------------- the round
const K = 16;
const M = 10n; // m(7), chosen by close() of round 6
const COUNTS = [27, 24, 26, 25, 29, 23, 26, 28, 24, 25, 27, 26, 25, 24, 26, 27];
const SOLD = COUNTS.reduce((a, b) => a + b, 0);
const S = BigInt(SOLD) * KAS;
const P_R = 260n * ((KAS * SH.R) / BP); // 260 buyers named a referrer, 0.1 KAS each
const F = muldiv(S, SH.F);
const C_CLOSE = muldiv(S, SH.C);
const C_DRAW = muldiv(S, SH.D);
const B = SH.T.map((s) => muldiv(S, s));
const H_R = muldiv(S, sH);
const B_R = S - P_R - F - C_CLOSE - C_DRAW - B.reduce((a, b) => a + b, 0n) - H_R;

// The vault entering round 7: the previous draw had nothing pending, so V'(6) = V_rec = A.
const A0 = 8_000_000n * KAS;
const S0 = 7_750_000n * KAS; // total_shares (share unit 1 sompi at genesis; the price has since risen)
const J7 = fixJackpot(A0, 0n, M); // J(7), fixed by draw() of round 6

// Backer requests made during round 7 (each one spends the vault, then the next waits 600 DAA).
const REQUESTS = [
  { kind: "deposit", amount: 30_000n * KAS },
  { kind: "withdraw", amount: 190_000n * KAS }, // shares
  { kind: "deposit", amount: 20_000n * KAS },
  { kind: "withdraw", amount: 6_700_000n * KAS }, // shares: a large backer leaving
];
const D = REQUESTS.filter((r) => r.kind === "deposit").reduce((a, r) => a + r.amount, 0n);
const W = REQUESTS.filter((r) => r.kind === "withdraw").reduce((a, r) => a + r.amount, 0n);

// close() of round 7 chooses m(8) from V_rec + H_7 and the boost estimate.
const M8 = chooseM(A0, H_R, 0n, B_R);

// draw() of round 7: TEST-ONLY seed result, frozen counts.
const WIN = [3, 11, 17, 24, 29];
const BONUS = 6;
const HOLDERS = [2, 10];
const H_J = BigInt(HOLDERS.length);
const N2 = 2n;
const V_PRIME = A0 + H_R - J7.jHouse - J7.g2 * N2; // vault_settle (jp, sp)
const O_PRIME = 0n + B_R + 0n; // boost_after: pot left 0, b_r, no J_boost return, every lower pool exists, nothing absorbed
const J8 = fixJackpot(V_PRIME, O_PRIME, M8);

// convert(): aggregate conversion at the post-settlement price.
const n = S0 + VS;
const m = V_PRIME + VA;
const mint = D * (n / m) + mulfrac(D, n % m, m);
const ok = mint >= (D + MIN_DEPOSIT - 1n) / MIN_DEPOSIT;
const D_C = ok ? D : 0n;
const MINT_C = ok ? mint : 0n;
const SLACK = V_PRIME + D_C - J8.L;
const W_STAR = ((SLACK + 1n) * n - 1n) / m;
const WC = minB(W, W_STAR);
const OUT_C = mulfrac(V_PRIME + VA, WC, S0 + VS);
const A1 = V_PRIME + D_C - OUT_C;
const S1 = S0 + MINT_C - WC;

// settle(): each request against the record's remaining totals; the last of each side takes the rest.
const rec = { dep: D, mint: MINT_C, wsh: W, burn: WC, out: OUT_C };
const SETTLED = REQUESTS.map((r) => {
  if (r.kind === "deposit") {
    const sh = r.amount === rec.dep ? rec.mint : mulfrac(rec.mint, r.amount, rec.dep);
    rec.dep -= r.amount;
    rec.mint -= sh;
    return { kind: r.kind, amount: r.amount, shares: sh };
  }
  const conv = r.amount === rec.wsh ? rec.burn : mulfrac(rec.burn, r.amount, rec.wsh);
  const pay = rec.burn === 0n ? 0n : conv === rec.burn ? rec.out : mulfrac(rec.out, conv, rec.burn);
  rec.wsh -= r.amount;
  rec.burn -= conv;
  rec.out -= pay;
  const left = r.amount - conv;
  return { kind: r.kind, amount: r.amount, conv, pay, left, mode: left === 0n ? "final" : pay >= P_MIN ? "pay" : "accrue" };
});

const kas = (x) => Number(x) / 1e8;
const price = (a, s) => Number(a + VA) / Number(s + VS);
export const MODEL = {
  k: K,
  m: Number(M),
  mNext: Number(M8),
  counts: COUNTS,
  sold: SOLD,
  sales: kas(S),
  shares: { F: Number(SH.F), R: Number(SH.R), C: Number(SH.C), D: Number(SH.D), T: SH.T.map(Number), H: Number(sH) },
  amounts: { F: kas(F), R: kas(P_R), C_close: kas(C_CLOSE), C_draw: kas(C_DRAW), B: B.map(kas), H: kas(H_R), b: kas(B_R) },
  vault: {
    V: kas(A0),
    J: kas(J7.J),
    jBase: kas(J7.base),
    G2: kas(J7.g2),
    L: kas(J7.L),
    after: kas(V_PRIME),
    Jnext: kas(J8.J),
    Lnext: kas(J8.L),
    final: kas(A1),
  },
  backers: {
    S0: kas(S0),
    S1: kas(S1),
    price0: price(A0, S0),
    priceDraw: price(V_PRIME, S0),
    price1: price(A1, S1),
    D: kas(D),
    W: kas(W),
    mint: kas(MINT_C),
    slack: kas(SLACK),
    wc: kas(WC),
    out: kas(OUT_C),
    pendingAfter: kas(W - WC),
    requests: REQUESTS.map((r) => ({ kind: r.kind, amount: kas(r.amount) })),
    settled: SETTLED.map((s) => Object.fromEntries(Object.entries(s).map(([key, v]) => [key, typeof v === "bigint" ? kas(v) : v]))),
    drained: rec.dep === 0n && rec.mint === 0n && rec.burn === 0n && rec.out === 0n,
    delayDaa: 600,
  },
  win: WIN,
  bonus: BONUS,
  holders: HOLDERS,
  h: Number(H_J),
  n2: Number(N2),
  prize: kas(J7.J / H_J),
};

if (process.argv[1] && process.argv[1].endsWith("model.mjs")) console.log(JSON.stringify({ ...MODEL, counts: undefined }, null, 1));
