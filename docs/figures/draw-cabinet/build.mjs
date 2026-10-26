// Draw cabinet: the Kaspa L1 lottery as a televised draw machine.
// node build.mjs [--light]  → draw-cabinet.html / draw-cabinet-light.html
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as k from "../../../.claude/skills/anatomy/kit/iso-kit.mjs";
import * as G from "../../../.claude/skills/anatomy/kit/lathe.mjs";
import { tubePieces, tubeSvg } from "../../../.claude/skills/anatomy/kit/tube.mjs";
import { kitScript } from "../../../.claude/skills/anatomy/scripts/inline-kit.mjs";
import { MODEL } from "./model.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const KIT = join(HERE, "..", "..", "..", ".claude", "skills", "anatomy", "kit");
const THEME = process.argv.includes("--light") ? "light" : "dark";
const TOUR = process.argv.includes("--tour");

const W = 760;
const H = 590;

// ---------------------------------------------------------------- world plan
const PLATE = { x: -182, y: -88, w: 364, d: 186, r: 10 };
const BODY = { x: -140, y: -60, w: 280, d: 120, r: 4 };
const KICK = { x: -134, y: -54, w: 268, d: 108, r: 3 };
const DECK = { x: -143, y: -63, w: 286, d: 126, r: 5 };
const Z0 = 8; // body bottom
const ZB = 100; // body top
const ZD = 104; // deck top
const FY = 60; // front face
const RX = 140; // right face

const GLOBE = { c: [0, -6, 0], r: 40, cup: 16, cupZ: 125 };
GLOBE.c[2] = GLOBE.cupZ + Math.sqrt(GLOBE.r ** 2 - GLOBE.cup ** 2);
const BONUS = { c: [98, -12, 0], r: 21, cup: 10, cupZ: 120 };
BONUS.c[2] = BONUS.cupZ + Math.sqrt(BONUS.r ** 2 - BONUS.cup ** 2);
const BALL_R = 4.4;
const BBALL_R = 3.7;

const RAIL = { x0: -38, x1: 88, y: 44, z: ZD + 3 };
const SLOTS = [-28, -17.6, -7.2, 3.2, 13.6];
const ENTRY = 26;
const BONUS_SLOT = 76;
const DIVIDER = 50;

const RACK = { x0: -131, pitch: 5.1, y0: -54, y1: -32, z: ZD + 4, top: ZD + 44 };
const STACK_UNIT = 1.15; // world units per ticket in a rack slot

const TERM = { x: -134, y: 6, w: 44, d: 46, z: ZD, h: 24 };
const LEDGE = { x: -134, y: FY, w: 126, d: 22, z: 52 };
const TRAYS = Array.from({ length: 5 }, (_, i) => ({ x: -131 + i * 17.8, y: 63, w: 15.6, d: 16.5, z: LEDGE.z + 4, h: 7 }));
const READER = { x: -38, y: 62, w: 28, d: 17, z: LEDGE.z + 4, h: 9 };
const DOOR = { x0: 20, x1: 132, z0: 14, z1: 94 };
const SIGHT = { x0: 108, x1: 124, z0: 24, z1: 86 };
const LEVERS = { close: { y: -24, z: 76 }, draw: { y: 22, z: 76 } };
const LEVER_LEN = 64;
const CALLER_CUP = [160, -2];
const PAYOUT_CUP = [2, 82];

const AZ = 58;
const EL = 30;
const P = k.fitProjection(
  [
    ...k.boxCorners(PLATE, -15, 0),
    [GLOBE.c[0], GLOBE.c[1], GLOBE.c[2] + GLOBE.r + 10],
    [GLOBE.c[0] - GLOBE.r, GLOBE.c[1], GLOBE.c[2]],
    [RX + 10, 60, ZD + 62],
    [RX + 10, -60, ZD + 62],
    [-140, -60, RACK.top + 22],
  ],
  W,
  H,
  { pad: 26, azimuth: AZ, elevation: EL },
);
const V = G.viewOf(P);
const KS = G.scaleOf(P);
const at = (p) => k.iso(p, P);
const f2 = (v) => Math.round(v * 100) / 100;
const S = (paths, style) => k.solidSvg(paths, style);
const Ln = (d, style) => k.lineSvg(d, style);
const Dots = (points, style) => k.dotsSvg(points, style);
const path3 = (points, closed = false) => k.pathOf(points.map(at), closed);
const depth = (p) => G.dot3(p, V);

// A matrix that lays flat 2D drawing (x right, y down) onto a world plane, oriented to read left to right on screen.
function planeMatrix(o, ex, ey) {
  const p0 = at(o);
  const px = at(G.add3(o, ex));
  const py = at(G.add3(o, ey));
  const v = (x) => x.toFixed(4);
  return `matrix(${v(px[0] - p0[0])} ${v(px[1] - p0[1])} ${v(py[0] - p0[0])} ${v(py[1] - p0[1])} ${p0[0].toFixed(2)} ${p0[1].toFixed(2)})`;
}
const rightward = (a) => (at(a)[0] - at([0, 0, 0])[0] >= 0 ? a : G.mul3(a, -1));
const FRONT_EX = rightward([1, 0, 0]);
const SIDE_EX = rightward([0, -1, 0]);
const DOWN = [0, 0, -1];
const TOP_DOWN = at([0, 1, 0])[1] - at([0, 0, 0])[1] >= 0 ? [0, 1, 0] : [0, -1, 0];

// The Kaspa K mark: the white polygon of the official logo (refs/rusty-kaspa-v2.1.0/bridge/static/assets/kaspa.svg),
// centred on its teal disc (centre 94.4, 66.58, radius 40). kMark draws it in flat 2D, `radius` = the disc it sits on.
const K_POINTS = [[106.18, 44.79], [98.08, 45.99], [100.4, 61.66], [83.44, 48.61], [78.45, 55.12], [93.33, 66.58], [78.45, 78.03], [83.44, 84.55], [100.4, 71.49], [98.08, 87.16], [106.18, 88.36], [109.4, 66.58]];
const kMark = (cx, cy, radius) => {
  const s = radius / 40;
  return `<polygon class="dc-k" points="${K_POINTS.map(([x, y]) => `${f2(cx + (x - 94.4) * s)},${f2(cy + (y - 66.58) * s)}`).join(" ")}"/>`;
};

// ---------------------------------------------------------------- painter list
const items = [];
let capture = null;
function put(ref, svg, bias = 0, name = "") {
  if (capture) capture.push(svg);
  else items.push({ key: depth(ref) + bias, svg, name });
}
// Parts mounted on one face are drawn as one item, in the order they were put (host first).
function assembly(ref, bias, name, build) {
  capture = [];
  build();
  const svg = capture.join("");
  capture = null;
  put(ref, svg, bias, name);
}
const box = (x0, y0, z0, x1, y1, z1, style = {}, { r = 0.5, steps = 2, bevel = 0, inner = "" } = {}) =>
  S(k.slabOf({ x: x0, y: y0, w: x1 - x0, d: y1 - y0, r }, z0, z1 - z0, P, steps, bevel), { tone: "mid", ...style, inner });
const cyl = (x, y, r, z, h, style = {}, steps = 28, bevel = 0) => S(k.cylinder(x, y, r, z, h, P, steps, bevel), { tone: "mid", ...style });
const along = (o, a) => {
  const ref = Math.abs(a[2]) > 0.9 ? [1, 0, 0] : [0, 0, 1];
  const u = G.unit3(G.cross3(ref, a));
  return G.frameOf(o, a, u, G.cross3(a, u));
};
const disc = (F, s0, s1, r, style = {}, opts = {}) => S(G.disc(s0, s1, r, F, P, opts), { tone: "mid", ...style });
const screws = (points, size = 0.45, tone = "mid") => Dots(points.map(at), { size, tone });

// Visible front band of a vertical cylinder between z0 and z1, for lips that sit in front of something resting inside.
function frontBand(cx, cy, r, z0, z1, tone = "mid") {
  const n = 64;
  const pts = Array.from({ length: n + 1 }, (_, i) => (i / n) * Math.PI * 2);
  const front = pts.filter((a) => Math.cos(a) * V[0] + Math.sin(a) * V[1] > 0);
  const lo = (a) => at([cx + r * Math.cos(a), cy + r * Math.sin(a), z0]);
  const hi = (a) => at([cx + r * Math.cos(a), cy + r * Math.sin(a), z1]);
  const centreA = Math.atan2(V[1], V[0]);
  const arc = Array.from({ length: 41 }, (_, i) => centreA - Math.PI / 2 + (i / 40) * Math.PI);
  const poly = [...arc.map(hi), ...arc.slice().reverse().map(lo)];
  const d = k.pathOf(poly, true);
  const edge = k.pathOf(arc.map(hi)) + k.pathOf(arc.map(lo)) + k.pathOf([hi(arc[0]), lo(arc[0])]) + k.pathOf([hi(arc[40]), lo(arc[40])]);
  void front;
  return `<g class="iso-solid"><path class="iso-fill" d="${d}"/><path class="iso-shade" data-shade="2" d="${d}"/><path class="iso-line iso-edge" data-tone="${tone}" d="${edge}"/></g>`;
}

// Split a horizontal ring (a band round a globe) into the half behind the centre and the half in front.
function ringHalves(c, r, z, n = 96) {
  const pts = Array.from({ length: n + 1 }, (_, i) => {
    const a = (i / n) * Math.PI * 2;
    return [c[0] + r * Math.cos(a), c[1] + r * Math.sin(a), z];
  });
  const back = [];
  const front = [];
  let run = null;
  let runFront = null;
  for (const p of pts) {
    const f = depth(p) - depth([c[0], c[1], z]) >= 0;
    if (f !== runFront) {
      if (run) (runFront ? front : back).push(run);
      run = run ? [run[run.length - 1]] : [];
      runFront = f;
    }
    run.push(p);
  }
  if (run) (runFront ? front : back).push(run);
  return { back: back.map((r2) => path3(r2)).join(""), front: front.map((r2) => path3(r2)).join("") };
}

