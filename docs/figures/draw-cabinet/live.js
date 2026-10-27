const D = __DATA__;
const P = D.P;
const M = D.model;
const BK = M.backers;
const TL = D.teller;
const stage = document.querySelector(".dc-stage");
const svg = stage.querySelector("svg");
const readout = document.querySelector("[data-readout]");
const status = stage.querySelector("[data-status]");
const still = matchMedia("(prefers-reduced-motion: reduce)");
const q = (sel) => svg.querySelector(sel);
const qa = (sel) => [...svg.querySelectorAll(sel)];

// ---------------------------------------------------------------- timeline (seconds)
const PHASES = [
  { id: "buy", t0: 0, t1: 6, say: "buy(): tickets into 16 shards; backers queue requests at the vault" },
  { id: "close", t0: 6, t1: 9, say: "close(): shards sealed, counts frozen" },
  { id: "draw", t0: 9, t1: 19, say: "draw(): anchor block X's hash picks the numbers; the vault settles, then backer requests convert" },
  { id: "claim", t0: 19, t1: 24.5, say: "claim(): 16 proofs, prize paid" },
  { id: "settle", t0: 24.5, t1: 29, say: "settle(): each backer request takes its share of the record" },
  { id: "next", t0: 29, t1: 31.5, say: "next round" },
];
const LOOP = 31.5;
const REQ_T = [0.7, 2.1, 3.5, 4.9]; // request flights start; each lands 0.55 s later and spends the vault
const FLY = 0.55;
const CLOCK_RUN = 1.3; // 600 DAA, compressed
const DRAW_SPEND = 18.8;
const SETTLE_T = [24.8, 25.8, 26.8, 27.8];

const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
const seg = (t, a, b) => clamp((t - a) / (b - a));
const ease = (x) => x * x * (3 - 2 * x);
const lerp = (a, b, s) => a + (b - a) * s;
const lerp3 = (a, b, s) => [lerp(a[0], b[0], s), lerp(a[1], b[1], s), lerp(a[2], b[2], s)];
const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const depth = (p) => p[0] * D.V[0] + p[1] * D.V[1] + p[2] * D.V[2];
const move = (el, p) => el.setAttribute("transform", translateAlong(P, p));
const show = (el, on) => { const want = on ? "" : "none"; if (el.style.display !== want) el.style.display = want; };
const fmt = (n, d = 2) => n.toLocaleString("en-US", { maximumFractionDigits: d });
const two = (n) => String(n).padStart(2, "0");
const phaseAt = (t) => Math.max(0, PHASES.findIndex((p) => t >= p.t0 && t < p.t1));
const arc = (a, b, s, lift) => add(lerp3(a, b, s), [0, 0, Math.sin(Math.PI * s) * lift]);
const NEXT = (t) => ease(seg(t, 29.2, 30.9)); // 0 during the round, 1 once the next round has opened
const setLit = (el, on) => { if (on) el.setAttribute("data-on", ""); else el.removeAttribute("data-on"); };
const keyed = (el, key, html) => { if (el.dataset.k !== key) { el.dataset.k = key; el.innerHTML = typeof html === "function" ? html() : html; } };

