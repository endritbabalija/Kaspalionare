
// ---------------------------------------------------------------- guided tour (build --tour): camera stops, pinned callouts
// Runs after live.js in the same module, so it shares render(), the refs and the easing helpers.
// Each stop: the camera travels (pulls back, pans, pushes in), the stop's window of the round plays, then it holds to read.
// Space pauses and resumes the autoplay; ← and → step by hand (and stay on the stop); a click goes to the next stop.
const TS = D.tour;
const VB = [0, 0, ...svg.getAttribute("viewBox").split(/[\s,]+/).slice(2).map(Number)];
const MOVE = 1.6; // camera travel, s
const FADE = 0.3; // callout fade, s
const DRIFT = 0.035; // slow push-in while a stop plays
const RUN_TQ = 6.9; // the backers' clock once every request has landed and the 600-DAA clock has run
const hint = document.querySelector('[data-corner="hint"]');
const box = stage.querySelector("[data-callout]");
const boxStep = box.querySelector("[data-callout-step]");
const boxTitle = box.querySelector("[data-callout-title]");
const boxText = box.querySelector("[data-callout-text]");
const boxBar = box.querySelector("[data-callout-bar]");
const leadSvg = stage.querySelector(".dc-lead");
const lead = leadSvg.querySelector("[data-lead]");
const leadDot = leadSvg.querySelector("[data-lead-dot]");
const dotsEl = stage.querySelector("[data-dots]");
const smoother = (x) => x * x * x * (x * (x * 6 - 15) + 10);

const playLen = (st) => (st.tq === "run" ? RUN_TQ : (st.t[1] - st.t[0]) / (st.rate || 1));
const stayLen = (st) => playLen(st) + st.hold;
function roundAt(st, p) {
  // a stop ends just short of the next one's first moment, so its readout still names its own call
  const end = st.t[1] > st.t[0] ? st.t[1] - 0.02 : st.t[1];
  const t = st.tq === "run" ? st.t[0] : Math.min(end, st.t[0] + p * (st.rate || 1));
  const tq = st.tq === "zero" ? 0 : st.tq === "run" ? Math.min(p, RUN_TQ) : Math.max(t, RUN_TQ);
  return [t, tq];
}

// ---------------------------------------------------------------- camera: an animated viewBox
// the overview pulls back a little and to the lower right, so its callout gets the bare top-left corner
const FULL = [-VB[2] * 0.08, -VB[3] * 0.06, VB[2] * 1.08, VB[3] * 1.08];
const centre = (v) => [v[0] + v[2] / 2, v[1] + v[3] / 2];
const ASPECT = VB[3] / VB[2];
// the view may run past the drawing (bare paper there makes room for the callout), but its centre stays on the drawing
function fitIn([cx, cy], w) {
  w = Math.min(w, FULL[2]);
  const h = w * ASPECT;
  return [clamp(cx, VB[2] * 0.1, VB[2] * 0.9) - w / 2, clamp(cy, VB[3] * 0.1, VB[3] * 0.9) - h / 2, w, h];
}
// fit the camera to a stop: its parts fill the free part of the view, the callout's strip stays clear
const MARGIN = 14;
const ZOOM_MAX = 2.6;
const probe = box.cloneNode(true);
probe.removeAttribute("data-callout");
probe.style.cssText = "visibility:hidden;opacity:0;transform:none";
stage.appendChild(probe);
const views = new Map();
let viewKey = "";
const phone = () => stage.clientWidth < 560;
const sideOf = (st) => (st.full ? "corner" : phone() ? "bottom" : st.side);
function viewOf(i) {
  const st = TS[i];
  if (st.full) return FULL.slice();
  const key = `${stage.clientWidth}x${stage.clientHeight}`;
  if (key !== viewKey) { views.clear(); viewKey = key; }
  if (views.has(i)) return views.get(i);
  probe.querySelector("[data-callout-title]").textContent = st.title;
  probe.querySelector("[data-callout-text]").textContent = st.text;
  const side = sideOf(st);
  const across = side === "left" || side === "right";
  const f = across ? (probe.offsetWidth + MARGIN * 2) / stage.clientWidth : (probe.offsetHeight + MARGIN * 2) / stage.clientHeight;
  const [x0, y0, x1, y1] = st.box;
  const cw = (x1 - x0) * 1.12 + 10;
  const ch = (y1 - y0) * 1.12 + 10;
  let w = across ? Math.max(cw / (1 - f), ch / ASPECT) : Math.max(cw, ch / ASPECT / (1 - f));
  w = clamp(w, VB[2] / ZOOM_MAX, VB[2]);
  const h = w * ASPECT;
  const cx = (x0 + x1) / 2;
  const cy = (y0 + y1) / 2;
  // centre of the view, so the parts sit in the middle of the free part
  const vx = side === "left" ? cx - (f * w) / 2 : side === "right" ? cx + (f * w) / 2 : cx;
  const vy = side === "top" ? cy - (f * h) / 2 : side === "bottom" ? cy + (f * h) / 2 : cy;
  const v = fitIn([vx, vy], w);
  views.set(i, v);
  return v;
}
// travel from a to b: centre and width ease together, and the width swells mid-way by how far the camera goes
function travel(a, b, s) {
  const e = smoother(s);
  const ca = centre(a);
  const cb = centre(b);
  const far = Math.hypot(cb[0] - ca[0], cb[1] - ca[1]) / VB[2];
  const swell = 1 + Math.min(0.5, far * 0.9) * Math.sin(Math.PI * s);
  return fitIn([lerp(ca[0], cb[0], e), lerp(ca[1], cb[1], e)], lerp(a[2], b[2], e) * swell);
}
// while a stop plays the camera keeps drifting in a little, toward the callout's anchor
function drift(st, k) {
  const v = viewOf(TS.indexOf(st));
  if (TS[TS.indexOf(st)].full) return v;
  const c = centre(v);
  const d = DRIFT * ease(k);
  return fitIn([lerp(c[0], st.anchor[0], d), lerp(c[1], st.anchor[1], d)], v[2] * (1 - d));
}
let cam = FULL.slice();
const setCam = (v) => { cam = v; svg.setAttribute("viewBox", v.map((n) => n.toFixed(2)).join(" ")); };