// ---------------------------------------------------------------- bench plate
const back = [];
back.push(`<path class="iso-halo" d="${k.haloOf(PLATE, -15, 10, P)}"/>`);
const byDepth = (pts) => [...pts].sort((a, b) => depth([a[0], a[1], 0]) - depth([b[0], b[1], 0]));
for (const [x, y] of byDepth(k.corners(PLATE, 24))) {
  back.push(cyl(x, y, 7.5, -15, 1.6, { tone: "lo" }));
  back.push(cyl(x, y, 5.6, -13.4, 3.4, { tone: "mid" }));
}
back.push(S(k.slabOf(PLATE, -10, 10, P, 8, 1.6), { tone: "mid", inner: Ln(k.sideSeam(PLATE, -4, P, 8), { tone: "faint" }) }));
back.push(Ln(k.planOutline(k.insetPlan(PLATE, 5), 0, P), { tone: "lo" }));
const plateScrews = k.corners(PLATE, 11);
back.push(Ln(plateScrews.map(([x, y]) => k.ring(x, y, 2.2, 0, P, 16)).join(""), { tone: "lo" }));
back.push(screws(plateScrews.map(([x, y]) => [x, y, 0]), 0.5));
{
  const strip = { x: PLATE.x + 16, y: PLATE.y + PLATE.d - 12, w: 150, d: 6, r: 1 };
  back.push(S(k.slabOf(strip, 0, 0.6, P, 3), { tone: "lo", crease: "none" }));
  const t = k.topTicks(strip.x + 4, strip.x + strip.w - 4, 4, 5, strip.y, 0.6, [1.8, 3.4], P);
  back.push(Ln(t.minor, { tone: "lo" }), Ln(t.major, { tone: "mid" }));
  const label = { x: 104, y: PLATE.y + PLATE.d - 28, w: 40, d: 15, r: 1.2 };
  back.push(S(k.slabOf(label, 0, 0.8, P, 3), { tone: "lo", crease: "none" }));
  back.push(Ln(k.planOutline(k.insetPlan(label, 1.4), 0.8, P, 3), { tone: "faint" }));
  back.push(Ln([26, 18, 22].map((w, i) => k.lineOnTop([label.x + 4, label.y + 4 + i * 3.2], [label.x + 4 + w, label.y + 4 + i * 3.2], 0.8, P)).join(""), { tone: "lo", free: true }));
  back.push(screws([[label.x + label.w - 3, label.y + 2.6, 0.8], [label.x + label.w - 3, label.y + label.d - 2.6, 0.8]]));
}
// cups on the bench: caller's reward cup and the winner's payout cup
function cup(cx, cy, r, h, name) {
  const F = G.frameOf([cx, cy, 0], [0, 0, 1], [1, 0, 0], [0, 1, 0]);
  const body = S(G.lathe([[0, 0], [0, r - 1], [h, r], [h, r - 1.1], [1.2, r - 2]], F, P, { steps: 56 }), { tone: "lo" });
  return `<path class="iso-halo" d="${k.haloOf({ x: cx - r, y: cy - r, w: 2 * r, d: 2 * r, r }, 0, 2, P)}" style="filter:blur(4px)"/>${body}` + `<g data-part="${name}"></g>`;
}
put([CALLER_CUP[0], CALLER_CUP[1], 1], cup(CALLER_CUP[0], CALLER_CUP[1], 9, 4.2, "caller-coins"), 0, "caller-cup");
put([PAYOUT_CUP[0], PAYOUT_CUP[1], 1], cup(PAYOUT_CUP[0], PAYOUT_CUP[1], 10, 4.6, "payout-coins"), 0, "payout-cup");

// ---------------------------------------------------------------- cabinet body
{
  const body = [];
  body.push(S(k.slabOf(KICK, 0, Z0, P, 4, 0), { tone: "lo" }));
  const seams = [k.sideSeam(BODY, 48, P, 6)].join("");
  body.push(S(k.slabOf(BODY, Z0, ZB - Z0, P, 6, 0.8), { tone: "mid", inner: Ln(seams, { tone: "faint" }) }));
  // front panel joints, edge to edge
  body.push(Ln([-8, 16].map((x) => path3([[x, FY, Z0], [x, FY, ZB]])).join(""), { tone: "faint" }));
  // right face: vents, rating plate, joints
  const side = planeMatrix([RX + 0.02, 52, ZB - 6], SIDE_EX, DOWN);
  const vents = Array.from({ length: 7 }, (_, i) => `<rect x="${64 + i * 6.2}" y="4" width="2.6" height="22" rx="1.3"/>`).join("");
  body.push(`<g transform="${side}" class="dc-vents">${vents}</g>`);
  body.push(Ln(path3([[RX, 0, Z0], [RX, 0, ZB]]), { tone: "faint" }));
  // deck rim
  body.push(S(k.slabOf(DECK, ZB, ZD - ZB, P, 6, 1), { tone: "mid", inner: Ln(k.sideSeam(DECK, ZB + 1.6, P, 6), { tone: "faint" }) }));
  body.push(Ln(k.planOutline(k.insetPlan(DECK, 4), ZD, P), { tone: "faint" }));
  body.push(screws(k.corners(DECK, 6).map(([x, y]) => [x, y, ZD]), 0.5));
  put([0, 0, 50], body.join(""), -400, "body");
}
// rating plate on the right face
put([RX + 0.5, -40, 30], box(RX, -52, 22, RX + 0.8, -26, 38, { tone: "lo" }, { r: 0.3 }) +
  `<g transform="${planeMatrix([RX + 0.82, -27, 36], SIDE_EX, DOWN)}">${[18, 13, 16].map((w, i) => `<path class="iso-line" data-tone="lo" data-free="" d="M3 ${3.6 + i * 3.6}H${3 + w}"/>`).join("")}</g>` +
  screws([[RX + 0.8, -50, 24], [RX + 0.8, -28, 24], [RX + 0.8, -50, 36], [RX + 0.8, -28, 36]], 0.4), 0, "rating");