// ---------------------------------------------------------------- art builders (same kit as the build)
const coinStack = (cx, cy, r, z, h, lit = false) => {
  if (h <= 0.05) return "";
  let seams = "";
  for (let s = z + 0.9; s < z + h - 0.3; s += 0.9) seams += sideArc(cx, cy, r, s, P, 24);
  return `<g class="dc-kas">${solidSvg(cylinder(cx, cy, r, z, h, P, 28), { tone: "hi", lit })}${seams ? lineSvg(seams, { tone: "lo" }) : ""}</g>`;
};
const ticketStack = (i, h, lit) => {
  if (h <= 0.05) return "";
  const cx = D.rack.x0 + i * D.rack.pitch + D.rack.pitch / 2;
  const cy = (D.rack.y0 + D.rack.y1) / 2;
  const z = D.rack.z + 2.2;
  let seams = "";
  for (let s = z + 2.3; s < z + h - 0.6; s += 2.3) seams += sideArc(cx, cy, 1.65, s, P, 12);
  return solidSvg(cylinder(cx, cy, 1.65, z, h, P, 16), { tone: "hi", lit }) + (seams ? lineSvg(seams, { tone: "lo" }) : "");
};
const shareCards = (x, y, z, n, lit) => {
  let out = "";
  for (let i = 0; i < n; i++) out += solidSvg(slabOf({ x: x - 3.2, y: y - 2.2, w: 6.4, d: 4.4, r: 0.5 }, z + i * 0.9, 0.7, P, 2), { tone: "hi", lit });
  return out;
};
function leverSvg(L, theta, lit) {
  const o = [D.levers.x, L.y, L.z];
  const d = [0, Math.sin(theta), Math.cos(theta)];
  const F = G.frameOf(o, d, [1, 0, 0], G.cross3(d, [1, 0, 0]));
  const tip = G.add3(o, G.mul3(d, D.levers.len));
  return solidSvg(G.disc(0, 6, 3, F, P, { steps: 24 }), { tone: "mid" }) + solidSvg(G.disc(6, D.levers.len - 4, 1.7, F, P, { steps: 16 }), { tone: "mid", crease: "none" }) + solidSvg(G.sphereOf(tip, 5.6, P, { steps: 32 }), { tone: "hi", lit });
}
function recordSvg(o, coins, cards, lit) {
  if (o < 0.2) return "";
  const R = TL.record;
  const y1 = R.y;
  const out = [];
  out.push(solidSvg(slabOf({ x: R.x0, y: y1, w: R.x1 - R.x0, d: o, r: 0.3 }, R.z0, 0.8, P, 2), { tone: "lo" }));
  out.push(solidSvg(slabOf({ x: R.x0, y: y1, w: 0.8, d: o, r: 0.2 }, R.z0 + 0.8, 3.6, P, 2), { tone: "mid" }));
  if (o > R.depth - 3) {
    out.push(coinStack(R.x0 + 8, y1 + o - 9.5, 3.4, R.z0 + 0.8, coins, lit));
    out.push(shareCards(R.x1 - 8.5, y1 + o - 9.5, R.z0 + 0.8, cards, lit));
  }
  out.push(solidSvg(slabOf({ x: R.x1 - 0.8, y: y1, w: 0.8, d: o, r: 0.2 }, R.z0 + 0.8, 3.6, P, 2), { tone: "mid" }));
  out.push(solidSvg(slabOf({ x: R.x0 - 1, y: y1 + o, w: R.x1 - R.x0 + 2, d: 1.6, r: 0.6 }, R.z0 - 0.6, R.z1 - R.z0 + 1.2, P, 3, 0.4), { tone: "mid" }));
  const cx = (R.x0 + R.x1) / 2;
  out.push(solidSvg(slabOf({ x: cx - 5, y: y1 + o + 1.6, w: 10, d: 1.4, r: 0.6 }, (R.z0 + R.z1) / 2 - 0.8, 1.6, P, 2), { tone: "hi", lit }));
  return out.join("");
}

// ---------------------------------------------------------------- refs
const stacks = qa("[data-stack]");
const trays = qa("[data-tray]");
const lamps = qa("[data-lamp]");
const pins = qa("[data-pin]");
const seal = q('[data-part="seal"]');
const counter = q('[data-part="counter"]');
const coin = q('[data-part="coin"]');
const ticket = q('[data-part="ticket"]');
const card = q('[data-part="claim-card"]');
const callerCoin = q('[data-part="caller-coin"]');
const payoutCoin = q('[data-part="payout-coin"]');
const callerCoins = q('[data-part="caller-coins"]');
const payoutCoins = q('[data-part="payout-coins"]');
const drawer = q('[data-part="drawer"]');
const level = q('[data-part="vault-level"]');
const levelTop = q('[data-part="vault-top"]');
const liability = q('[data-part="vault-l"]');
const needle = q('[data-part="seed-needle"]');
const lever = { close: q('[data-lever="close"]'), draw: q('[data-lever="draw"]') };
const priceNeedle = q('[data-part="price-needle"]');
const clockHand = q('[data-part="clock-hand"]');
const ready = q('[data-part="ready"]');
const hopper = q('[data-part="hopper"]');
const cards = qa("[data-card]");
const record = q('[data-part="record"]');
const backerItems = q('[data-part="backer-items"]');
const movers = q('[data-part="movers"]');
const mk = (html) => { const g = document.createElementNS("http://www.w3.org/2000/svg", "g"); g.innerHTML = html; g.style.display = "none"; movers.appendChild(g); return g; };
const reqCoin = mk(coin.innerHTML);
const reqCard = mk(solidSvg(slabOf({ x: -3, y: -0.4, w: 6, d: 0.8, r: 0.3 }, 0, 9, P, 2), { tone: "hi", lit: true }));
const setCard = mk(shareCards(0, 0, 0, 1, true));
const setCoin = mk(coin.innerHTML);