// ---------------------------------------------------------------- state
let cur = 0; // the stop being shown
let shown = -1; // the stop whose words are in the callout
let mode = "move"; // move | stay
let u = 0; // seconds into the mode
let from = cam.slice(); // camera at the start of a move
let auto = true; // advance on its own
let frozen = false; // Space: everything stops
let frame = 0;
let last = 0;
let visible = false;

TS.forEach((st, i) => {
  const b = document.createElement("button");
  b.type = "button";
  b.tabIndex = -1;
  b.title = st.title;
  b.addEventListener("click", (e) => { e.stopPropagation(); go(i, !auto || frozen); });
  dotsEl.appendChild(b);
});
const dots = [...dotsEl.children];

function go(i, manual) {
  cur = (i + TS.length) % TS.length;
  from = cam.slice();
  mode = still.matches ? "stay" : "move";
  u = still.matches ? playLen(TS[cur]) : 0;
  frozen = false;
  auto = !manual;
  status.textContent = `${TS[cur].title}. ${TS[cur].text}`;
  paint();
  runLoop();
}
function togglePause() {
  if (frozen || !auto) { frozen = false; auto = true; } else frozen = true;
  paint();
  runLoop();
}

// ---------------------------------------------------------------- the callout, pinned to its part
function placeCallout(opacity) {
  const st = TS[shown];
  const sr = stage.getBoundingClientRect();
  const vr = svg.getBoundingClientRect();
  const [px, py] = st.anchor || [0, 0];
  const ax = vr.left - sr.left + ((px - cam[0]) / cam[2]) * vr.width;
  const ay = vr.top - sr.top + ((py - cam[1]) / cam[3]) * vr.height;
  const W0 = sr.width;
  const H0 = sr.height;
  const w = box.offsetWidth;
  const h = box.offsetHeight;
  // the callout sits in the strip the camera left free, level with its part where it can
  const side = sideOf(st);
  const M = MARGIN;
  let x = clamp(ax - w / 2, M, W0 - w - M);
  let y = clamp(ay - h / 2, M, H0 - h - M);
  if (side === "left") x = M;
  else if (side === "right") x = W0 - w - M;
  else if (side === "top") y = M;
  else if (side === "bottom") y = H0 - h - M;
  else { x = M; y = M; } // overviews: the bare corner, top left
  box.style.transform = `translate(${x.toFixed(1)}px, ${y.toFixed(1)}px)`;
  box.style.opacity = opacity.toFixed(3);
  // the leader runs from the part to the nearest point on the callout's edge
  const ex = clamp(ax, x, x + w);
  const ey = clamp(ay, y, y + h);
  const inside = ex === ax && ey === ay;
  lead.setAttribute("x1", ax.toFixed(1));
  lead.setAttribute("y1", ay.toFixed(1));
  lead.setAttribute("x2", ex.toFixed(1));
  lead.setAttribute("y2", ey.toFixed(1));
  leadDot.setAttribute("cx", ax.toFixed(1));
  leadDot.setAttribute("cy", ay.toFixed(1));
  leadSvg.style.opacity = inside || !st.anchor ? "0" : opacity.toFixed(3);
}
function fillCallout(i) {
  shown = i;
  const st = TS[i];
  boxStep.textContent = `${String(i + 1).padStart(2, "0")} / ${String(TS.length).padStart(2, "0")}`;
  boxTitle.textContent = st.title;
  boxText.textContent = st.text;
  box.dataset.side = sideOf(st);
}