// backers' window on the vault door: price gauge, 600-DAA clock, deposit hopper, withdrawal cards, record drawer
const GAUGE = { c: [72, 75], r: 9 };
const CLOCK = { c: [70, 52], r: 6.2 };
const HOPPER = { c: [96, FY + 10.6], r: 4.2, z0: 30, z1: 84 };
const CARDS = { x: [34, 46], y: FY + 8.6, z: 30 };
const RECORD = { x0: 60, x1: 88, z0: 18, z1: 24.5, depth: 18 };
const GAUGE_RANGE = [0.85, 1.1];
const gaugeAngle = (p) => -0.75 * Math.PI + ((p - GAUGE_RANGE[0]) / (GAUGE_RANGE[1] - GAUGE_RANGE[0])) * 1.5 * Math.PI;
// ---------------------------------------------------------------- vault (front right)
assembly([76, FY + 2, 50], 0, "vault", () => {
  const out = [];
  out.push(box(DOOR.x0 - 4, FY, DOOR.z0 - 4, DOOR.x1 + 4, FY + 2, DOOR.z1 + 4, { tone: "lo" }, { r: 1.2, steps: 3 }));
  out.push(box(DOOR.x0, FY + 2, DOOR.z0, DOOR.x1, FY + 5, DOOR.z1, { tone: "mid" }, { r: 1.4, steps: 3, bevel: 0.6 }));
  const frameBolts = [];
  for (let x = DOOR.x0 + 2; x <= DOOR.x1 - 2; x += (DOOR.x1 - DOOR.x0 - 4) / 6) frameBolts.push([x, FY + 2, DOOR.z1 + 2], [x, FY + 2, DOOR.z0 - 2]);
  out.push(screws(frameBolts, 0.45));
  put([76, FY + 2, 50], out.join(""), 0, "vault.door");

  // hinges on the left edge
  for (const z of [26, 80]) put([DOOR.x0 - 2, FY + 4, z], cyl(DOOR.x0 - 2, FY + 4.2, 2.2, z - 6, 12, { tone: "mid" }, 20) + cyl(DOOR.x0 - 2, FY + 4.2, 2.6, z + 6, 1.2, { tone: "lo" }, 20), 1, "vault.hinge");

  // wheel: hub, rim and three spokes
  const WC = [40, FY + 5, 64];
  const WF = G.frameOf(WC, [0, 1, 0], [1, 0, 0], [0, 0, 1]);
  const wheel = [];
  wheel.push(disc(WF, 0, 1.2, 7.6, { tone: "lo" }));
  for (let i = 0; i < 3; i++) {
    const a = (i / 3) * Math.PI * 2 + Math.PI / 2;
    const tip = G.pointOf(WF, 2.6, 13.2, a);
    const base = G.pointOf(WF, 2.6, 3.6, a);
    const SF = along(base, G.unit3(G.sub3(tip, base)));
    wheel.push(disc(SF, 0, 9.6, 1, { tone: "mid" }, { steps: 16 }));
    wheel.push(S(G.sphereOf(G.pointOf(WF, 2.6, 15.4, a), 2.1, P, { steps: 24 }), { tone: "hi" }));
  }
  wheel.push(S(G.ringBand(1.6, 3.6, 12, 13.4, WF, P), { tone: "mid" }));
  wheel.push(disc(WF, 1.2, 4.2, 3.8, { tone: "hi" }) + Ln(G.circleOf(WF, 4.2, 1.8, P, 24), { tone: "lo" }));
  put(0, wheel.join(""));

  // ---- the backers' window on the vault door
  const DY = FY + 5;
  // share-price gauge (A + VA) / (S + VS)
  const GF = G.frameOf([GAUGE.c[0], DY, GAUGE.c[1]], [0, 1, 0], [1, 0, 0], [0, 0, 1]);
  const gTicks = Array.from({ length: 26 }, (_, i) => {
    const a = gaugeAngle(GAUGE_RANGE[0] + ((GAUGE_RANGE[1] - GAUGE_RANGE[0]) * i) / 25);
    const r0 = i % 5 ? GAUGE.r - 2.6 : GAUGE.r - 3.6;
    return `M${f2(Math.sin(a) * r0)} ${f2(-Math.cos(a) * r0)}L${f2(Math.sin(a) * (GAUGE.r - 1.6))} ${f2(-Math.cos(a) * (GAUGE.r - 1.6))}`;
  });
  const one = gaugeAngle(1);
  put(0, disc(GF, 0, 1.2, GAUGE.r + 1.6, { tone: "lo" }) + disc(GF, 1.2, 3, GAUGE.r, { tone: "hi", crease: "lo" }) + k.fadedSvg(G.ribsOf(GF, 1.2, 3, GAUGE.r, 48, P, { fade: [0.1, 0.55], seams: true }), { tone: "lo" }));
  put(0, `<g transform="${planeMatrix([GAUGE.c[0], DY + 3.02, GAUGE.c[1]], FRONT_EX, DOWN)}"><path class="iso-line" data-tone="lo" d="${gTicks.filter((_, i) => i % 5).join("")}"/><path class="iso-line" data-tone="mid" d="${gTicks.filter((_, i) => !(i % 5)).join("")}"/><path class="iso-line" data-tone="hi" d="M${f2(Math.sin(one) * (GAUGE.r - 4.6))} ${f2(-Math.cos(one) * (GAUGE.r - 4.6))}L${f2(Math.sin(one) * (GAUGE.r - 3.6))} ${f2(-Math.cos(one) * (GAUGE.r - 3.6))}"/><path class="iso-line" data-tone="lit" data-free="end" data-part="price-needle" style="stroke-width:1.1" d="M0 0L0 -5"/><circle class="iso-line" data-tone="mid" r="${f2(GAUGE.r - 1.6)}"/><circle class="dc-hub" r="1.1"/></g>`);
  // request clock: the hand sweeps once in 600 DAA after every vault spend; the lamp lights when a request may land
  const CF = G.frameOf([CLOCK.c[0], DY, CLOCK.c[1]], [0, 1, 0], [1, 0, 0], [0, 0, 1]);
  const cTicks = Array.from({ length: 12 }, (_, i) => {
    const a = (i / 12) * Math.PI * 2;
    const r0 = i % 3 ? CLOCK.r - 1.8 : CLOCK.r - 2.6;
    return `M${f2(Math.sin(a) * r0)} ${f2(-Math.cos(a) * r0)}L${f2(Math.sin(a) * (CLOCK.r - 1))} ${f2(-Math.cos(a) * (CLOCK.r - 1))}`;
  }).join("");
  put(0, disc(CF, 0, 1, CLOCK.r + 1.2, { tone: "lo" }) + disc(CF, 1, 2.4, CLOCK.r, { tone: "hi", crease: "lo" }));
  put(0, `<g transform="${planeMatrix([CLOCK.c[0], DY + 2.42, CLOCK.c[1]], FRONT_EX, DOWN)}"><path class="iso-line" data-tone="mid" d="${cTicks}"/><circle class="iso-line" data-tone="lo" r="${f2(CLOCK.r - 1)}"/><path class="iso-line" data-tone="lit" data-free="end" data-part="clock-hand" style="stroke-width:1" d="M0 0L0 -4"/><circle class="dc-hub" r="0.8"/></g>`);
  const LF = G.frameOf([CLOCK.c[0] + 10, DY, CLOCK.c[1] - 4.4], [0, 1, 0], [1, 0, 0], [0, 0, 1]);
  put(0, disc(LF, 0, 0.8, 2.4, { tone: "lo" }) + `<g class="dc-lamp" data-part="ready">${disc(LF, 0.8, 2.2, 1.6, { tone: "hi" })}</g>`);
  // withdrawal card rack: a shelf with one slot per pending request
  put(0, box(CARDS.x[0] - 7, DY, CARDS.z - 4, CARDS.x[1] + 7, DY + 7, CARDS.z, { tone: "mid" }, { r: 1, steps: 3, bevel: 0.3 }) + screws([[CARDS.x[0] - 5, DY + 7, CARDS.z - 2], [CARDS.x[1] + 5, DY + 7, CARDS.z - 2]], 0.4));
  CARDS.x.forEach((x, i) => {
    put(0, `<path class="dc-slot" d="${k.planOutline({ x: x - 3.4, y: CARDS.y - 0.4, w: 6.8, d: 1.6, r: 0.6 }, CARDS.z + 0.02, P, 2)}"/>`);
    put(0, `<g data-card="${i}"></g>`);
  });
  // settlement record: opening in the door and the drawer draw() fills
  put(0, `<g transform="${planeMatrix([RECORD.x0, DY + 0.02, RECORD.z1], FRONT_EX, DOWN)}"><rect class="dc-window" width="${RECORD.x1 - RECORD.x0}" height="${RECORD.z1 - RECORD.z0}" rx="1"/><rect class="iso-line" data-tone="lo" width="${RECORD.x1 - RECORD.x0}" height="${RECORD.z1 - RECORD.z0}" rx="1"/></g>`);
  put(0, `<g data-part="record"></g>`);
  // deposit hopper: a glass column on a bracket, coin slot in the cap
  const [hx, hy] = HOPPER.c;
  put(0, box(hx - 6, DY, HOPPER.z0 - 3, hx + 6, hy + 6, HOPPER.z0, { tone: "mid" }, { r: 1, steps: 3, bevel: 0.3 }) + cyl(hx, hy, HOPPER.r + 1.2, HOPPER.z0, 1.6, { tone: "lo" }, 32));
  put(0, `<g data-part="hopper"></g>`);
  put(0, Ln(k.ring(HOPPER.c[0], HOPPER.c[1], HOPPER.r, HOPPER.z0 + 1.6, P, 32), { tone: "lo" }));
  const nrm = [-Math.sin((AZ * Math.PI) / 180), Math.cos((AZ * Math.PI) / 180)];
  const hl = [hx - HOPPER.r * nrm[0], hy - HOPPER.r * nrm[1]];
  const hr = [hx + HOPPER.r * nrm[0], hy + HOPPER.r * nrm[1]];
  const hz0 = HOPPER.z0 + 1.6;
  put(0, box(hx - 1.6, DY, HOPPER.z1 - 1, hx + 1.6, hy, HOPPER.z1, { tone: "lo" }, { r: 0.3 }));
  put(0, `<path class="dc-glass" d="${k.pathOf([at([hl[0], hl[1], hz0]), at([hl[0], hl[1], HOPPER.z1]), at([hr[0], hr[1], HOPPER.z1]), at([hr[0], hr[1], hz0])], true)}"/>` + Ln(path3([[hl[0], hl[1], hz0], [hl[0], hl[1], HOPPER.z1]]) + path3([[hr[0], hr[1], hz0], [hr[0], hr[1], HOPPER.z1]]), { tone: "mid" }) +
    `<path class="dc-glare" d="${path3([[hx + 0.5 * HOPPER.r * nrm[0] - 0.8, hy + 0.5 * HOPPER.r * nrm[1] + 0.5, hz0 + 3], [hx + 0.5 * HOPPER.r * nrm[0] - 0.8, hy + 0.5 * HOPPER.r * nrm[1] + 0.5, HOPPER.z1 - 3]])}" style="stroke-width:.8"/>`);
  put(0, cyl(hx, hy, HOPPER.r + 0.9, HOPPER.z1, 2.2, { tone: "mid" }, 32, 0.4) + `<path class="dc-slot" d="${k.planOutline({ x: hx - 2.8, y: hy - 0.7, w: 5.6, d: 1.4, r: 0.7 }, HOPPER.z1 + 2.22, P, 2)}"/>`);

  // sight glass with the KAS level and the liability line L
  out.length = 0;
  out.push(box(SIGHT.x0 - 2.4, FY + 5, SIGHT.z0 - 2.4, SIGHT.x1 + 2.4, FY + 7.4, SIGHT.z1 + 2.4, { tone: "mid" }, { r: 1.2, steps: 3, bevel: 0.4 }));
  const gm = planeMatrix([SIGHT.x0, FY + 7.42, SIGHT.z1], FRONT_EX, DOWN);
  const gw = SIGHT.x1 - SIGHT.x0;
  const gh = SIGHT.z1 - SIGHT.z0;
  const glassTicks = Array.from({ length: 11 }, (_, i) => `M${gw} ${f2((i / 10) * gh)}H${i % 5 ? gw - 2 : gw - 3.6}`).join("");
  out.push(
    `<g transform="${gm}"><clipPath id="dc-sight"><rect width="${gw}" height="${gh}" rx="1.6"/></clipPath>` +
      `<rect class="dc-window" width="${gw}" height="${gh}" rx="1.6"/>` +
      `<g clip-path="url(#dc-sight)"><rect class="dc-level" data-part="vault-level" x="0" y="${gh}" width="${gw}" height="0"/>` +
      `<path class="dc-level-top" data-part="vault-top" d="M0 ${gh}H${gw}"/></g>` +
      `<path class="iso-line" data-tone="mid" d="${glassTicks}"/>` +
      `<path class="dc-liability" data-part="vault-l" d="M0 ${gh}H${gw}"/>` +
      `<rect class="iso-line" data-tone="lo" width="${gw}" height="${gh}" rx="1.6"/></g>`,
  );
  out.push(screws([[SIGHT.x0 - 1.2, FY + 7.4, SIGHT.z0 - 1.2], [SIGHT.x1 + 1.2, FY + 7.4, SIGHT.z0 - 1.2], [SIGHT.x0 - 1.2, FY + 7.4, SIGHT.z1 + 1.2], [SIGHT.x1 + 1.2, FY + 7.4, SIGHT.z1 + 1.2]], 0.4));
  put([116, FY + 6, 55], out.join(""), 2, "vault.sight");
  // lock bolts on the opening edge
  for (const z of [30, 54, 78]) put([DOOR.x1 + 1, FY + 4, z], box(DOOR.x1 - 1, FY + 3, z - 2, DOOR.x1 + 3, FY + 5.4, z + 2, { tone: "hi" }, { r: 0.6 }), 1.5, "vault.bolt");
});