function ballGroups(host, spec, count, prefix) {
  const out = [];
  for (let i = 0; i < count; i++) {
    const g = document.createElementNS("http://www.w3.org/2000/svg", "g");
    g.innerHTML = spec.art;
    g.dataset.ball = prefix + (i + 1);
    host.appendChild(g);
    out.push(g);
  }
  return out;
}
const mainHost = q('[data-part="globe-balls"]');
const bonusHost = q('[data-part="bonus-balls"]');
const mainBalls = ballGroups(mainHost, D.globe, 30, "m");
const bonusBalls = ballGroups(bonusHost, D.bonus, M.m, "b");
const railHost = q('[data-part="rail-balls"]');
const railBalls = [...M.win, M.bonus].map((n, j) => {
  const g = document.createElementNS("http://www.w3.org/2000/svg", "g");
  const spec = j < 5 ? D.globe : D.bonus;
  const rs = spec.ballR * D.KS;
  g.innerHTML = spec.plain + `<circle class="dc-numdisc" cx="${D.origin[0].toFixed(2)}" cy="${(D.origin[1] - rs * 0.12).toFixed(2)}" r="${(rs * 0.62).toFixed(2)}"/><text class="dc-num" x="${D.origin[0].toFixed(2)}" y="${(D.origin[1] - rs * 0.1).toFixed(2)}" text-anchor="middle" font-size="${(rs * 0.78).toFixed(2)}">${two(n)}</text>`;
  g.setAttribute("data-lit-ball", "");
  railHost.appendChild(g);
  return g;
});

// ---------------------------------------------------------------- the round as a pure function of time
const EXIT = M.win.map((_, j) => 11.0 + j * 0.85);
const EXIT_BONUS = 15.4;
const SEED_ANGLE = 2.35;
const LEVER_REST = -0.32;
const LEVER_PULL = 0.95;
const leverAt = (t, t0) => LEVER_REST + (LEVER_PULL - LEVER_REST) * (ease(seg(t, t0, t0 + 0.45)) - ease(seg(t, t0 + 0.6, t0 + 1.3)));

function swirl(spec, i, t) {
  const inner = spec.r - 1.4 - spec.ballR;
  const fr = (i * 0.618) % 1;
  const rad = inner * (0.15 + 0.75 * fr);
  const a = i * 2.399 + t * (2.1 + (i % 5) * 0.37);
  const zz = Math.sqrt(Math.max(0, inner * inner - rad * rad)) * 0.92 * Math.sin(t * 1.7 + i * 1.3);
  return [spec.c[0] + rad * Math.cos(a), spec.c[1] + rad * Math.sin(a), spec.c[2] + zz];
}
function ballState(spec, i, t, exitAt) {
  const mix = ease(seg(t, 9.7, 10.3)) * (1 - ease(seg(t, 16.0, 16.8)));
  let p = lerp3(spec.rest[i], swirl(spec, i, t), mix);
  let on = true;
  let fade = 1;
  if (exitAt !== null) {
    if (t >= exitAt && t < 29) {
      const portIn = add(spec.c, G.mul3(G.unit3(G.sub3(spec.port, spec.c)), spec.r - spec.ballR - 1));
      p = lerp3(p, portIn, ease(seg(t, exitAt, exitAt + 0.3)));
      on = t < exitAt + 0.32;
    }
    if (t >= 29) { fade = ease(seg(t, 30.1, 30.9)); p = spec.rest[i]; }
  }
  return { p, on, fade };
}
function railState(j, t) {
  const exitAt = j < 5 ? EXIT[j] : EXIT_BONUS;
  const entry = [j < 5 ? D.rail.entry : D.rail.bonusSlot + 6, D.rail.y, D.rail.z];
  const slot = [j < 5 ? D.rail.slots[j] : D.rail.bonusSlot, D.rail.y, D.rail.z];
  const t0 = exitAt + 0.38;
  if (t < t0) return { on: false };
  const drop = [entry[0], entry[1], D.rail.gateZ];
  const p = t < t0 + 0.2 ? lerp3(drop, entry, seg(t, t0, t0 + 0.2) ** 2) : lerp3(entry, slot, ease(seg(t, t0 + 0.2, t0 + 0.75)));
  const fade = 1 - ease(seg(t, 29.2, 30.0));
  return { on: fade > 0.01, p, fade };
}