// ---------------------------------------------------------------- one frame of the tour
function paint() {
  const st = TS[cur];
  let p = 0;
  let opacity = 1;
  if (mode === "move") {
    setCam(travel(from, viewOf(cur), clamp(u / MOVE)));
    // the old words fade out as the camera leaves, the new ones fade in as it arrives
    if (u < FADE && shown !== cur) opacity = 1 - u / FADE;
    else { if (shown !== cur) fillCallout(cur); opacity = clamp((u - (MOVE - FADE)) / FADE); }
  } else {
    if (shown !== cur) fillCallout(cur);
    p = Math.min(u, playLen(st));
    setCam(drift(st, clamp(u / stayLen(st))));
  }
  if (shown < 0) fillCallout(cur);
  const [t, tq] = roundAt(st, p);
  render(t, tq);
  placeCallout(opacity);
  boxBar.style.transform = `scaleX(${(mode === "stay" ? clamp(u / stayLen(st)) : 0).toFixed(3)})`;
  box.toggleAttribute("data-paused", frozen || !auto);
  dots.forEach((d, i) => d.toggleAttribute("data-on", i === cur));
  const h = frozen ? "Paused · Space resume · ← → step" : auto ? "Space pause · ← → step" : "Space autoplay · ← → step";
  if (hint && hint.textContent !== h) hint.textContent = h;
}
function tick(now) {
  const dt = last ? Math.min((now - last) / 1000, 1 / 30) : 1 / 60;
  last = now;
  if (!frozen) {
    u += dt;
    const st = TS[cur];
    if (mode === "move" && u >= MOVE) { mode = "stay"; u -= MOVE; }
    if (mode === "stay" && u >= stayLen(st)) {
      if (auto) { cur = (cur + 1) % TS.length; from = cam.slice(); mode = "move"; u = 0; status.textContent = `${TS[cur].title}. ${TS[cur].text}`; }
      else u = stayLen(st);
    }
  }
  paint();
  const busy = !frozen && (auto || mode === "move" || u < stayLen(TS[cur]));
  frame = visible && !still.matches && busy ? requestAnimationFrame(tick) : 0;
}
function runLoop() { if (!frame && visible && !still.matches) { last = 0; frame = requestAnimationFrame(tick); } }

// ---------------------------------------------------------------- input
new IntersectionObserver((entries) => { visible = entries[entries.length - 1].isIntersecting; if (visible) runLoop(); }, { rootMargin: "120px 0px" }).observe(stage);
stage.addEventListener("click", () => go(cur + 1, !auto || frozen));
addEventListener("keydown", (event) => {
  if (event.altKey || event.ctrlKey || event.metaKey || !visible) return;
  if (/^(INPUT|TEXTAREA|SELECT|BUTTON)$/.test(document.activeElement?.tagName || "")) return;
  if (event.key === " " || event.key === "k") { event.preventDefault(); togglePause(); }
  else if (event.key === "ArrowRight") { event.preventDefault(); go(cur + 1, !auto || frozen); }
  else if (event.key === "ArrowLeft") { event.preventDefault(); go(cur - 1, !auto || frozen); }
});
addEventListener("resize", () => { if (!frame) paint(); });
if (still.matches) { auto = false; mode = "stay"; u = playLen(TS[0]); }
stage.closest(".iso-plate").appendChild(dotsEl); // the stop dots live in the card margin, off the drawing
setCam(viewOf(0));
paint();
window.__tour = { go: (i) => go(i, true), stop: TS, at: (i, s) => { cur = i; shown = -1; frozen = true; mode = "stay"; u = s; paint(); }, state: () => ({ cur, mode, u, auto, frozen }) };