// ---------------------------------------------------------------- pilaster with tier lamps and the boost drawer
{
  const out = [];
  out.push(box(-8, FY, Z0, 16, FY + 1.6, ZB, { tone: "mid" }, { r: 0.6 }));
  out.push(screws([[-5, FY + 1.6, Z0 + 4], [13, FY + 1.6, Z0 + 4], [-5, FY + 1.6, ZB - 4], [13, FY + 1.6, ZB - 4]], 0.45));
  // payout spout feeding the cup
  const SF = G.frameOf([4, FY + 1.6, 30], [0, 1, 0], [1, 0, 0], [0, 0, 1]);
  out.push(disc(SF, 0, 3, 5.4, { tone: "lo" }) + disc(SF, 3, 9, 3.6, { tone: "mid" }) + Ln(G.circleOf(SF, 9, 2.4, P, 28), { tone: "lo" }));
  // Kaspa badge at the top of the pilaster
  const BF = G.frameOf([4, FY + 1.6, 84], [0, 1, 0], [1, 0, 0], [0, 0, 1]);
  out.push(disc(BF, 0, 1, 9.4, { tone: "lo" }) + `<g class="dc-kas">${disc(BF, 1, 2.6, 8.2, { tone: "hi" })}</g>`);
  out.push(`<g transform="${planeMatrix([4, FY + 4.22, 84], FRONT_EX, DOWN)}">${kMark(0, 0, 8.2)}</g>`);
  put([4, FY + 2, 30], out.join(""), 0, "pilaster");
}
{
  // boost-pot drawer: case lip, front, pull and label holder (moves as one)
  const out = [];
  put([-74, FY + 0.5, 30], box(-134, FY, 12, -14, FY + 1.2, 48, { tone: "lo" }, { r: 1 }), 0, "drawer.case");
  out.push(box(-130, FY + 1.2, 15, -18, FY + 4.4, 45, { tone: "mid" }, { r: 1.2, steps: 3, bevel: 0.5 }));
  const dm = planeMatrix([-130, FY + 4.42, 45], FRONT_EX, DOWN);
  out.push(`<g transform="${dm}"><rect class="iso-line" data-tone="faint" x="3" y="3" width="106" height="24" rx="1.5"/><rect class="dc-card" x="42" y="5.5" width="28" height="7" rx="0.6"/><path class="iso-line" data-tone="lo" data-free="" d="M45 8H60M45 10.4H54"/></g>`);
  out.push(screws([[-88.6, FY + 4.4, 39], [-59.4, FY + 4.4, 39]], 0.4));
  // pull: two posts and a bar
  out.push(box(-86, FY + 4.4, 25, -83, FY + 9, 28, { tone: "mid" }, { r: 0.5 }) + box(-65, FY + 4.4, 25, -62, FY + 9, 28, { tone: "mid" }, { r: 0.5 }));
  out.push(box(-87.5, FY + 9, 24.6, -60.5, FY + 11.2, 28.4, { tone: "hi" }, { r: 1, steps: 3 }));
  put([-74, FY + 4, 30], `<g data-part="drawer">${out.join("")}</g>`, 1, "drawer");
}

const PINS = Array.from({ length: 16 }, (_, i) => [READER.x + 3.6 + (i % 8) * 3, READER.y + (i < 8 ? 6 : 11), READER.z + READER.h]);
// ---------------------------------------------------------------- ledge, prize trays, tier lamps, claim reader
{
  const out = [];
  for (const x of [-124, -28]) out.push(box(x - 1.5, FY, LEDGE.z - 14, x + 1.5, FY + 18, LEDGE.z, { tone: "lo" }, { r: 0.3 }));
  put([-76, FY + 9, LEDGE.z - 7], out.join(""), 0, "ledge.brackets");
  const LEDGE_SVG = (box(LEDGE.x, LEDGE.y, LEDGE.z, LEDGE.x + LEDGE.w, LEDGE.y + LEDGE.d, LEDGE.z + 4, { tone: "mid" }, { r: 1.5, steps: 3, bevel: 0.5 }) +
    Ln(k.planOutline({ x: LEDGE.x + 2.5, y: LEDGE.y + 1.5, w: LEDGE.w - 5, d: LEDGE.d - 3, r: 1 }, LEDGE.z + 4, P, 3), { tone: "faint" }));
  // tier lamps above the trays, one per tray, on a trim rail
  // tier rail: one lamp per tray, and under it the tier's pips (matched main numbers, ring = bonus matched)
  assembly([-71, FY + 1, 88], 0, "lamp.rail", () => {
    put(0, box(-134, FY, 74, -10, FY + 1.4, 97, { tone: "lo" }, { r: 0.8, steps: 3 }) + screws([[-131, FY + 1.4, 77], [-13, FY + 1.4, 77], [-131, FY + 1.4, 94], [-13, FY + 1.4, 94]], 0.4));
    const PIPS = [[5, true], [5, false], [4, true], [4, false], [3, true]];
    TRAYS.forEach((t, i) => {
      const cx = t.x + t.w / 2;
      const LF = G.frameOf([cx, FY + 1.4, 89.5], [0, 1, 0], [1, 0, 0], [0, 0, 1]);
      put(0, disc(LF, 0, 1.2, 4.6, { tone: "lo" }) + `<g class="dc-lamp" data-lamp="${i}">${disc(LF, 1.2, 3.4, 3.4, { tone: "hi" })}</g>`);
      const [n, bonus] = PIPS[i];
      const pm = planeMatrix([cx, FY + 1.42, 80.4], FRONT_EX, DOWN);
      const dots = Array.from({ length: n }, (_, j) => `<circle class="dc-hub" cx="${f2((j - (n - 1) / 2) * 2.2 - (bonus ? 1.6 : 0))}" cy="0" r="0.62"/>`).join("");
      const ring = bonus ? `<circle class="iso-line" data-tone="hi" cx="${f2(((n - 1) / 2) * 2.2 + 1.4)}" cy="0" r="0.95"/>` : "";
      put(0, `<g transform="${pm}">${dots}${ring}</g>`);
    });
  });
  // ledge, trays, reader and pins draw as one assembly, left to right
  capture = [LEDGE_SVG];
  // trays: floor, far walls, the coin stack slot, near walls
  TRAYS.forEach((t, i) => {
    const z = t.z;
    const far = [box(t.x, t.y, z, t.x + t.w, t.y + t.d, z + 0.8, { tone: "lo" }, { r: 0.8, steps: 3 }), box(t.x, t.y, z + 0.8, t.x + t.w, t.y + 1, z + t.h, { tone: "mid" }, { r: 0.3 }), box(t.x, t.y + 1, z + 0.8, t.x + 1, t.y + t.d, z + t.h, { tone: "mid" }, { r: 0.3 })];
    const near = [box(t.x + t.w - 1, t.y + 1, z + 0.8, t.x + t.w, t.y + t.d, z + t.h, { tone: "mid" }, { r: 0.3 }), box(t.x + 1, t.y + t.d - 1, z + 0.8, t.x + t.w - 1, t.y + t.d, z + t.h - 2.4, { tone: "mid" }, { r: 0.3 })];
    put([t.x + t.w / 2, t.y + t.d / 2, z], far.join(""), 0.5, "tray.far");
    put([t.x + t.w / 2, t.y + t.d / 2, z + 1], `<g data-tray="${i}"></g>`, 0.6, "tray.stack");
    put([t.x + t.w / 2, t.y + t.d / 2, z + 2], near.join(""), 0.7, "tray.near");
  });
  // claim reader: body, card slot, 16 proof pins in two rows of 8
  const r = READER;
  const body = [box(r.x, r.y, r.z, r.x + r.w, r.y + r.d, r.z + r.h, { tone: "mid" }, { r: 1.2, steps: 3, bevel: 0.5 })];
  body.push(Ln(k.planOutline({ x: r.x + 2, y: r.y + 2, w: r.w - 4, d: r.d - 4, r: 0.8 }, r.z + r.h, P, 3), { tone: "faint" }));
  body.push(`<path class="dc-slot" d="${path3([[r.x + 5, r.y + r.d + 0.02, r.z + 3.4], [r.x + r.w - 5, r.y + r.d + 0.02, r.z + 3.4], [r.x + r.w - 5, r.y + r.d + 0.02, r.z + 4.4], [r.x + 5, r.y + r.d + 0.02, r.z + 4.4]], true)}"/>`);
  put([r.x + r.w / 2, r.y + r.d / 2, r.z + 2], body.join(""), 0.8, "reader");
}
{
  const pins = PINS.map((p, i) => ({ i, p })).sort((a, b) => depth(a.p) - depth(b.p));
  const svg = pins.map(({ i, p }) => `<g data-pin="${i}">${cyl(p[0], p[1], 1, p[2] - 2, 3, { tone: "hi" }, 16)}</g>`).join("");
  const wells = Ln(PINS.map((p) => k.ring(p[0], p[1], 1.5, p[2], P, 16)).join(""), { tone: "lo" });
  put([READER.x + 14, READER.y + 8, READER.z + READER.h + 1], wells + svg, 1, "reader.pins");
  const svgAll = capture.join("");
  capture = null;
  put([-71, FY + 11, LEDGE.z + 2], svgAll, 0, "ledge");
}

// ---------------------------------------------------------------- ticket rack: 16 glass shard tubes and the seal bar
const TUBE_R = 2.05;
const tubeX = (i) => RACK.x0 + i * RACK.pitch + RACK.pitch / 2;
const TUBE_Y = (RACK.y0 + RACK.y1) / 2;
assembly([RACK.x0 + 40, TUBE_Y, ZD + 20], -30, "rack", () => {
  const xEnd = RACK.x0 + 16 * RACK.pitch;
  put(0, box(RACK.x0 - 4, RACK.y0 + 2, ZD, xEnd + 3, RACK.y1 - 2, ZD + 4, { tone: "mid" }, { r: 1.4, steps: 3, bevel: 0.4 }));
  // count scale on the base, in front of the tubes
  const strip = { x: RACK.x0 - 2, y: RACK.y1 - 5.6, w: xEnd - RACK.x0 + 3, d: 3, r: 0.6 };
  put(0, S(k.slabOf(strip, ZD + 4, 0.5, P, 2), { tone: "lo", crease: "none" }));
  const t = k.topTicks(strip.x + 1.5, strip.x + strip.w - 1.5, RACK.pitch / 2, 2, strip.y, ZD + 4.5, [1.2, 2.2], P);
  put(0, Ln(t.minor, { tone: "lo" }) + Ln(t.major, { tone: "mid" }));
  // guide posts behind the tubes
  for (const x of [RACK.x0 - 2, xEnd + 1]) {
    put(0, cyl(x, RACK.y0 + 3.6, 2.4, ZD + 4, 2, { tone: "lo" }, 20) + cyl(x, RACK.y0 + 3.6, 1.5, ZD + 6, RACK.top + 22 - ZD - 6, { tone: "mid", crease: "none" }, 20) + cyl(x, RACK.y0 + 3.6, 2.1, RACK.top + 22, 1.6, { tone: "mid" }, 20));
  }
  const n = [-Math.sin((AZ * Math.PI) / 180), Math.cos((AZ * Math.PI) / 180)];
  for (let i = 0; i < 16; i++) {
    const cx = tubeX(i);
    put(0, cyl(cx, TUBE_Y, TUBE_R + 0.7, ZD + 4, 2.2, { tone: "lo" }, 20));
    put(0, `<g data-stack="${i}"></g>`);
    const side = (s) => [cx + s * TUBE_R * n[0], TUBE_Y + s * TUBE_R * n[1]];
    const [l, r] = [side(-1), side(1)];
    const z0 = ZD + 6.2;
    const z1 = RACK.top;
    const glassPoly = [at([l[0], l[1], z0]), at([l[0], l[1], z1]), at([r[0], r[1], z1]), at([r[0], r[1], z0])];
    put(0, `<path class="dc-glass" d="${k.pathOf(glassPoly, true)}"/>` + Ln(path3([[l[0], l[1], z0], [l[0], l[1], z1]]) + path3([[r[0], r[1], z0], [r[0], r[1], z1]]) + k.ring(cx, TUBE_Y, TUBE_R, z1, P, 24), { tone: "mid" }) +
      `<path class="dc-glare" d="${path3([[cx + 0.55 * TUBE_R * n[0] - 0.6, TUBE_Y + 0.55 * TUBE_R * n[1] + 0.4, z0 + 4], [cx + 0.55 * TUBE_R * n[0] - 0.6, TUBE_Y + 0.55 * TUBE_R * n[1] + 0.4, z1 - 4]])}" style="stroke-width:.7"/>`);
  }
});
// seal bar: rides the guide posts and drops onto the tube tops at close()
{
  const xEnd = RACK.x0 + 16 * RACK.pitch;
  const bar = box(RACK.x0 - 5, RACK.y0 + 0.6, 0, xEnd + 4, RACK.y1 - 2, 3.2, { tone: "hi" }, { r: 1.4, steps: 3, bevel: 0.5 });
  const caps = Ln(Array.from({ length: 16 }, (_, i) => k.ring(tubeX(i), TUBE_Y, TUBE_R + 0.5, 3.2, P, 20)).join(""), { tone: "lo" });
  const hasp = box(RACK.x0 + 36, RACK.y1 - 2, -3.4, RACK.x0 + 46, RACK.y1 - 0.8, 3.2, { tone: "mid" }, { r: 0.4 });
  put([RACK.x0 + 40, TUBE_Y, RACK.top + 8], `<g data-part="seal" class="dc-seal">${bar}${caps}${hasp}${screws([[RACK.x0 - 2, RACK.y0 + 3.6, 3.2], [xEnd + 1, RACK.y0 + 3.6, 3.2]], 0.5, "hi")}</g>`, -29.9, "rack.seal");
}