// backers: which requests have landed, the clock since the last vault spend
// tq is the backers' own clock: equal to t here, separate in the tour so their requests get their own stop
const landed = (t, tq, i) => tq >= REQ_T[i] + FLY && t < 29.2;
const lastSpend = (t) => {
  let s = -100;
  REQ_T.forEach((r) => { if (t >= r + FLY) s = r + FLY; });
  if (t >= DRAW_SPEND) s = DRAW_SPEND;
  return s;
};
const DEP_SCALE = 0.15 / 1000; // hopper height per KAS
const WREQ = BK.requests.map((r, i) => ({ ...r, i })).filter((r) => r.kind === "withdraw");
const W_MAX = Math.max(...WREQ.map((r) => r.amount));
const cardH = (amount) => 3 + 11 * (amount / W_MAX);

const TRAY_FULL = [14, 6, 3.2, 2.4, 2.4];
function render(t, tq = t) {
  const next = NEXT(t);
  const ph = phaseAt(t);

  // buy(): coins into the terminal, tickets into the rack
  const fBuy = ease(seg(t, 0.3, 5.6)) * (1 - next);
  const sold = Math.round(M.sold * fBuy);
  counter.textContent = String(sold).padStart(6, "0");
  const sealed = t >= 7.25 && t < 29.2;
  stacks.forEach((g, i) => {
    const h = M.counts[i] * D.rack.unit * fBuy;
    keyed(g, h.toFixed(2) + (ph === 0 ? "a" : ""), () => ticketStack(i, h, ph === 0));
  });
  let coinOn = false;
  let ticketOn = false;
  for (let j = 0; j < 6; j++) {
    const tj = 0.3 + j * 0.88;
    if (t >= tj && t < tj + 0.45) { coinOn = true; const s = seg(t, tj, tj + 0.45); move(coin, add(D.term.slot, [0, 0, 26 * (1 - s * s)])); }
    if (t >= tj + 0.4 && t < tj + 0.95) {
      ticketOn = true;
      const shard = (j * 5 + 3) % 16;
      const dst = [D.rack.x0 + shard * D.rack.pitch + D.rack.pitch / 2, (D.rack.y0 + D.rack.y1) / 2, D.rack.top + 2];
      move(ticket, arc(D.term.mouth, dst, ease(seg(t, tj + 0.4, tj + 0.95)), 22));
    }
  }
  show(coin, coinOn);
  show(ticket, ticketOn);

  // backers queue requests: each spends the vault, so the next waits for the clock
  let rc = false;
  let rk = false;
  BK.requests.forEach((r, i) => {
    const t0 = REQ_T[i];
    if (tq < t0 || tq >= t0 + FLY) return;
    const s = ease(seg(tq, t0, t0 + FLY));
    if (r.kind === "deposit") { rc = true; move(reqCoin, arc(TL.from, [TL.hopper.c[0], TL.hopper.c[1], TL.hopper.z1 + 5], s, 16)); }
    else { rk = true; const slot = WREQ.findIndex((w) => w.i === i); move(reqCard, arc(TL.from, [TL.cards.x[slot], TL.cards.y + 0.4, TL.cards.z + 1], s, 10)); }
  });
  show(reqCoin, rc);
  show(reqCard, rk);
  const run = t >= DRAW_SPEND ? clamp((t - DRAW_SPEND) / CLOCK_RUN) : clamp((tq - lastSpend(tq)) / CLOCK_RUN);
  const ha = run * Math.PI * 2;
  clockHand.setAttribute("d", `M0 0L${(Math.sin(ha) * (TL.clock.r - 1.6)).toFixed(2)} ${(-Math.cos(ha) * (TL.clock.r - 1.6)).toFixed(2)}`);
  setLit(ready, run >= 1);
  // hopper: deposits wait here until draw() converts them, then drop into the vault
  const depIn = BK.requests.reduce((a, r, i) => a + (r.kind === "deposit" && landed(t, tq, i) ? r.amount : 0), 0);
  const drain = ease(seg(t, 17.6, 18.0));
  const hh = depIn * DEP_SCALE * (1 - drain);
  keyed(hopper, hh.toFixed(2) + (ph === 0), () => coinStack(TL.hopper.c[0], TL.hopper.c[1], TL.hopper.r - 1.2, TL.hopper.z0 + 1.6, hh, ph === 0));
  // cards: withdrawal requests; draw() burns the converted shares, the rest stays as a stub
  const burn = ease(seg(t, 18.2, 18.7));
  cards.forEach((g, slot) => {
    const r = WREQ[slot];
    const st = BK.settled[r.i];
    const on = landed(t, tq, r.i) || (t >= 29.2 && t < 30.4);
    const full = cardH(r.amount);
    const stub = Math.max(0.9, full * (st.left / r.amount));
    const h = on ? (t >= 29.2 ? stub * (1 - seg(t, 29.6, 30.4)) : lerp(full, stub, burn)) : 0;
    const x = TL.cards.x[slot];
    const lit = ph === 0 || (burn >= 1 && t < 29.2);
    keyed(g, h.toFixed(2) + lit, () => (h > 0.05 ? solidSvg(slabOf({ x: x - 3, y: TL.cards.y, w: 6, d: 0.8, r: 0.3 }, TL.cards.z, h, P, 2), { tone: "hi", lit }) : ""));
  });

  // close(): lever, seal bar, caller reward
  lever.close.innerHTML = leverSvg(D.levers.close, leverAt(t, 6.2), ph === 1);
  const sealZ = lerp(D.seal.up, D.seal.down, ease(seg(t, 6.55, 7.25)) * (1 - ease(seg(t, 29.2, 30.1))));
  move(seal, [0, 0, sealZ]);
  setLit(seal, sealed);

  // draw(): lever, seed dial, globes, rail
  lever.draw.innerHTML = leverSvg(D.levers.draw, leverAt(t, 9.15), ph === 2);
  const seedA = SEED_ANGLE * ease(seg(t, 9.4, 10.2)) * (1 - next);
  needle.setAttribute("d", `M0 0L${(Math.sin(seedA) * 4.4).toFixed(2)} ${(-Math.cos(seedA) * 4.4).toFixed(2)}`);
  placeBalls(mainHost, mainBalls, D.globe, t, (i) => { const j = M.win.indexOf(i + 1); return j >= 0 ? EXIT[j] : null; });
  placeBalls(bonusHost, bonusBalls, D.bonus, t, (i) => (i + 1 === M.bonus ? EXIT_BONUS : null));
  railBalls.forEach((g, j) => {
    const s = railState(j, t);
    show(g, s.on);
    if (s.on) { move(g, s.p); g.setAttribute("opacity", s.fade.toFixed(3)); }
  });

  // caller rewards: one coin at close(), one at draw()
  let flying = false;
  for (const [t0, from] of [[6.9, [D.levers.x + 2, D.levers.close.y, D.levers.close.z - 8]], [9.8, [D.levers.x + 2, D.levers.draw.y, D.levers.draw.z - 8]]]) {
    if (t >= t0 && t < t0 + 0.7) { flying = true; move(callerCoin, arc(from, D.cups.caller, ease(seg(t, t0, t0 + 0.7)), 14)); }
  }
  show(callerCoin, flying);
  const inCup = t >= 29.2 ? 0 : (t >= 7.6 ? 1 : 0) + (t >= 10.5 ? 1 : 0);
  keyed(callerCoins, String(inCup), () => coinStack(D.cups.caller[0], D.cups.caller[1], 3.1, D.cups.caller[2], 0.9 * inCup));

  // vault: prizes out (16.4), deposits in (17.6), capped withdrawals out to the record (18.2)
  const pay = ease(seg(t, 16.4, 17.3));
  const depC = ease(seg(t, 17.6, 18.0));
  const outC = ease(seg(t, 18.2, 18.7));
  const V = lerp(M.vault.V + (M.vault.after - M.vault.V) * pay + BK.D * depC - BK.out * outC, M.vault.V, next);
  const gh = D.sight.h;
  const lv = (gh * V) / 1e7;
  level.setAttribute("y", (gh - lv).toFixed(2));
  level.setAttribute("height", lv.toFixed(2));
  levelTop.setAttribute("d", `M0 ${(gh - lv).toFixed(2)}H${D.sight.w}`);
  const ly = (gh - (gh * M.vault.L) / 1e7).toFixed(2);
  liability.setAttribute("d", `M0 ${ly}H${D.sight.w}`);
  // share price: moves once, after the round's own settlement
  const price = lerp(lerp(BK.price0, BK.priceDraw, ease(seg(t, 17.2, 17.6))), BK.price0, next);
  const pa = -0.75 * Math.PI + ((price - TL.range[0]) / (TL.range[1] - TL.range[0])) * 1.5 * Math.PI;
  priceNeedle.setAttribute("d", `M0 0L${(Math.sin(pa) * (TL.gauge.r - 2.6)).toFixed(2)} ${(-Math.cos(pa) * (TL.gauge.r - 2.6)).toFixed(2)}`);
  // settlement record drawer: opens as draw() writes it, empties one settle() at a time, closes when drained
  const open = (ease(seg(t, 18.2, 18.6)) - ease(seg(t, 28.4, 28.9))) * (1 - next);
  const fillR = ease(seg(t, 18.5, 18.8));
  const coinsLeft = BK.requests.filter((r, i) => r.kind === "withdraw" && t < SETTLE_T[i] + 0.15).length;
  const cardsLeft = BK.requests.filter((r, i) => r.kind === "deposit" && t < SETTLE_T[i] + 0.15).length;
  const recCoins = fillR * coinsLeft * 2.2;
  const recCards = fillR > 0.5 ? cardsLeft : 0;
  keyed(record, open.toFixed(3) + "|" + recCoins.toFixed(2) + "|" + recCards + (ph === 4), () => recordSvg(open * TL.record.depth, recCoins, recCards, ph === 2 || ph === 4));
  let sc = false;
  let sk = false;
  SETTLE_T.forEach((s0, i) => {
    if (t < s0 || t >= s0 + 0.6) return;
    const isDep = BK.requests[i].kind === "deposit";
    const from = [isDep ? TL.record.x1 - 8.5 : TL.record.x0 + 8, TL.record.y + TL.record.depth - 9.5, TL.record.z0 + 4];
    const p = arc(from, TL.cup, ease(seg(t, s0, s0 + 0.6)), 12);
    if (isDep) { sk = true; move(setCard, p); } else { sc = true; move(setCoin, p); }
  });
  show(setCard, sk);
  show(setCoin, sc);
  const got = SETTLE_T.map((s0, i) => (t >= s0 + 0.6 && t < 29.2 ? BK.requests[i].kind : null)).filter(Boolean);
  const gotCards = got.filter((x) => x === "deposit").length;
  const gotCoins = got.filter((x) => x === "withdraw").length;
  keyed(backerItems, gotCards + "|" + gotCoins, () => coinStack(TL.cup[0] - 2, TL.cup[1] + 1.4, 3.1, TL.cup[2], 0.9 * gotCoins) + shareCards(TL.cup[0] + 2, TL.cup[1] - 1.6, TL.cup[2], gotCards, false));

  // prize trays: the vault pays the jackpot and two second prizes, sales fill the lower tiers
  const fill = [ease(seg(t, 16.4, 17.1)), ease(seg(t, 16.6, 17.3)), ease(seg(t, 16.8, 17.4)), ease(seg(t, 16.9, 17.5)), ease(seg(t, 17.0, 17.6))].map((f) => f * (1 - next));
  const split = ease(seg(t, 22.0, 22.5));
  const gone = ease(seg(t, 22.35, 22.75));
  const paid = t >= 22.6;
  trays.forEach((g, i) => {
    const tr = D.trays[i];
    const cx = tr.x + tr.w / 2;
    const cy = tr.y + tr.d / 2;
    const z = tr.z + 0.8;
    let key = fill[i].toFixed(3);
    let html;
    if (i === 0) {
      const h = TRAY_FULL[0] * fill[0];
      const half = h / 2;
      key += split.toFixed(3) + gone.toFixed(3) + paid;
      html = () => coinStack(cx, cy, 4.6, z, paid ? half : h - half * split, ph === 2 && fill[0] > 0 && fill[0] < 1) + (split > 0 && gone < 1 ? `<g opacity="${(1 - gone).toFixed(3)}">${coinStack(cx, cy, 4.6, z + h - half + split * 5, half, true)}</g>` : "");
    } else if (i === 1) html = () => coinStack(cx - 3.4, cy - 2.6, 3.4, z, TRAY_FULL[1] * fill[1]) + coinStack(cx + 2.8, cy + 3, 3.4, z, TRAY_FULL[1] * fill[1]);
    else html = () => coinStack(cx, cy, 3.6, z, TRAY_FULL[i] * fill[i]);
    keyed(g, key, html);
  });
  lamps.forEach((el, i) => setLit(el, fill[i] > 0.98));
  const openB = ease(seg(t, 16.5, 16.9)) - ease(seg(t, 17.3, 17.8));
  drawer.setAttribute("transform", translateAlong(P, [0, 9 * openB, 0]));

  // claim(): card into the reader, 16 proof pins, the slice cut by h and paid out of the spout
  show(card, t >= 19.2 && t < 20.0);
  if (t >= 19.2 && t < 20.0) move(card, add(D.reader.slot, [0, 22 * (1 - ease(seg(t, 19.2, 19.9))) + 7, 0]));
  pins.forEach((el, i) => {
    const t0 = 20.0 + i * 0.12;
    const held = M.holders.includes(i);
    const up = ease(seg(t, t0, t0 + 0.15)) * (1 - ease(seg(t, 23.9, 24.3)));
    el.setAttribute("transform", translateAlong(P, [0, 0, up * (held ? 3.6 : 1.8)]));
    setLit(el, held && up > 0.5);
  });
  const flyOn = t >= 22.7 && t < 23.3;
  show(payoutCoin, flyOn);
  if (flyOn) move(payoutCoin, arc(D.spout, D.cups.payout, seg(t, 22.7, 23.3) ** 1.6, 4));
  const payKey = t >= 23.3 && t < 29.2 ? "1" + (ph === 3) : "0";
  keyed(payoutCoins, payKey, () => (payKey !== "0" ? coinStack(D.cups.payout[0], D.cups.payout[1], 4.6, D.cups.payout[2], 3.4, ph === 3) : ""));

  say(t, sold, tq);
}