// ---------------------------------------------------------------- ticket terminal (buy)
{
  const t = TERM;
  const out = [];
  out.push(box(t.x, t.y, t.z, t.x + t.w, t.y + t.d, t.z + 3, { tone: "lo" }, { r: 2, steps: 3 }));
  out.push(box(t.x + 2, t.y + 2, t.z + 3, t.x + t.w - 2, t.y + t.d - 2, t.z + t.h, { tone: "mid" }, { r: 2.2, steps: 4, bevel: 0.6 }));
  const top = t.z + t.h;
  const slot = { x: t.x + 14, y: t.y + 12, w: 2.2, d: 11, r: 1.1 };
  out.push(box(slot.x - 4, slot.y - 3.4, top, slot.x + slot.w + 4, slot.y + slot.d + 3.4, top + 1.6, { tone: "hi" }, { r: 1.4, steps: 3, bevel: 0.3 }));
  out.push(`<path class="dc-slot" d="${k.planOutline(slot, top + 1.62, P, 3)}"/>`);
  out.push(Ln(k.planOutline(k.insetPlan({ x: t.x + 2, y: t.y + 2, w: t.w - 4, d: t.d - 4, r: 2 }, 3), top, P), { tone: "faint" }));
  out.push(screws([[t.x + 5, t.y + 5, top], [t.x + t.w - 5, t.y + 5, top], [t.x + 5, t.y + t.d - 5, top], [t.x + t.w - 5, t.y + t.d - 5, top]], 0.45));
  // counter window on the front face
  const fm = planeMatrix([t.x + 7, t.y + t.d - 1.98, top - 4], FRONT_EX, DOWN);
  out.push(`<g transform="${fm}"><rect class="dc-window" width="30" height="9" rx="1"/><text class="dc-digits" data-part="counter" x="15" y="6.6" text-anchor="middle" font-size="6.4">000000</text><rect class="iso-line" data-tone="lo" width="30" height="9" rx="1"/></g>`);
  // ticket mouth on the right face
  const sm = planeMatrix([t.x + t.w - 1.98, t.y + 34, top - 5], SIDE_EX, DOWN);
  out.push(`<g transform="${sm}"><rect class="dc-slot" width="22" height="2" rx="1"/></g>`);
  put([t.x + t.w / 2, t.y + t.d / 2, t.z + 10], out.join(""), 0, "terminal");
  // two bundle buttons on top
  for (const [x, y] of [[t.x + 30, t.y + 14], [t.x + 30, t.y + 26]]) put([x, y, top + 1], cyl(x, y, 3.4, top, 1, { tone: "lo" }, 24) + cyl(x, y, 2.6, top + 1, 1.6, { tone: "hi" }, 24), 0.5, "terminal.button");
}

// ---------------------------------------------------------------- globes: pedestals, glass, bands, hatch
function pedestal(c, baseR, cupR, cupZ, colR) {
  const out = [];
  out.push(cyl(c[0], c[1], baseR, ZD, 3.4, { tone: "lo" }, 56, 0.4));
  out.push(screws(Array.from({ length: 6 }, (_, i) => [c[0] + (baseR - 3) * Math.cos((i / 6) * Math.PI * 2 + 0.3), c[1] + (baseR - 3) * Math.sin((i / 6) * Math.PI * 2 + 0.3), ZD + 3.4]), 0.45));
  out.push(cyl(c[0], c[1], baseR * 0.72, ZD + 3.4, 3, { tone: "mid" }, 48, 0.4));
  out.push(cyl(c[0], c[1], colR, ZD + 6.4, cupZ - 4 - ZD - 6.4, { tone: "mid", crease: "none" }, 32));
  out.push(Ln(k.sideArc(c[0], c[1], colR, (ZD + cupZ) / 2, P, 24), { tone: "faint" }));
  out.push(cyl(c[0], c[1], cupR + 1.4, cupZ - 4, 4, { tone: "mid" }, 48, 0.4));
  return out.join("");
}
function globe(name, G0, ballR, balls, exitDir) {
  const c = G0.c;
  const outline = (() => {
    const s = at(c);
    const rr = G0.r * KS;
    return { s, rr };
  })();
  const { s, rr } = outline;
  const eq = ringHalves(c, G0.r + 0.6, c[2]);
  const eq2 = ringHalves(c, G0.r + 0.6, c[2] + 0.01);
  void eq2;
  const backSvg = `<circle class="dc-glass-back" cx="${f2(s[0])}" cy="${f2(s[1])}" r="${f2(rr)}"/>` + `<path class="dc-band" d="${eq.back}"/>`;
  // exit port on the sphere surface
  const port = G.add3(c, G.mul3(exitDir, G0.r));
  const PF = along(port, exitDir);
  const portSvg = disc(PF, -1, 3.2, ballR + 2.4, { tone: "mid" }) + Ln(G.circleOf(PF, 3.2, ballR + 0.8, P, 28), { tone: "lo" });
  const glare = (() => {
    const a0 = -2.5;
    const a1 = -1.6;
    const pts = Array.from({ length: 16 }, (_, i) => {
      const a = a0 + ((a1 - a0) * i) / 15;
      return [s[0] + Math.cos(a) * rr * 0.8, s[1] + Math.sin(a) * rr * 0.8];
    });
    return k.pathOf(pts);
  })();
  const hatchC = [c[0], c[1], c[2] + G0.r];
  const HF = G.frameOf([c[0], c[1], c[2] + G0.r - 0.6], [0, 0, 1], [1, 0, 0], [0, 1, 0]);
  const hatch = disc(HF, 0, 2.4, G0.r * 0.28, { tone: "mid" }) + disc(HF, 2.4, 4, G0.r * 0.14, { tone: "hi" }) + Dots(Array.from({ length: 8 }, (_, i) => at(G.pointOf(HF, 2.4, G0.r * 0.22, (i / 8) * Math.PI * 2))), { size: 0.4, tone: "mid" });
  void hatchC;
  const frontSvg =
    `<circle class="dc-glass" cx="${f2(s[0])}" cy="${f2(s[1])}" r="${f2(rr)}"/>` +
    `<path class="dc-band" d="${eq.front}"/>` +
    `<path class="dc-glare" d="${glare}"/>` +
    `<circle class="iso-line" data-tone="hi" cx="${f2(s[0])}" cy="${f2(s[1])}" r="${f2(rr)}"/>`;
  put(G.add3(c, [0, 0, -G0.r]), backSvg, 0.2, `${name}.back`);
  put(c, `<g data-part="${name}-balls"></g>`, 0.3, `${name}.balls`);
  put(G.add3(c, [0, 0, 0]), frontSvg + portSvg, 0.4, `${name}.front`);
  put([c[0], c[1], c[2] + G0.r], hatch, 0.5, `${name}.hatch`);
  put([c[0], c[1], G0.cupZ], frontBand(c[0], c[1], G0.cup + 1.4, G0.cupZ - 4, G0.cupZ, "mid"), 0.45, `${name}.lip`);
  return { port, balls };
}
put([GLOBE.c[0], GLOBE.c[1], ZD + 6], pedestal(GLOBE.c, 26, GLOBE.cup, GLOBE.cupZ, 8), -2, "globe.pedestal");
put([BONUS.c[0], BONUS.c[1], ZD + 6], pedestal(BONUS.c, 16, BONUS.cup, BONUS.cupZ, 5), -2, "bonus.pedestal");