const K_ANGLE = (i, t) => i * 47 + 160 * Math.sin(t * 1.3 + i) * ease(seg(t, 9.7, 10.3)) * (1 - ease(seg(t, 16.0, 16.8)));
function placeBalls(host, groups, spec, t, exitOf) {
  const order = [];
  groups.forEach((g, i) => {
    const s = ballState(spec, i, t, exitOf(i));
    const kg = g.firstElementChild && g.querySelector("[data-k]");
    if (kg) kg.setAttribute("transform", `rotate(${K_ANGLE(i, t).toFixed(1)} ${D.origin[0].toFixed(2)} ${D.origin[1].toFixed(2)})`);
    show(g, s.on);
    move(g, s.p);
    if (s.fade < 1) g.setAttribute("opacity", s.fade.toFixed(3)); else g.removeAttribute("opacity");
    order.push([depth(s.p), g]);
  });
  order.sort((a, b) => a[0] - b[0]);
  const now = [...host.children];
  if (order.some(([, g], i) => now[i] !== g)) {
    const frag = document.createDocumentFragment();
    order.forEach(([, g]) => frag.appendChild(g));
    host.appendChild(frag);
  }
}

// ---------------------------------------------------------------- readout and announcements
let lastPhase = -1;
function requestText(i) {
  const r = BK.requests[i];
  return r.kind === "deposit" ? `request_deposit() · ${fmt(r.amount)} KAS waits for the next draw` : `request_withdraw() · ${fmt(r.amount)} shares, owner-signed`;
}
function settleText(i) {
  const s = BK.settled[i];
  if (s.kind === "deposit") return `settle() · ${fmt(s.amount)} KAS → ${fmt(s.shares)} shares`;
  return `settle() · ${fmt(s.conv)} shares → ${fmt(s.pay)} KAS · ${fmt(s.left)} stay pending`;
}
function say(t, sold, tq = t) {
  const drawn = M.win.filter((_, j) => t >= EXIT[j] + 1.1).map(two);
  const bonusDrawn = t >= EXIT_BONUS + 1.1;
  const req = REQ_T.findIndex((r) => tq >= r && tq < r + 1.3);
  const set = SETTLE_T.findIndex((s) => t >= s && t < s + 1.0);
  let text = "";
  if (t < 6) text = req >= 0 ? requestText(req) : `round ${M.round} · buy() · ${sold} tickets · ${fmt(sold * M.price)} KAS in ${M.k} shards`;
  else if (t < 7.25) text = "close() · deadline passed · lever pulled";
  else if (t < 9) text = `close() · ${M.k} shards sealed · caller +${fmt(M.amounts.C_close)} KAS`;
  else if (t < 10.4) text = "draw() · anchor X · mixing";
  else if (t < 16.4) text = `draw() · ${drawn.length ? drawn.join(" ") : "…"}${bonusDrawn ? " + " + two(M.bonus) : ""}`;
  else if (t < 17.2) text = `draw() · h = ${M.h} · n2 = ${M.n2} · vault pays ${fmt(M.vault.J, 0)} + ${M.n2} × ${fmt(M.vault.G2, 0)} KAS`;
  else if (t < 18.2) text = `draw() · share price ${BK.price0.toFixed(4)} → ${BK.priceDraw.toFixed(4)} · deposits in`;
  else if (t < 19) text = `draw() · withdrawals capped: vault keeps L = ${fmt(M.vault.Lnext, 0)} KAS`;
  else if (t < 20.0) text = `claim() · ticket ${M.win.map(two).join(" ")} + ${two(M.bonus)}`;
  else if (t < 22.0) text = `claim() · proof ${Math.min(16, Math.floor((t - 20.0) / 0.12) + 1)} / 16 · h = ${M.holders.filter((i) => t >= 20.0 + i * 0.12 + 0.15).length}`;
  else if (t < 24.5) text = `claim() · floor(${fmt(M.vault.J)} / ${M.h}) = ${fmt(M.prize)} KAS paid`;
  else if (t < 29) text = set >= 0 ? settleText(set) : t < SETTLE_T[0] ? "settle() · anyone may settle a request" : `record drained · R refunded · ${fmt(BK.pendingAfter, 0)} shares wait for round ${M.round + 1}`;
  else text = `round ${M.round + 1} opens · m = ${M.mNext} · J = ${fmt(M.vault.Jnext, 0)} KAS`;
  if (readout.textContent !== text) readout.textContent = text;
  const ph = phaseAt(t);
  if (ph !== lastPhase) {
    lastPhase = ph;
    if (!D.tour && document.activeElement === stage) status.textContent = PHASES[ph].say;
  }
}

// ---------------------------------------------------------------- loop, tour, input
// the tour page (build --tour) drives render() from tour.js instead
if (!D.tour) {
  let clock = 0;
  let frame = 0;
  let last = 0;
  let visible = false;
  function tick(now) {
    const dt = last ? Math.min((now - last) / 1000, 1 / 30) : 1 / 60;
    last = now;
    if (!still.matches) clock = (clock + dt) % LOOP;
    render(clock);
    frame = visible && !still.matches ? requestAnimationFrame(tick) : 0;
  }
  const runLoop = () => { if (!frame && visible) { last = 0; frame = requestAnimationFrame(tick); } };
  function seek(step) {
    const ph = phaseAt(clock);
    const to = (ph + step + PHASES.length) % PHASES.length;
    clock = still.matches ? PHASES[to].t1 - 0.05 : PHASES[to].t0;
    status.textContent = PHASES[to].say;
    render(clock);
    runLoop();
  }
  new IntersectionObserver((entries) => { visible = entries[entries.length - 1].isIntersecting; if (visible) runLoop(); }, { rootMargin: "120px 0px" }).observe(stage);
  stage.addEventListener("click", () => seek(1));
  stage.addEventListener("keydown", (event) => {
    if (event.key === "ArrowRight" || event.key === " " || event.key === "Enter") { event.preventDefault(); seek(1); }
    else if (event.key === "ArrowLeft") { event.preventDefault(); seek(-1); }
  });
  stage.addEventListener("focus", () => { status.textContent = PHASES[phaseAt(clock)].say; });
  if (still.matches) clock = PHASES[0].t1 - 0.05;
  render(clock);
  window.__dc = { render: (t) => { clock = t; render(t); }, phases: PHASES };
}