// rest positions: balls settled in the bottom of each globe (deterministic pile)
function pile(c, inner, r, n, seed) {
  let s = seed;
  const rnd = () => ((s = (s * 16807) % 2147483647) / 2147483647);
  const out = [];
  for (let tries = 0; out.length < n && tries < 20000; tries++) {
    const a = rnd() * Math.PI * 2;
    const rad = Math.sqrt(rnd()) * (inner - r);
    const x = c[0] + rad * Math.cos(a);
    const y = c[1] + rad * Math.sin(a);
    // lowest z on the sphere floor at this radius, then stack on balls already there
    let z = c[2] - Math.sqrt(Math.max(0, (inner - r) ** 2 - rad ** 2));
    for (const q of out) {
      const dxy = Math.hypot(q[0] - x, q[1] - y);
      if (dxy < 2 * r) z = Math.max(z, q[2] + Math.sqrt(4 * r * r - dxy * dxy));
    }
    if (G.len3(G.sub3([x, y, z], c)) > inner - r) continue;
    out.push([x, y, z]);
  }
  return out;
}
const EXIT_MAIN = G.unit3([0.16, 0.8, -0.58]);
const EXIT_BONUS = G.unit3([-0.42, 0.78, -0.46]);
const mainRest = pile(GLOBE.c, GLOBE.r - 1.2, BALL_R, 30, 7);
const bonusRest = pile(BONUS.c, BONUS.r - 1, BBALL_R, MODEL.m, 11);
const mainGlobe = globe("globe", GLOBE, BALL_R, mainRest, EXIT_MAIN);
const bonusGlobe = globe("bonus", BONUS, BBALL_R, bonusRest, EXIT_BONUS);

// air blower behind the globe: housing, fan grille, duct into the pedestal column
{
  const B = { x0: 34, x1: 66, y0: -58, y1: -36, z1: ZD + 17 };
  const out = [box(B.x0, B.y0, ZD, B.x1, B.y1, ZD + 2, { tone: "lo" }, { r: 1, steps: 3 }), box(B.x0 + 1.5, B.y0 + 1.5, ZD + 2, B.x1 - 1.5, B.y1 - 1.5, B.z1, { tone: "mid" }, { r: 1.6, steps: 3, bevel: 0.5, inner: Ln(k.sideSeam({ x: B.x0 + 1.5, y: B.y0 + 1.5, w: B.x1 - B.x0 - 3, d: B.y1 - B.y0 - 3, r: 1.6 }, ZD + 7, P, 3), { tone: "faint" }) })];
  const FC = [(B.x0 + B.x1) / 2, (B.y0 + B.y1) / 2];
  out.push(Ln([8.4, 6, 3.6, 1.4].map((r) => k.ring(FC[0], FC[1], r, B.z1, P, 40)).join("") + [0, 1, 2, 3].map((i) => { const a = i * Math.PI / 2 + 0.4; return k.lineOnTop([FC[0] + 1.4 * Math.cos(a), FC[1] + 1.4 * Math.sin(a)], [FC[0] + 8.4 * Math.cos(a), FC[1] + 8.4 * Math.sin(a)], B.z1, P); }).join(""), { tone: "lo" }));
  out.push(Dots([at([FC[0], FC[1], B.z1])], { size: 0.9, tone: "mid" }));
  out.push(screws([[B.x0 + 4, B.y0 + 4, B.z1], [B.x1 - 4, B.y0 + 4, B.z1], [B.x0 + 4, B.y1 - 4, B.z1], [B.x1 - 4, B.y1 - 4, B.z1]], 0.45));
  // vent slots on the front face of the housing
  const vm = planeMatrix([B.x0 + 4, B.y1 - 1.48, B.z1 - 3], FRONT_EX, DOWN);
  out.push(`<g transform="${vm}" class="dc-vents">${Array.from({ length: 6 }, (_, i) => `<rect x="${i * 4.2}" y="0" width="1.8" height="8" rx="0.9"/>`).join("")}</g>`);
  put([FC[0], FC[1], ZD + 8], out.join(""), -1, "blower");
  const route = G.fillet([[B.x0 + 1.5, -47, ZD + 10], [19, -47, ZD + 10], [19, -6, ZD + 10], [8, -6, ZD + 10]], 6, 10, { P, tube: 2.4 });
  tubePieces(route, 2.4, P, { maxLength: 12 }).forEach((piece, i) => put(piece.mid, tubeSvg(piece, { tone: "mid" }), -1.5, `duct:${i}`));
  const DF = G.frameOf([8, -6, ZD + 10], [1, 0, 0], [0, 1, 0], [0, 0, 1]);
  put([8.6, -6, ZD + 10], disc(DF, 0, 1.6, 3.8, { tone: "lo" }), -1.4, "duct.flange");
  const DF2 = G.frameOf([B.x0 + 1.5, -47, ZD + 10], [-1, 0, 0], [0, 1, 0], [0, 0, 1]);
  put([B.x0, -47, ZD + 10], disc(DF2, 0, 1.6, 3.8, { tone: "lo" }), -0.9, "duct.flange2");
}
// service hatch on the deck, front right
{
  const h = { x: 94, y: 14, w: 38, d: 40, r: 2 };
  const out = [S(k.slabOf(h, ZD, 0.8, P, 3, 0.3), { tone: "mid", crease: "none" }), Ln(k.planOutline(k.insetPlan(h, 2.4), ZD + 0.8, P, 3), { tone: "faint" })];
  out.push(`<path class="dc-slot" d="${k.planOutline({ x: h.x + h.w / 2 - 6, y: h.y + h.d - 7, w: 12, d: 3, r: 1.5 }, ZD + 0.82, P, 3)}"/>`);
  out.push(screws(k.corners(h, 4).map(([x, y]) => [x, y, ZD + 0.8]), 0.45));
  put([h.x + h.w / 2, h.y + h.d / 2, ZD], out.join(""), -60, "hatch");
}
const BACKER_CUP = [54, 86];
put([BACKER_CUP[0], BACKER_CUP[1], 1], cup(BACKER_CUP[0], BACKER_CUP[1], 9, 4.2, "backer-items"), 0, "backer-cup");
// chutes from the exit ports to the rail
function chute(name, from, dir, to) {
  const a = G.add3(from, G.mul3(dir, 3.2));
  const b = G.add3(a, G.mul3(dir, 7));
  const route = G.fillet([a, b, [to[0], to[1], to[2] + 9], [to[0], to[1], to[2]]], 8, 10, { P, tube: 5.6 });
  tubePieces(route, 5.6, P, { maxLength: 14 }).forEach((piece, i) => put(piece.mid, tubeSvg(piece, { tone: "mid" }), 0.6, `${name}:${i}`));
  return route;
}
const RAIL_BALL_Z = RAIL.z + BALL_R + 0.2;
const GATE_TOP = ZD + 18;
const mainChute = chute("chute.main", mainGlobe.port, EXIT_MAIN, [ENTRY, RAIL.y - 3, GATE_TOP]);
const bonusChute = chute("chute.bonus", bonusGlobe.port, EXIT_BONUS, [BONUS_SLOT + 6, RAIL.y - 3, GATE_TOP]);
// drop gates: blocks on the rail back wall where each chute lands; the ball drops out underneath
for (const x of [ENTRY, BONUS_SLOT + 6]) {
  const g = box(x - 5, RAIL.y - 8, ZD + 11, x + 5, RAIL.y + 2, GATE_TOP, { tone: "mid" }, { r: 1, steps: 3, bevel: 0.4 });
  const collar = cyl(x, RAIL.y - 3, 6.8, GATE_TOP, 1.4, { tone: "lo" }, 32);
  put([x, RAIL.y - 3, ZD + 14], g + collar + screws([[x - 3.6, RAIL.y + 2, ZD + 12.4], [x + 3.6, RAIL.y + 2, ZD + 12.4]], 0.4), 0.35, "rail.gate");
}

// ---------------------------------------------------------------- result rail
{
  const r = RAIL;
  put([(r.x0 + r.x1) / 2, r.y, ZD + 1], box(r.x0, r.y - 7, ZD, r.x1, r.y + 7, ZD + 3, { tone: "mid" }, { r: 2, steps: 3, bevel: 0.4 }) +
    Ln(SLOTS.concat([BONUS_SLOT]).map((x) => k.ring(x, r.y, 2.6, ZD + 3, P, 20)).join(""), { tone: "lo" }), 0, "rail.floor");
  put([(r.x0 + r.x1) / 2, r.y - 6.4, ZD + 6], box(r.x0, r.y - 7, ZD + 3, r.x1, r.y - 5.8, ZD + 11, { tone: "mid" }, { r: 0.3 }), 0.1, "rail.back");
  put([(r.x0 + r.x1) / 2, r.y, ZD + 6], `<g data-part="rail-balls"></g>`, 0.2, "rail.balls");
  put([(r.x0 + r.x1) / 2, r.y + 6.4, ZD + 5], box(r.x0, r.y + 5.8, ZD + 3, r.x1, r.y + 7, ZD + 7.4, { tone: "mid" }, { r: 0.3 }) +
    screws([r.x0 + 4, r.x1 - 4, (DIVIDER + r.x0) / 2].map((x) => [x, r.y + 7, ZD + 5.2]), 0.4), 0.3, "rail.front");
  for (const x of [r.x0, r.x1 - 2.4]) put([x + 1.2, r.y, ZD + 7], box(x, r.y - 5.8, ZD + 3, x + 2.4, r.y + 5.8, ZD + 12, { tone: "mid" }, { r: 0.6 }), 0.4, "rail.stop");
  put([DIVIDER, r.y, ZD + 7], box(DIVIDER - 1.6, r.y - 5.8, ZD + 3, DIVIDER + 1.6, r.y + 5.8, ZD + 13, { tone: "hi" }, { r: 0.6 }), 0.4, "rail.divider");
}

// ---------------------------------------------------------------- right side: close() and draw() levers, seed dial
const LEVER_X = RX + 9.4;
for (const [id, L] of Object.entries(LEVERS)) {
  const F = G.frameOf([RX, L.y, L.z], [1, 0, 0], [0, 1, 0], [0, 0, 1]);
  const mount = [disc(F, 0, 1.4, 11, { tone: "lo" }), Dots([0, 1, 2, 3, 4, 5].map((i) => at(G.pointOf(F, 1.4, 9, (i / 6) * Math.PI * 2 + 0.5))), { size: 0.45, tone: "mid" }), disc(F, 1.4, 7.6, 5.6, { tone: "mid" }), Ln(G.arcOf(F, 4.4, 5.6, P), { tone: "faint" })];
  put([RX + 3, L.y, L.z], mount.join(""), 0, `lever.${id}.mount`);
  put([LEVER_X, L.y, L.z + 30], `<g data-lever="${id}"></g>`, 0, `lever.${id}`);
  put([RX + 10, L.y, L.z], disc(G.frameOf([RX + 7.6, L.y, L.z], [1, 0, 0], [0, 1, 0], [0, 0, 1]), 3.4, 5.4, 3.6, { tone: "hi" }), 40, `lever.${id}.cap`);
}
{
  // TEST SEED plaque and seed dial beside the draw lever
  const pm = planeMatrix([RX + 1.02, 58, 58], SIDE_EX, DOWN);
  const plaque = box(RX, 32, 48, RX + 1, 58, 59, { tone: "lo" }, { r: 0.4 }) +
    `<g transform="${pm}"><rect class="iso-line" data-tone="faint" x="1.2" y="1.2" width="23.6" height="8.6" rx="0.8"/>${k.faceTextSvg("", "TEST SEED", { size: 3.4, tone: "mid", anchor: "middle" }).replace("<text", '<text x="13" y="6.9"')}</g>` +
    screws([[RX + 1, 33.4, 53.5], [RX + 1, 56.6, 53.5]], 0.4);
  put([RX + 1, 45, 53], plaque, 1, "seed.plaque");
  const SF = G.frameOf([RX, 45, 72], [1, 0, 0], [0, 1, 0], [0, 0, 1]);
  const dial = disc(SF, 0, 1, 8, { tone: "lo" }) + disc(SF, 1, 3, 6.4, { tone: "hi" }) + k.fadedSvg(G.ribsOf(SF, 1, 3, 6.4, 32, P, { fade: [0.1, 0.55], seams: true }), { tone: "lo" });
  const sm = planeMatrix([RX + 3.02, 45, 72], SIDE_EX, DOWN);
  const ticks = Array.from({ length: 24 }, (_, i) => {
    const a = (i / 24) * Math.PI * 2;
    return `M${f2(Math.sin(a) * (i % 6 ? 4.6 : 3.8))} ${f2(-Math.cos(a) * (i % 6 ? 4.6 : 3.8))}L${f2(Math.sin(a) * 5.8)} ${f2(-Math.cos(a) * 5.8)}`;
  }).join("");
  put([RX + 3, 45, 72], dial + `<g transform="${sm}"><path class="iso-line" data-tone="lo" d="${ticks}"/><path class="iso-line" data-tone="lit" data-free="end" data-part="seed-needle" d="M0 0L0 -4.4" style="stroke-width:1.1"/><circle class="dc-hub" r="1.2"/></g>`, 2, "seed.dial");
}

// ---------------------------------------------------------------- movers drawn on top
// KAS coin: a teal disc with the K mark on its face
const coinArt = () => {
  const F = G.frameOf([0, 0, 0], [0, 0, 1], [1, 0, 0], [0, 1, 0]);
  const face = planeMatrix([0, 0, 0.52], FRONT_EX, TOP_DOWN);
  return `<g class="dc-kas">${S(G.disc(-0.5, 0.5, 3.8, F, P, { steps: 32 }), { tone: "hi" })}</g>` + `<g transform="${face}">${kMark(0, 0, 2.6)}</g>`;
};
const ticketArt = S(k.slabOf({ x: -2, y: -6, w: 4, d: 12, r: 0.5 }, 0, 0.6, P, 2), { tone: "hi", lit: true });
const claimCard = S(k.slabOf({ x: -9, y: -7, w: 18, d: 14, r: 0.8 }, 0, 0.6, P, 3), { tone: "hi", lit: true }) +
  Ln(k.lineOnTop([-6, 2], [6, 2], 0.6, P) + k.lineOnTop([-6, -1], [3, -1], 0.6, P), { tone: "lo", free: true });

// ---------------------------------------------------------------- assemble
items.sort((a, b) => a.key - b.key);
const svg = k.figureSvg({
  width: W,
  height: H,
  label:
    "An isometric lottery draw machine on a bench. On its deck: a coin terminal that sells tickets, a rack of 16 ticket slots with a seal bar, a large glass globe of 30 numbered balls and a small bonus globe, joined by chutes to a result rail. On the right side are two pull levers, close() and draw(), with a dial and plaque marked TEST SEED. On the front: five prize trays with lamps, a claim reader with 16 proof pins, a boost-pot drawer and a vault door with a sight glass that shows the KAS level and a red liability line.",
  body: [
    ...back,
    ...items.map((it) => `<g class="it" data-name="${it.name}">${it.svg}</g>`),
    `<g data-part="movers"><g data-part="coin" style="display:none">${coinArt()}</g><g data-part="ticket" style="display:none">${ticketArt}</g><g data-part="claim-card" style="display:none">${claimCard}</g><g data-part="caller-coin" style="display:none">${coinArt()}</g><g data-part="payout-coin" style="display:none">${coinArt()}</g></g>`,
  ],
});

// ---------------------------------------------------------------- data for the page
const ORIGIN = at([0, 0, 0]);
// a teal ball; balls in the glass carry the K mark (turned live as they tumble), rail balls carry their number
const ballBody = (r) => `<g class="dc-kas">${S(G.sphereOf([0, 0, 0], r, P, { steps: 40 }), { tone: "hi" })}</g>`;
const ballArt = (r) => ballBody(r) + `<g data-k>${`<g transform="translate(${f2(ORIGIN[0])} ${f2(ORIGIN[1])})">${kMark(0, 0, r * KS * 0.82)}</g>`}</g>`;
const DATA = {
  P,
  V,
  KS,
  model: MODEL,
  globe: { c: GLOBE.c, r: GLOBE.r, rest: mainRest, ballR: BALL_R, port: mainGlobe.port, art: ballArt(BALL_R), plain: ballBody(BALL_R) },
  bonus: { c: BONUS.c, r: BONUS.r, rest: bonusRest, ballR: BBALL_R, port: bonusGlobe.port, art: ballArt(BBALL_R), plain: ballBody(BBALL_R) },
  rail: { gateZ: ZD + 11 - BALL_R, slots: SLOTS, bonusSlot: BONUS_SLOT, entry: ENTRY, y: RAIL.y, z: RAIL_BALL_Z, mainEnd: mainChute[mainChute.length - 1], bonusEnd: bonusChute[bonusChute.length - 1] },
  origin: ORIGIN,
  rack: { ...RACK, unit: STACK_UNIT },
  trays: TRAYS,
  pins: PINS,
  term: { slot: [TERM.x + 15.1, TERM.y + 17.5, TERM.z + TERM.h + 1.6], mouth: [TERM.x + TERM.w + 2, TERM.y + 23, TERM.z + TERM.h - 6] },
  levers: { x: LEVER_X, len: LEVER_LEN, close: LEVERS.close, draw: LEVERS.draw },
  seal: { up: RACK.top + 14, down: RACK.top },
  sight: { h: SIGHT.z1 - SIGHT.z0, w: SIGHT.x1 - SIGHT.x0 },
  cups: { caller: [...CALLER_CUP, 2.2], payout: [...PAYOUT_CUP, 2.4] },
  spout: [4, FY + 10.6, 30],
  teller: { hopper: HOPPER, cards: CARDS, record: { ...RECORD, y: FY + 5 }, range: GAUGE_RANGE, gauge: GAUGE, clock: CLOCK, cup: [...BACKER_CUP, 2.2], from: [70, 120, 70] },
  reader: { slot: [READER.x + READER.w / 2, READER.y + READER.d, READER.z + 3.9] },
  jackpotTray: [TRAYS[0].x + TRAYS[0].w / 2, TRAYS[0].y + TRAYS[0].d / 2, TRAYS[0].z + 0.8],
};

// ---------------------------------------------------------------- the guided tour (--tour): one camera stop per call
// Each stop plays a window [a, b] of the round clock while the camera frames the parts that move in it.
// tq is the backers' clock: held at 0 through buy(), run on its own at the vault window, then past every request.
if (TOUR) {
  const cube = (x0, y0, z0, x1, y1, z1) => [[x0, y0, z0], [x1, y0, z0], [x0, y1, z0], [x1, y1, z0], [x0, y0, z1], [x1, y0, z1], [x0, y1, z1], [x1, y1, z1]];
  const ball = (c, r) => cube(c[0] - r, c[1] - r, c[2] - r, c[0] + r, c[1] + r, c[2] + r);
  // the parts' box on screen; tour.js fits the camera to it at run time, around the real callout size
  const boxOf = (pts) => {
    const q = pts.map(at);
    const xs = q.map((p) => p[0]);
    const ys = q.map((p) => p[1]);
    return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)].map(f2);
  };
  const rackEnd = RACK.x0 + 16 * RACK.pitch;
  const leverBox = (y) => [...cube(RX, y - 22, 62, LEVER_X + 6, y + 54, 146), [CALLER_CUP[0], CALLER_CUP[1], 0]];
  const tray = (i) => cube(TRAYS[i].x, TRAYS[i].y, TRAYS[i].z, TRAYS[i].x + TRAYS[i].w, TRAYS[i].y + TRAYS[i].d, TRAYS[i].z + 16);
  const lZ = SIGHT.z0 + (SIGHT.z1 - SIGHT.z0) * (MODEL.vault.L / 1e7);
  const MB = MODEL.backers;
  const fk = (n, d = 0) => n.toLocaleString("en-US", { maximumFractionDigits: d });
  const S = [
    {
      id: "round", title: "One round, no operator", t: [0, 0], tq: "zero", hold: 4.2, side: "right", full: true, anchor: null,
      text: `Every part is held by a covenant on Kaspa L1. Anyone may buy, pull the levers or claim; the machine only accepts moves its rules allow.`,
    },
    {
      id: "buy", title: "buy()", t: [0, 5.9], tq: "zero", hold: 2.2, side: "right", anchor: [TERM.x + 15.1, TERM.y + 17.5, TERM.z + TERM.h + 1.6],
      pts: [...cube(TERM.x, TERM.y, ZD, TERM.x + TERM.w, TERM.y + TERM.d, ZD + TERM.h + 4), ...cube(RACK.x0, RACK.y0, ZD, rackEnd, RACK.y1, RACK.top + 6)],
      text: `Each 1 KAS coin splits by fixed shares: fee 5 %, referral 10 %, callers 0.5 % each, prize tiers 10 %, house 74 %. Its ticket drops into one of 16 shard tubes.`,
    },
    {
      id: "backers", title: "Backers queue requests", t: [5.9, 5.9], tq: "run", hold: 1.6, side: "left", anchor: [CLOCK.c[0], FY, CLOCK.c[1]],
      pts: [...cube(28, FY, 22, 104, FY + 14, 92), [70, 112, 70]],
      text: `Backers own the vault through shares. Deposits wait in the glass hopper, withdrawal cards in the rack. Each request spends the vault, so the next waits for the 600-DAA clock.`,
    },
    {
      id: "close", title: "close()", t: [5.9, 6.6], tq: "after", hold: 1.2, side: "right", anchor: [RX + 7.6, LEVERS.close.y, LEVERS.close.z], // the pivot hub: the ball moves
      pts: leverBox(LEVERS.close.y),
      text: `After the deadline anyone pulls the lever. close() spends all 16 shards in one transaction, and the caller is paid ${fk(MODEL.amounts.C_close, 2)} KAS.`,
    },
    {
      id: "seal", title: "Counts frozen", t: [6.6, 9.0], tq: "after", hold: 1.2, side: "left", anchor: [(RACK.x0 + rackEnd) / 2, RACK.y1, RACK.top + 1],
      pts: cube(RACK.x0, RACK.y0, ZD, rackEnd, RACK.y1, RACK.top + 16),
      text: `The seal bar drops on all 16 tubes. Ticket counts are fixed before the seed reference exists, so nobody can buy into a known draw.`,
    },
    {
      id: "draw", title: "draw()", t: [9.0, 10.4], tq: "after", hold: 1.4, side: "right", anchor: [RX + 3, 45, 72],
      pts: [...leverBox(LEVERS.draw.y), ...cube(RX, 38, 64, RX + 4, 52, 80)],
      text: `Anyone pulls the second lever. This prototype reads a clearly marked TEST-ONLY seed; a real seed source is parked.`,
    },
    {
      id: "balls", title: "Five balls and a bonus", t: [10.4, 16.3], tq: "after", hold: 1.6, side: "right", anchor: [SLOTS[2], RAIL.y, RAIL.z + 2],
      pts: [...ball(GLOBE.c, GLOBE.r), ...ball(BONUS.c, BONUS.r), ...cube(RAIL.x0, RAIL.y - 6, ZD, RAIL.x1, RAIL.y + 6, ZD + 12)],
      text: `The seed picks 5 main balls from 30 and a bonus from m = ${MODEL.m}. They roll onto the result rail: ${MODEL.win.map((n) => String(n).padStart(2, "0")).join(" ")} + ${String(MODEL.bonus).padStart(2, "0")}.`,
    },
    {
      id: "prizes", title: "Prizes booked", t: [16.3, 17.6], tq: "after", rate: 0.6, hold: 2.2, side: "bottom", anchor: [TRAYS[0].x + TRAYS[0].w / 2, TRAYS[0].y + TRAYS[0].d / 2, TRAYS[0].z + 8],
      pts: [...tray(0), ...tray(4), ...cube(SIGHT.x0, FY, SIGHT.z0, SIGHT.x1, FY + 2, SIGHT.z1)],
      text: `The vault pays the jackpot (${fk(MODEL.vault.J, 2)} KAS) and ${MODEL.n2} held second prizes (${fk(MODEL.vault.G2, 2)} each) into their trays; ticket sales fill the lower tiers.`,
    },
    {
      id: "convert", title: "Backers convert, capped", t: [17.6, 19.0], tq: "after", rate: 0.7, hold: 2.6, side: "left", anchor: [(SIGHT.x0 + SIGHT.x1) / 2, FY, lZ],
      pts: cube(28, FY, 14, DOOR.x1, FY + 22, 92),
      text: `Only now do requests convert, at the new share price ${MB.price0.toFixed(4)} → ${MB.priceDraw.toFixed(4)}. Withdrawals stop at the red line: the vault keeps L = ${fk(MODEL.vault.Lnext, 0)} KAS for the next round.`,
    },
    {
      id: "claim", title: "claim()", t: [19.0, 24.5], tq: "after", hold: 1.6, side: "top", anchor: [READER.x + READER.w / 2, READER.y + READER.d / 2, READER.z + READER.h],
      pts: [...cube(READER.x, READER.y, READER.z, READER.x + READER.w, READER.y + READER.d, READER.z + READER.h + 4), ...tray(0), [PAYOUT_CUP[0], PAYOUT_CUP[1], 0]],
      text: `The reader checks one proof per shard, all 16, so the holder count h = ${MODEL.h} cannot be under-reported. It pays floor(J / ${MODEL.h}) = ${fk(MODEL.prize, 2)} KAS.`,
    },
    {
      id: "settle", title: "settle()", t: [24.5, 29.0], tq: "after", hold: 1.4, side: "right", anchor: [(RECORD.x0 + RECORD.x1) / 2, FY + 5 + RECORD.depth, RECORD.z1],
      pts: [...cube(RECORD.x0, FY, RECORD.z0, RECORD.x1, FY + 5 + RECORD.depth + 2, RECORD.z1), ...cube(BACKER_CUP[0] - 9, BACKER_CUP[1] - 9, 0, BACKER_CUP[0] + 9, BACKER_CUP[1] + 9, 5)],
      text: `draw() wrote the conversion once into this record. Anyone calls settle(): each request takes its share, the last takes the rest, and ${fk(MB.pendingAfter)} shares wait for round 8.`,
    },
    {
      id: "next", title: "Round 8 opens", t: [29.0, 31.5], tq: "after", hold: 2.2, side: "right", full: true, anchor: null,
      text: `The rack empties and a new round starts with m = ${MODEL.mNext} and a fixed jackpot of ${fk(MODEL.vault.Jnext, 0)} KAS. Rounds are strictly ordered.`,
    },
  ];
  DATA.tour = S.map(({ pts, anchor, ...st }) => ({ ...st, box: pts ? boxOf(pts) : null, anchor: anchor ? at(anchor).map(f2) : null }));
}

const CSS = readFileSync(join(HERE, "draw-cabinet.css"), "utf8");
const latheSource = readFileSync(join(KIT, "lathe.mjs"), "utf8").replace(/^import .*$/gm, "").replace(/^export /gm, "");
const latheScript = `const G = (() => {${latheSource}\nreturn { viewOf, frameOf, pointOf, disc, sphereOf, circleOf, dot3, add3, sub3, mul3, unit3, cross3, len3 };})();\n`;
const LIVE = readFileSync(join(HERE, "live.js"), "utf8").replace("__DATA__", JSON.stringify(DATA)) + (TOUR ? readFileSync(join(HERE, "tour.js"), "utf8") : "");

const fk = (n, d = 0) => n.toLocaleString("en-US", { maximumFractionDigits: d });
const MB = MODEL.backers;
const CAPTION =
  `One round of the Kaspa L1 lottery as a draw machine with no operator (an example round; every amount follows the covenants' arithmetic). ` +
  `buy(): each 1 KAS coin splits by fixed shares (fee 5 %, referral 10 %, callers 0.5 % each, prize tiers 10 %, house 74 %) and its ticket lands in one of 16 shard tubes. ` +
  `close(): anyone pulls the lever after the deadline; the seal bar freezes all 16 counts before the seed exists, and the puller is paid. ` +
  `draw(): anyone pulls again; this prototype draws from a TEST-ONLY seed, five main balls from 30 and a bonus from m = ${MODEL.m}. ` +
  `The vault pays the jackpot (${fk(MODEL.vault.J, 2)} KAS) and two held second prizes (${fk(MODEL.vault.G2, 2)} each) into their trays. ` +
  `claim(): the reader checks one proof per shard, all 16, so the holdings count h = 2 cannot be under-reported, and pays floor(J / 2) = ${fk(MODEL.prize, 2)} KAS. ` +
  `Backers own the vault through shares and use the window in its door. Each request spends the vault, so the next must wait until the clock has run 600 DAA; deposits wait in the glass hopper and withdrawal cards in the rack, irrevocably. ` +
  `draw() converts them only after the round's own prizes are booked, at the new share price (${MB.price0.toFixed(4)} → ${MB.priceDraw.toFixed(4)} here, because the jackpot was hit). ` +
  `Withdrawals are capped so the vault keeps the next round's liability L = ${fk(MODEL.vault.Lnext, 2)} KAS (the red line): ${fk(MB.wc)} of ${fk(MB.W)} shares convert and ${fk(MB.pendingAfter)} stay pending, pro rata, for round 8. ` +
  `The conversion is written once into a settlement record (the drawer); settle(), callable by anyone, pays each request its share of what remains, and the last one takes the rest. ` +
  `Prize trays, left to right: jackpot, second, 4 + bonus, 4, 3 + bonus. Coin heights are not to scale.`;

const TOUR_OVERLAY = `<svg class="dc-lead" aria-hidden="true"><line data-lead/><circle data-lead-dot r="3"/></svg><div class="dc-callout" aria-hidden="true" data-callout><span class="dc-callout-step" data-callout-step></span><strong data-callout-title></strong><span data-callout-text></span><span class="dc-callout-bar"><i data-callout-bar></i></span></div><div class="dc-dots" aria-hidden="true" data-dots></div>`;
const body =
  `<style>${CSS}${TOUR ? readFileSync(join(HERE, "tour.css"), "utf8") : ""}</style>` +
  k.plateHtml({
    fig: "Fig 1",
    title: "Kaspa draw cabinet",
    hint: TOUR ? "Space pause · ← → step" : "Click or → next call · ← back",
    readout: "round 7 · open",
    keys: [
      { mark: "lit", label: "What the current call moves: coins, tickets, the seal, balls, proof pins" },
      { mark: "raised", label: "Covenant-held parts: shard rack, globes, prize trays, vault" },
      { mark: "red", label: "Liability line L on the vault's sight glass" },
      { mark: "flat", label: "Backers' window on the vault door: share price, 600-DAA clock, deposit hopper, withdrawal cards, record drawer" },
    ],
    caption: CAPTION,
    body: `<div class="iso-stage dc-stage" tabindex="0" role="group" aria-label="${TOUR ? "Guided tour of the lottery draw cabinet. Space pauses or resumes; the arrow keys step through the calls." : "Lottery draw cabinet. Click, Space or the right arrow moves to the next protocol call; the left arrow goes back."}">${svg}${TOUR ? TOUR_OVERLAY : ""}<span class="dc-sr" role="status" aria-live="polite" data-status></span></div>`,
  });

const name = `draw-cabinet${TOUR ? "-tour" : ""}${THEME === "light" ? "-light" : ""}.html`;
const page = k.pageHtml({ title: "Draw cabinet", theme: THEME, body, script: kitScript() + latheScript + LIVE, width: 820 });
writeFileSync(join(HERE, name), page);
console.log("wrote", name, `${(page.length / 1024).toFixed(0)} KB`, `${items.length} items`, `scale ${KS.toFixed(3)}`);
