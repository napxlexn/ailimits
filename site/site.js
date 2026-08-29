/* AI Limits site v2.2 — renderer port + scene direction.
   The widget replica is painted from the app's own constants
   (theme.rs / layout.rs / renderer.rs), including the u8 truncation, so the
   playground lands on mathematically the same colors the app shows.
   Close-ups use CSS zoom (a real re-layout, text stays sharp), never a
   blurry transform scale. All from-states are applied by JS, so no-JS and
   reduced-motion get the complete static page. */
"use strict";

/* ════ theme.rs port ════ */
const PALETTES = {
  default: { low: [[80,155,110],[20,25,20]], mid: [[180,145,50],[30,20,20]], high: [[185,75,75],[25,15,15]] },
  ocean:   { low: [[60,140,180],[30,40,20]], mid: [[50,100,170],[20,30,30]], high: [[20,60,140],[15,20,30]] },
  sunset:  { low: [[200,140,60],[30,20,20]], mid: [[210,80,100],[20,20,20]], high: [[140,40,140],[20,15,20]] },
  forest:  { low: [[80,160,65],[30,35,20]],  mid: [[175,160,38],[25,18,20]], high: [[130,80,28],[18,14,18]] },
  neon:    { low: [[30,180,140],[20,55,35]], mid: [[180,180,18],[55,55,18]], high: [[180,18,140],[55,18,35]] },
  ice:     { low: [[140,200,220],[20,18,14]],mid: [[110,160,200],[18,18,18]],high: [[80,100,150],[14,14,18]] },
  rose:    { low: [[220,150,165],[25,20,20]],mid: [[200,80,110],[20,20,18]], high: [[140,15,55],[18,12,18]] },
  slate:   { low: [[120,155,185],[25,22,20]],mid: [[80,110,145],[18,18,20]], high: [[42,65,95],[15,15,18]] },
};
const u8 = (v) => Math.trunc(Math.min(255, Math.max(0, v)));
function levelColor([base, delta], sat) {
  const r = base[0] + delta[0], g = base[1] + delta[1], b = base[2] + delta[2];
  const lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  const mix = (v) => u8(lum + (v - lum) * sat);
  return { bar: [mix(r), mix(g), mix(b), u8(0.82 * 255)],
           text: [u8(mix(r) * 1.12), u8(mix(g) * 1.12), u8(mix(b) * 1.12), u8(0.88 * 255)] };
}
function computeTheme(st) {
  const bright = Math.min(100, Math.max(20, st.brightness)) / 100;
  const dim = (c) => [c[0], c[1], c[2], u8(c[3] * bright)];
  let low, mid, high;
  if (st.palette === "mono") {
    const t = (a) => [255, 255, 255, u8(a * 255)];
    low  = { bar: [220, 220, 220, 229], text: t(0.60) };
    mid  = { bar: [195, 195, 195, 229], text: t(0.45) };
    high = { bar: [170, 170, 170, 229], text: t(0.35) };
  } else {
    const spec = PALETTES[st.palette] || PALETTES.default;
    const sat = Math.min(100, st.saturation) / 100;
    low = levelColor(spec.low, sat); mid = levelColor(spec.mid, sat); high = levelColor(spec.high, sat);
  }
  const d = (l) => ({ bar: dim(l.bar), text: dim(l.text) });
  return { name: dim([255, 255, 255, 107]), meta: dim([255, 255, 255, 56]),
           track: [255, 255, 255, 2], low: d(low), mid: d(mid), high: d(high) };
}
const rgba = (c) => `rgba(${c[0]},${c[1]},${c[2]},${(c[3] / 255).toFixed(3)})`;
/* Browser text AA composites with sRGB gamma while the app's fontdue path
   blends linearly, so equal alphas render brighter here. Measured against
   the real renderer at brightness 40/70/100 (bars matched exactly, text ran
   up to +12%); a' = a(1 - k a) brings the text luminance back onto the
   renderer's. Applies to TEXT only - bars already match. */
const textA = (a255) => { const a = a255 / 255; const k = 0.34 - 0.28 * a; return u8(255 * a * (1 - k * a)); };
const rgbaT = (c) => rgba([c[0], c[1], c[2], textA(c[3])]);
const levelOf = (pct) => (pct < 60 ? "low" : pct < 80 ? "mid" : "high");

/* ════ layout.rs / renderer.rs port ════ */
const D = 1.11, PAD = 11.1, TOP = 4, HINT = 20 * D;
const ROWS_W = { compact: 212 * D, medium: 250 * D, expanded: 290 * D };
const ROW_H = { compact: 20 * D, medium: 40 * D, expanded: 52 * D };
const WFRAC = { full: 1, threequarters: 0.75, half: 0.5 };
const COL_W = { compact: 46 * D, medium: 54 * D, expanded: 62 * D };
const BODY_H = { compact: 42 * D, medium: 68 * D, expanded: 88 * D };
const COL_PAD = 6 * D, GAP_ROW = 2.5 * D, GAP_STACK = 10 * D;
const COL_SPEC = { compact: [7, 4, 13, 0, 9.5, 0], medium: [8.5, 19, 17, 12.5, 10.5, 0], expanded: [11, 23, 30, 14.5, 11.5, 11] };
const NAME_W = 48, PCT_W = 30, RGAP = 7, MIN_BAR = 24;

function rowTier(detail, rowW) {
  if (detail === "compact") {
    if (rowW - NAME_W - PCT_W - RGAP * 2 >= MIN_BAR) return "full";
    if (rowW - PCT_W - RGAP >= MIN_BAR) return "nameless";
    return "pctonly";
  }
  if (rowW >= NAME_W + PCT_W + RGAP) return "full";
  return rowW >= PCT_W ? "nameless" : "pctonly";
}
function computeSize(st) {
  const n = st.providers.length;
  if (st.layout === "rows") {
    const w = ROWS_W[st.detail] * WFRAC[st.width];
    const t = rowTier(st.detail, w - PAD * 2);
    const hint = st.detail === "compact" && t === "full" ? HINT : 0;
    return [w, TOP + n * ROW_H[st.detail] + hint + 4];
  }
  const cw = COL_W[st.detail], bh = BODY_H[st.detail];
  if (st.layout === "cols-column") return [COL_PAD * 2 + cw, TOP + n * bh + (n - 1) * GAP_STACK + 6];
  return [COL_PAD * 2 + n * cw + (n - 1) * GAP_ROW, TOP + bh + 6];
}
function span(x, y, size, color, text, extra) {
  return `<span class="aw-abs" style="left:${x.toFixed(1)}px;top:${y.toFixed(1)}px;font-size:${size}px;line-height:${Math.round(size * 1.2)}px;color:${color};${extra || ""}">${text}</span>`;
}
function bar(x, y, w, h, color, r) {
  return `<i class="aw-bar" style="left:${x.toFixed(1)}px;top:${y.toFixed(1)}px;width:${Math.max(0, w).toFixed(1)}px;height:${h}px;background:${color};border-radius:${r == null ? 2 : r}px"></i>`;
}
function renderContent(st) {
  const th = computeTheme(st);
  const [W] = computeSize(st);
  let h = "";
  const rowsOf = (p) => {
    const grey = p.stale || p.estimated;
    const lvl = th[levelOf(p.pct)];
    return { barC: rgba(grey ? th.meta : lvl.bar), txtC: rgbaT(grey ? th.meta : lvl.text),
             pctText: (p.estimated ? "≈" : "") + Math.round(p.pct) + "%" };
  };
  if (st.layout === "rows") {
    const rowW = W - PAD * 2, rh = ROW_H[st.detail], t = rowTier(st.detail, rowW);
    st.providers.forEach((p, i) => {
      const y = TOP + i * rh;
      const { barC, txtC, pctText } = rowsOf(p);
      if (st.detail === "compact") {
        const ty = y + (rh - 13) / 2;
        if (t === "full") h += span(PAD, ty, 11, rgbaT(th.name), p.name, `width:${NAME_W}px;overflow:hidden`);
        const bx = t === "full" ? PAD + NAME_W + RGAP : PAD;
        const bw = t === "full" ? rowW - NAME_W - PCT_W - RGAP * 2 : rowW - PCT_W - RGAP;
        if (t !== "pctonly") {
          h += bar(bx, y + (rh - 3) / 2, bw, 3, rgba(th.track));
          h += bar(bx, y + (rh - 3) / 2, Math.max(1, bw * Math.min(1, p.pct / 100)), 3, barC);
          h += span(bx + bw + RGAP, ty, 11, txtC, pctText, `width:${PCT_W}px;text-align:right`);
        } else {
          h += span(PAD, ty, 11, txtC, pctText, `width:${rowW}px;text-align:center`);
        }
      } else {
        const ex = st.detail === "expanded";
        const nameS = ex ? 13 : 12, pctS = ex ? 15 : 13, barH = ex ? 5 : 4, barY = y + (ex ? 26 : 20);
        h += span(PAD, y + 3, nameS, rgbaT(th.name), p.name);
        h += span(PAD, y + 2, pctS, txtC, pctText, `width:${rowW}px;text-align:right`);
        h += bar(PAD, barY, rowW, barH, rgba(th.track));
        h += bar(PAD, barY, Math.max(1, rowW * Math.min(1, p.pct / 100)), barH, barC);
        const metaY = barY + barH + 4, metaS = ex ? 11 : 10.5;
        if (p.meta) h += span(PAD + rowW / 2, metaY, metaS, rgbaT(th.meta), p.meta, `width:${rowW / 2}px;text-align:right`);
        if (ex && p.weekly != null) h += span(PAD, metaY + 0.5, 11, rgbaT(th.meta), `Weekly: ${p.weekly}%`);
      }
    });
    if (st.detail === "compact" && t === "full" && st.providers[0]) {
      const p = st.providers[0];
      h += span(PAD, TOP + st.providers.length * rh + 1, 10.5, rgbaT(th.meta), `${p.name}: ${Math.round(p.pct)}% / ${p.meta || ""}`);
    }
  } else {
    const [bw, topG, botG, pctS, nameS, resetS] = COL_SPEC[st.detail];
    const cw = COL_W[st.detail], bh = BODY_H[st.detail];
    st.providers.forEach((p, i) => {
      const x = st.layout === "cols-column" ? COL_PAD : COL_PAD + i * (cw + GAP_ROW);
      const y = st.layout === "cols-column" ? TOP + i * (bh + GAP_STACK) : TOP;
      const { barC, txtC, pctText } = rowsOf(p);
      if (pctS) h += span(x, y + 1, pctS, txtC, pctText, `width:${cw}px;text-align:center`);
      const bx = x + (cw - bw) / 2, bTop = y + topG, bH = bh - topG - botG;
      h += bar(bx, bTop, bw, bH, rgba(th.track));
      const fh = Math.max(1, Math.round(bH * Math.min(1, p.pct / 100)));
      h += bar(bx, bTop + bH - fh, bw, fh, barC, fh >= bH - 0.5 ? 2 : 0);
      h += span(x, y + bh - (resetS ? 23 : 12), nameS, rgbaT(th.name), p.name, `width:${cw}px;text-align:center`);
      if (resetS && p.meta) h += span(x, y + bh - 10, resetS, rgbaT(th.meta), p.meta, `width:${cw}px;text-align:center`);
    });
  }
  return `<div class="aw-content">${h}</div>`;
}
function renderWidget(el, st, k) {
  if (!el) return;
  const [w, hgt] = computeSize(st);
  el.style.width = w.toFixed(1) + "px";
  el.style.height = hgt.toFixed(1) + "px";
  el.style.setProperty("--op", (st.opacity / 100).toFixed(2));
  if (k) el.style.zoom = k;
  el.innerHTML = renderContent(st);
}

/* ════ demo data ════ */
const HERO_P = [
  { name: "Claude", pct: 93, meta: "2h 22min", weekly: 19 },
  { name: "Codex", pct: 81, meta: "57min" },
  { name: "Copilot", pct: 40, meta: "14h 40min" },
  { name: "Antigravity", pct: 7, meta: "33min" },
];
const baseState = (over) => Object.assign(
  { detail: "medium", layout: "rows", width: "full", palette: "mono", opacity: 45, brightness: 100, saturation: 55, providers: HERO_P },
  over);

const $ = (s, r) => (r || document).querySelector(s);
const $$ = (s, r) => Array.from((r || document).querySelectorAll(s));
const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
const lerp = (a, b, t) => a + (b - a) * t;
const smooth = (t) => t * t * (3 - 2 * t);

/* ════ hero constellation ════ */
renderWidget($("#aw-hero"), baseState({}), innerWidth < 700 ? Math.min(1.25, (innerWidth - 56) / 277.5) : 1.5);
renderWidget($("#aw-sat-a"), baseState({ detail: "compact" }), 0.95);
renderWidget($("#aw-sat-b"), baseState({ detail: "medium", layout: "cols-row" }), 0.95);

/* ════ shapes: one family at a time, main + variants together ════ */
const FAMS = [
  { hud: "DETAIL LEVELS",
    note: "Compact, Medium, Expanded. Switched live from the context menu; the reset countdown and the weekly window join as the row grows.",
    main: { detail: "medium" }, mk: 1.3,
    sats: [{ st: { detail: "compact" }, cap: "COMPACT" }, { st: { detail: "expanded" }, cap: "EXPANDED" }] },
  { hud: "WIDTH STEPS",
    note: "100, 75 and 50 percent of the natural width. Names are dropped whole before anything overlaps: honest degradation.",
    main: { detail: "medium", width: "threequarters" }, mk: 1.3,
    sats: [{ st: { detail: "medium", width: "full" }, cap: "100%" }, { st: { detail: "compact", width: "half" }, cap: "50% · COMPACT" }] },
  { hud: "COLUMNS",
    note: "Vertical bars, side by side or stacked into a narrow tower for a screen edge.",
    main: { detail: "expanded", layout: "cols-row" }, mk: 1.3,
    sats: [{ st: { detail: "medium", layout: "cols-column" }, cap: "STACKED" }] },
];
const shapeEl = $("#aw-shape");
const satEls = [$("#aw-shape-s1"), $("#aw-shape-s2")];
const capEls = [$("[data-cap-s1]"), $("[data-cap-s2]")];
const shapeHud = $("[data-shape-hud]");
const shapeNote = $("[data-shape-note]");
let famRendered = -1;
function setFam(i) {
  if (famRendered === i) return;
  famRendered = i;
  const f = FAMS[i];
  renderWidget(shapeEl, baseState(f.main), f.mk);
  shapeHud.textContent = f.hud;
  shapeNote.textContent = f.note;
  satEls.forEach((el, s) => {
    const fig = el.closest("figure");
    if (f.sats[s]) {
      fig.style.display = "";
      renderWidget(el, baseState(f.sats[s].st), 0.9);
      capEls[s].textContent = f.sats[s].cap;
    } else {
      fig.style.display = "none";
    }
  });
}
setFam(0);

function shapeProgress(p) {
  /* the last family has nothing to turn into, so it would otherwise hold for a
     full slot — a third of the section standing still. It gets half a slot. */
  const n = FAMS.length, slot = 1 / (n - 0.5);
  const idx = Math.min(n - 1, Math.floor(p / slot));
  const inSlot = (p - idx * slot) / slot;
  const HOLD = 0.42;
  const stage = $(".shape-stage");
  if (inSlot <= HOLD || idx >= n - 1) {
    setFam(idx);
    const c = shapeEl.firstElementChild;
    if (c) c.style.opacity = 1;
    if (stage) stage.style.opacity = 1;
    const [w, h] = computeSize(baseState(FAMS[idx].main));
    shapeEl.style.width = w + "px"; shapeEl.style.height = h + "px";
    return;
  }
  const t = smooth((inSlot - HOLD) / (1 - HOLD));
  const a = computeSize(baseState(FAMS[idx].main));
  const b = computeSize(baseState(FAMS[idx + 1].main));
  shapeEl.style.width = lerp(a[0], b[0], t).toFixed(1) + "px";
  shapeEl.style.height = lerp(a[1], b[1], t).toFixed(1) + "px";
  /* a straight cross-dissolve: the old family fades out, the new one fades in,
     and the swap happens at the single point where nothing is on screen. The
     old curve held alpha at zero across a quarter of the transition, so the
     widget stood there as an empty box for a stretch of the scroll. */
  let alpha;
  if (t < 0.5) { setFam(idx); alpha = 1 - t / 0.5; }
  else { setFam(idx + 1); alpha = (t - 0.5) / 0.5; }
  const c = shapeEl.firstElementChild;
  if (c) c.style.opacity = alpha.toFixed(3);
  // the satellites cross-fade with the same curve
  $(".shape-sats").style.opacity = alpha.toFixed(3);
}
if (reduced) setFam(0);

/* ════ honesty cycle ════ */
const honEl = $("#aw-honesty");
const HON_STATES = [
  { name: "Claude", pct: 36, meta: "2h 22min" },
  { name: "Claude", pct: 36, meta: "12 min ago", stale: true },
  { name: "Claude", pct: 0, meta: "est · 21 min ago", estimated: true },
];
let honI = 0;
function honRender() {
  renderWidget(honEl, baseState({ providers: [HON_STATES[honI], { name: "Codex", pct: 81, meta: "57min" }] }));
}
honRender();
if (!reduced) setInterval(() => { honI = (honI + 1) % HON_STATES.length; honRender(); }, 2600);

/* ════ 04 taskbar: the presence in the bar, taken apart and reassembled ════

   The tilt is an ORTHOGRAPHIC projection packed into a plain 2D matrix(). A 3D
   transform would promote every plate to a composited layer, and a promoted
   layer is rasterised once and then sampled by the GPU with no mip chain — at
   this tilt the vertical minification alone chops every diagonal edge, glyph
   stem and thin bar in the render. Kept out of the compositor, Skia rasterises
   each plate with the projection already in the matrix, so the edges are
   analytically anti-aliased and the 2x asset is filtered properly. The flat
   Windows view at the end of the scroll is the same matrix with tilt run to
   zero: cos 0 = 1, sin 0 = 0 — one code path, no seam.
   Nothing in here may use filter/backdrop-filter/will-change, and both canvases
   size their backing store to the projected device footprint, for that reason. */
{
  const stage = $("[data-tb-stage]");
  if (stage && !reduced) {
    const RAD = Math.PI / 180, TAU = Math.PI * 2;
    const BAR_W = 420, BAR_H = 48, ICON_BOX = 44, ICON_C = ICON_BOX / 2;
    const Z = [0, 55, 110, 165];        /* bar, gauge, panel, tooltip */
    const DX = [0, 0, -84, 0];          /* the panel starts pushed left */
    const TARGETS = [93, 81, 40, 7];
    const SCRUB = $(".tb-scrub");
    const rig = $("[data-tb-rig]");
    const plates = [...rig.querySelectorAll(".tb-plate")];
    const barCv = $(".tb-bar"), ticCv = $(".tb-tic");
    const bctx = barCv.getContext("2d"), tctx = ticCv.getContext("2d");
    const panel = $(".tb-panel"), tipbox = $(".tb-tipbox");
    const targets = [barCv, ticCv, panel, tipbox];
    const tags = [0, 1, 2, 3].map((i) => $(`[data-tb-tag="${i}"]`));
    const pvs = [...document.querySelectorAll("[data-tb-pv]")];
    const pbs = [...document.querySelectorAll("[data-tb-pb]")];
    const tns = [...document.querySelectorAll("[data-tb-tn]")];
    const csvg = $("[data-tb-csvg]"), cline = $("[data-tb-cline]");
    const noteBox = $("[data-tb-notebox]");
    const COL_GAP = 56, NOTE_MIN = 32, NOTE_W = 260;
    /* the section rail lives on the right of every viewport wide enough for it,
       and the tag column has to stop before it */
    const railW = () => (innerWidth >= 1181 ? 150 : 24);
    /* The pose has two independent writers: the scroll sets the base (the plane
       turns to face you at the end of the scrub) and the pointer adds a
       parallax offset. Both used to assign `tilt`/`spin` outright, so a scroll
       event landing between two pointer events wiped the parallax and the next
       pointer event put it back — with smooth scrolling still gliding after a
       flick, the two alternated every frame and the object shook between two
       positions instead of settling. Each writer now owns its own term and
       pose() composes them. */
    let tiltBase = 55, spinBase = -10;      /* what the scroll asks for */
    let aimX = 0, aimY = 0;                 /* what the pointer asks for */
    let offX = 0, offY = 0;                 /* what is actually drawn */
    let chasing = false;
    let tilt = 55, spin = -10, PROJ = null, activeEl = null;
    let fill = 0, spread = 1, SCALE = 1, tipOut = 0;

    /* the shared bar render carries the Ukrainian language label; Windows shows
       three latin letters, so it is repainted once into an offscreen copy.
       Metrics measured off the asset, which is 2x: ink x 528..566, baseline 53. */
    const barImg = new Image();
    let barSrc = null;
    barImg.onload = () => {
      const c = document.createElement("canvas");
      c.width = barImg.naturalWidth; c.height = barImg.naturalHeight;
      const g = c.getContext("2d");
      g.drawImage(barImg, 0, 0);
      g.fillStyle = "rgb(24,25,27)";
      g.fillRect(510, 26, 68, 44);
      g.font = '23px "Segoe UI", sans-serif';
      g.textAlign = "right"; g.textBaseline = "alphabetic";
      g.fillStyle = "rgba(255,255,255,.80)";
      g.fillText("ENG", 567, 53);
      barSrc = c;
      if (geo) apply();
    };
    barImg.src = "./media/bar-tray-slot.png";

    const q = (x) => Math.round(x * 20) / 20;
    let barKey = "", ticKey = "";
    function drawBar(sq) {
      if (!barSrc) return;
      const dpr = Math.min(2, Math.max(1, devicePixelRatio || 1)) * q(SCALE);
      const w = Math.round(BAR_W * dpr), h = Math.max(2, Math.round(BAR_H * q(sq) * dpr));
      const key = w + "x" + h;
      if (key === barKey) return;
      barKey = key;
      if (barCv.width !== w || barCv.height !== h) { barCv.width = w; barCv.height = h; }
      bctx.setTransform(w / BAR_W, 0, 0, h / BAR_H, 0, 0);
      bctx.clearRect(0, 0, BAR_W, BAR_H);
      bctx.imageSmoothingEnabled = true;
      bctx.imageSmoothingQuality = "high";
      bctx.drawImage(barSrc, 0, 0, 840, 96, 0, 0, BAR_W, BAR_H);
    }
    function drawIcon(sq, f1, f2) {
      const dpr = Math.min(2, Math.max(1, devicePixelRatio || 1)) * q(SCALE);
      const w = Math.round(ICON_BOX * dpr), h = Math.max(2, Math.round(ICON_BOX * q(sq) * dpr));
      const key = w + "x" + h + ":" + f1 + ":" + f2;
      if (key === ticKey) return;
      ticKey = key;
      if (ticCv.width !== w || ticCv.height !== h) { ticCv.width = w; ticCv.height = h; }
      else tctx.setTransform(1, 0, 0, 1, 0, 0), tctx.clearRect(0, 0, w, h);
      tctx.setTransform(w / ICON_BOX, 0, 0, h / ICON_BOX, 0, 0);
      tctx.clearRect(0, 0, ICON_BOX, ICON_BOX);
      tctx.save();
      tctx.translate(ICON_C, ICON_C);
      tctx.lineCap = "round";
      /* the gauge draws its own shadow: a CSS filter would spawn a render
         surface and put the plate back on the texture path */
      tctx.shadowColor = "rgba(0,0,0,.5)"; tctx.shadowBlur = 5; tctx.shadowOffsetY = 2;
      tctx.lineWidth = 2.0; tctx.strokeStyle = "rgba(236,240,244,.18)";
      tctx.beginPath(); tctx.arc(0, 0, 7.6, 0, TAU); tctx.stroke();
      if (f1 > 0.5) {
        tctx.strokeStyle = "#ececec";
        tctx.beginPath(); tctx.arc(0, 0, 7.6, -Math.PI / 2, -Math.PI / 2 + (f1 / 100) * TAU); tctx.stroke();
      }
      tctx.lineWidth = 1.8; tctx.strokeStyle = "rgba(236,240,244,.16)";
      tctx.beginPath(); tctx.arc(0, 0, 3.7, 0, TAU); tctx.stroke();
      if (f2 > 0.5) {
        tctx.strokeStyle = "#ececec";
        tctx.beginPath(); tctx.arc(0, 0, 3.7, -Math.PI / 2, -Math.PI / 2 + (f2 / 100) * TAU); tctx.stroke();
      }
      tctx.restore();
    }

    const visBox = (el) => {
      const rows = [...el.querySelectorAll(".tb-row")];
      if (rows.length) {
        const t = Math.min(...rows.map((r) => r.offsetTop));
        const b = Math.max(...rows.map((r) => r.offsetTop + r.offsetHeight));
        return [el.offsetLeft, el.offsetTop + t, el.offsetWidth, b - t];
      }
      const pad = el === ticCv ? 13 : 0;   /* the gauge is 17px inside its box */
      return [el.offsetLeft + pad, el.offsetTop + pad,
              el.offsetWidth - 2 * pad, el.offsetHeight - 2 * pad];
    };
    /* offsetLeft/offsetTop and friends are layout reads, and they were being
       taken on every scroll event right after the transforms had been written —
       a forced synchronous layout per event, twelve of them per pass. None of
       these boxes move when the plates are transformed, so they are measured
       once and re-measured only when the layout can actually have changed. */
    let geo = null;
    function measure() {
      geo = {
        ox: rig.offsetLeft, oy: rig.offsetTop,
        cx: rig.offsetWidth / 2, cy: rig.offsetHeight / 2,
        box: targets.map(visBox),
        tagW: Math.max(...tags.map((t) => t.offsetWidth)),
      };
    }

    const quadOf = (i) => {
      const { a, b, c, d, lift, ox, oy, cx, cy } = PROJ;
      const [l, t, w, h] = geo.box[i];
      return [[l, t], [l + w, t], [l + w, t + h], [l, t + h]].map(([x, y]) => {
        const u = x - cx, v = y - cy;
        return [ox + cx + a * u + c * v + DX[i] * spread * SCALE,
                oy + cy + b * u + d * v - Z[i] * spread * lift];
      });
    };
    const ptSeg = (p, a, b) => {
      const dx = b[0] - a[0], dy = b[1] - a[1], L = dx * dx + dy * dy;
      let t = L ? ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / L : 0;
      t = Math.max(0, Math.min(1, t));
      return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
    };
    const inQuad = (pt, qd) => {
      let inside = false;
      for (let i = 0, j = 3; i < 4; j = i++) {
        const [xi, yi] = qd[i], [xj, yj] = qd[j];
        if ((yi > pt[1]) !== (yj > pt[1]) &&
            pt[0] < ((xj - xi) * (pt[1] - yi)) / (yj - yi) + xi) inside = !inside;
      }
      return inside;
    };
    const clearance = (pts, quads) => {
      let m = Infinity;
      for (let i = 0; i < pts.length; i++) {
        if (quads.some((qd) => inQuad(pts[i], qd))) return 0;
        if (i === pts.length - 1) break;
        for (const qd of quads) {
          for (let k = 0; k < 4; k++) {
            const p1 = pts[i], p2 = pts[i + 1], q1 = qd[k], q2 = qd[(k + 1) % 4];
            m = Math.min(m, Math.min(ptSeg(p1, q1, q2), ptSeg(p2, q1, q2),
                                     ptSeg(q1, p1, p2), ptSeg(q2, p1, p2)));
          }
        }
      }
      return m;
    };

    function apply() {
      if (!geo) measure();
      const sr = stage.getBoundingClientRect();
      /* the rig takes whatever the viewport leaves between the note column and
         the tag column: more screen pixels per plate is the only real cure for
         a soft projection — tilted 55 degrees a 48px bar owns 27 device rows */
      const room = sr.width - (NOTE_MIN + NOTE_W + 90) - (railW() + 190);
      SCALE = Math.max(1, Math.min(1.9, room / (BAR_W * 1.06)));
      const sq = Math.cos(tilt * RAD), lift = Math.sin(tilt * RAD) * SCALE, th = spin * RAD;
      const a = Math.cos(th) * SCALE, b = sq * Math.sin(th) * SCALE;
      const c = -Math.sin(th) * SCALE, d = sq * Math.cos(th) * SCALE;
      const m = `matrix(${a.toFixed(5)},${b.toFixed(5)},${c.toFixed(5)},${d.toFixed(5)},0,0)`;
      PROJ = { a, b, c, d, lift, ox: geo.ox, oy: geo.oy, cx: geo.cx, cy: geo.cy };
      /* centring by measurement: the stack grows upward from the bar, so its
         visual middle wanders as the layers collapse and the tilt flattens */
      const set = (ty) => plates.forEach((p, i) => {
        p.style.transform = `translate(${(DX[i] * spread * SCALE).toFixed(2)}px,` +
          `${(ty - Z[i] * spread * lift).toFixed(2)}px) ${m}`;
      });
      const bounds = [0, 1, 2, 3].map(quadOf).flat().map((p) => p[1]);
      const ty = sr.height / 2 - (Math.min(...bounds) + Math.max(...bounds)) / 2;
      set(ty);
      PROJ.oy = geo.oy + ty;
      if (!fast) drawBar(sq);
      const cur = TARGETS.map((t) => Math.round(t * fill));
      pvs.forEach((el, i) => { el.textContent = cur[i] + "%"; });
      pbs.forEach((el, i) => { el.style.width = cur[i] + "%"; });
      tns.forEach((el, i) => { el.textContent = cur[i] + "%"; });
      if (!fast) drawIcon(sq, cur[0], cur[1]);
      else if (!settle) settle = setTimeout(() => { settle = 0; fast = false; paint(); }, 90);
      /* the tooltip is a hover popup, not a permanent part of the bar. It goes
         on its own schedule, the way it does in the S1 take: a plain fade that
         starts as the limits finish filling and is over early in the assembly,
         so the stack lands with the panel and the gauge alone. */
      tipbox.style.setProperty("--o", (1 - tipOut).toFixed(3));
      layoutTags(sr);
      if (activeEl) route(activeEl, false);
    }

    function layoutTags(sr) {
      const quads = [0, 1, 2, 3].map(quadOf);
      const asmR = Math.max(...quads.flat().map((p) => p[0]));
      const tagW = geo.tagW;
      const colX = Math.min(sr.width - railW() - tagW, Math.round(asmR + COL_GAP));
      /* centred on the layer, not on its rightmost corner: with the rig turned
         the far corner is the BOTTOM one, which dropped every tag below the
         thing it names */
      const ys = quads.map((qd) => qd.reduce((a, p) => a + p[1], 0) / 4);
      for (let i = 1; i < ys.length; i++) ys[i] = Math.min(ys[i], ys[i - 1] - 26);
      tags.forEach((tag, i) => {
        tag.style.left = colX + "px";
        tag.style.top = (Math.max(14, Math.min(sr.height - 14, ys[i])) - 8) + "px";
        /* opacity is the legend's own business now (--legend, set in pose) */
      });
    }

    /* the hover description leaves on the LEFT, on an axis-aligned leader that
       starts beside the layer and takes whichever lane is roomiest */
    function route(el, animate) {
      const sr = stage.getBoundingClientRect();
      const quads = [0, 1, 2, 3].map(quadOf);
      const idx = targets.indexOf(el);
      const all = quads.flat();
      const asmL = Math.min(...all.map((p) => p[0]));
      const asmT = Math.min(...all.map((p) => p[1]));
      const asmB = Math.max(...all.map((p) => p[1]));
      const C = quads[idx].reduce((m, p) => (p[0] < m[0] ? p : m));
      const S = [C[0] - 12, C[1]];
      /* the note sits just left of the model, so the leader is short by
         construction rather than stretching across the window */
      const noteL = Math.max(NOTE_MIN, Math.round(asmL - 90 - noteBox.offsetWidth));
      noteBox.style.left = noteL + "px";
      const endX = noteL + noteBox.offsetWidth + 22;
      const bendX = Math.max(endX + 12, Math.min(asmL - 24, S[0] - 18));
      const dir = S[1] < sr.height / 2 ? 1 : -1;
      const noteY = Math.min(sr.height - 130, Math.max(34, S[1] + dir * 96));
      noteBox.style.top = (noteY - 10) + "px";
      const jx = S[0] - 16;
      const lane = (ly) => [S, [jx, S[1]], [jx, ly], [bendX, ly], [bendX, noteY], [endX, noteY]];
      const cands = [
        [S, [bendX, S[1]], [bendX, noteY], [endX, noteY]],
        lane(S[1] - 16), lane(S[1] + 16), lane(S[1] - 30), lane(S[1] + 30),
        lane(asmT - 24), lane(asmB + 24),
      ];
      let pts = cands[0], best = -1;
      cands.forEach((cd) => { const g = clearance(cd, quads); if (g > best) { best = g; pts = cd; } });
      cline.setAttribute("points", pts.map((p) => `${p[0].toFixed(1)},${p[1].toFixed(1)}`).join(" "));
      csvg.setAttribute("viewBox", `0 0 ${sr.width} ${sr.height}`);
      csvg.setAttribute("width", sr.width); csvg.setAttribute("height", sr.height);
      if (!animate) { cline.style.transition = "none"; cline.style.strokeDasharray = "none"; return; }
      let len = 0;
      for (let i = 1; i < pts.length; i++) len += Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]);
      cline.style.transition = "none";
      cline.style.strokeDasharray = len;
      cline.style.strokeDashoffset = len;
      cline.getBoundingClientRect();
      cline.style.transition = "stroke-dashoffset .35s ease";
      cline.style.strokeDashoffset = 0;
    }

    targets.forEach((el, i) => {
      el.addEventListener("pointerenter", () => {
        if (spread < 0.25) return;            /* assembled: nothing to explain */
        activeEl = el;
        stage.classList.add("hovering");
        tags.forEach((t, k) => t.classList.toggle("hot", k === i));
        noteBox.innerHTML = el.dataset.tbNote;
        noteBox.classList.add("on");
        route(el, true);
      });
      el.addEventListener("pointerleave", () => {
        activeEl = null;
        stage.classList.remove("hovering");
        tags.forEach((t) => t.classList.remove("hot"));
        noteBox.classList.remove("on");
        cline.setAttribute("points", "");
      });
    });
    /* the parallax fades out as the layers assemble, so the flat end state is
       exactly flat wherever the pointer happens to have been left */
    function pose() {
      const par = Math.min(1, Math.max(0, (spread - 0.05) / 0.55));
      tilt = tiltBase + offY * par;
      spin = spinBase + offX * par;
      /* the legend belongs to the exploded view: it fades on the same curve the
         layers travel on, so tags, leader and description leave together */
      stage.style.setProperty("--legend", par.toFixed(3));
      apply();
    }
    /* a flick of the pointer should not snap the plane across: chase the target
       and stop the moment it is reached, so nothing runs while nothing moves */
    function chase() {
      offX += (aimX - offX) * 0.2;
      offY += (aimY - offY) * 0.2;
      if (Math.abs(aimX - offX) < 0.02 && Math.abs(aimY - offY) < 0.02) {
        offX = aimX; offY = aimY; chasing = false;
      }
      pose();
      if (chasing) requestAnimationFrame(chase);
    }
    function chaseOn() { if (!chasing) { chasing = true; requestAnimationFrame(chase); } }

    stage.addEventListener("pointermove", (e) => {
      if (spread < 0.25) return;
      const r = stage.getBoundingClientRect();
      aimY = ((e.clientY - r.top) / r.height - 0.5) * -14;
      aimX = ((e.clientX - r.left) / r.width - 0.5) * 16;
      chaseOn();
    });

    /* the scroll: limits fill, the layers land, the plane turns to face you,
       and past the pin the finished bar fades out with the page */
    const ease = (t) => 1 - Math.pow(1 - t, 3);
    const seg = (p, a, b) => Math.min(1, Math.max(0, (p - a) / (b - a)));
    /* main reserves room for the section rail on the right, so the usual
       full-bleed trick (margin-left: 50% - 50vw) lands half that padding off.
       The section is pulled back onto the viewport's own left edge by
       measurement instead of by arithmetic that has to know about the rail. */
    function bleed() {
      SCRUB.style.marginLeft = "0px";
      const off = SCRUB.getBoundingClientRect().left;
      SCRUB.style.marginLeft = (-off) + "px";
    }
    let lastY = scrollY, fast = false;
    function paint() {
      /* travelling: leave the scene as it is and cut it once, on arrival */
      if (document.documentElement.dataset.jump) return;
      const dy = Math.abs(scrollY - lastY);
      lastY = scrollY;
      fast = dy > innerHeight * 0.12;
      if (!fast && settle) { clearTimeout(settle); settle = 0; }
      const r = SCRUB.getBoundingClientRect();
      const denom = Math.max(1, r.height - stage.offsetHeight);
      const pinned = Math.min(1, Math.max(0, -r.top / denom));
      const after = Math.min(1, Math.max(0, (-r.top - denom) / (stage.offsetHeight * 0.38)));
      const eInOut = (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
      fill = ease(seg(pinned, 0.02, 0.5));
      tipOut = eInOut(seg(pinned, 0.50, 0.62));
      spread = 1 - ease(seg(pinned, 0.55, 0.88));
      tiltBase = 55 * (1 - ease(seg(pinned, 0.78, 1)));
      spinBase = -10 * (1 - ease(seg(pinned, 0.78, 1)));
      rig.style.opacity = (1 - after).toFixed(3);
      if (after > 0.001) { noteBox.classList.remove("on"); cline.setAttribute("points", ""); }
      pose();
    }
    /* The handler used to run on every scroll event anywhere on the page, so
       reading any other section paid for this one; and Lenis can deliver more
       than one event per frame, so a jump through the section ran the whole
       pass several times over per frame. Now: only while the scene is near the
       viewport, and at most once per frame. */
    let queued = false, live = false, settle = 0;
    function onScroll() {
      if (!live || queued) return;
      queued = true;
      requestAnimationFrame(() => { queued = false; paint(); });
    }
    addEventListener("scroll", onScroll, { passive: true });
    addEventListener("resize", () => { measure(); bleed(); paint(); });
    new IntersectionObserver((es) => {
      live = es[0].isIntersecting;
      if (live) paint();
    }, { rootMargin: "300px" }).observe(SCRUB);
    bleed();
    measure();
    paint();
  }
}

/* ════ 02 sources: the cards are read, not lit ════
   Scroll drives a front of focus across the row. Ahead of it a card is out of
   focus; behind it, resolved for good. The focus is not switched at the front
   but earned across a distance: the card's body exists at four depths of blur
   at once and the copies hand focus over to one another in cross-fading bands,
   so exactly one card's worth of ink sits in any column — stack them plainly
   and the blurred text glows out from under the sharp one. In that same
   transition, and only there, a stream of hex and binary runs along the rows,
   as if the values were being taken off the card. */
{
  const row = $(".providers .panes");
  const sec = row ? row.closest("section") : null;
  const panes = row ? [...row.querySelectorAll(".pane")] : [];
  const D = { blur: 3, flow: 60, glyph: 9, glyphA: 0.5, ramp: 150, dens: 0.77 };
  const HEX = "0123456789ABCDEF";
  const sm = (t) => t * t * (3 - 2 * t);
  const cl = (x) => (x < 0 ? 0 : x > 1 ? 1 : x);
  /* a stable pseudo-random per (row, value id, channel): a value must keep its
     identity while it travels, otherwise the stream reads as noise */
  const hash = (a, b, c) => {
    let x = (a * 374761393 + b * 668265263 + c * 2246822519) | 0;
    x = Math.imul(x ^ (x >>> 13), 1274126177);
    return ((x ^ (x >>> 16)) >>> 0) / 4294967296;
  };

  if (panes.length && !reduced) {
    panes.forEach((pane) => {
      const body = pane.querySelector(".body");
      for (let i = 0; i < 3; i++) {
        const c = body.cloneNode(true);
        c.classList.add("step");
        c.setAttribute("aria-hidden", "true");
        pane.appendChild(c);
      }
      const cv = document.createElement("canvas");
      cv.className = "glyphs";
      cv.setAttribute("aria-hidden", "true");
      pane.appendChild(cv);
    });

    const decode = (pane, pr, rr, x, live) => {
      const cv = pane.querySelector(".glyphs");
      const layers = pane.__layers || (pane.__layers =
        [pane.querySelector(".body"), ...pane.querySelectorAll(".body.step")]);
      if (pane.__ox === undefined) pane.__ox = layers[0].offsetLeft;
      const local = x - (pr.left - rr.left);
      const ramp = D.ramp, n = layers.length, w = ramp / n;
      const bound = (k) => local + ramp * (0.5 - k / n);
      const ox = pane.__ox;                            /* stops are box-relative */
      const st = (v) => (v - ox).toFixed(1) + "px";
      layers.forEach((el, k) => {
        el.style.filter = "blur(" + (D.blur * Math.pow((n - 1 - k) / (n - 1), 1.5)).toFixed(2) + "px)";
        const parts = [];
        if (k < n - 1) { const b = bound(k + 1); parts.push("transparent " + st(b - w / 2), "#000 " + st(b + w / 2)); }
        if (k > 0) { const b = bound(k); parts.push("#000 " + st(b - w / 2), "transparent " + st(b + w / 2)); }
        el.style.webkitMaskImage = el.style.maskImage =
          parts.length ? "linear-gradient(90deg," + parts.join(",") + ")" : "none";
      });

      const dpr = Math.min(2, Math.max(1, devicePixelRatio || 1));
      const dw = Math.round(pr.width * dpr), dh = Math.round(pr.height * dpr);
      if (cv.width !== dw) cv.width = dw;
      if (cv.height !== dh) cv.height = dh;
      const g = cv.getContext("2d");
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      g.clearRect(0, 0, pr.width, pr.height);
      const half = ramp * 0.55;
      if (!live || local < -half || local > pr.width + half) return;
      g.font = D.glyph + 'px "Cascadia Code", Consolas, monospace';
      g.textBaseline = "middle";
      /* the grid is one CHARACTER wide, not one value wide: on a value-wide
         grid a single bit always sat alone in a slot as broad as a hex pair,
         and the binary read as scattered digits instead of a bit stream */
      const cw = g.measureText("0").width, lead = D.glyph * 1.15;
      const t = performance.now() / 1000;
      const cols = Math.ceil(pr.width / cw) + 3;
      for (let j = 0; (j + 0.6) * lead < pr.height; j++) {
        const gy = (j + 0.6) * lead;
        const sp = D.flow * (0.55 + 0.9 * hash(j, 0, 3));
        const trav = t * sp / cw, base = Math.floor(trav), off = (trav - base) * cw;
        for (let i = -2; i < cols; i++) {
          const id = i - base, blk = Math.floor(id / 2), sub = id - blk * 2;
          if (hash(j, blk, 0) > D.dens) continue;
          const gx = i * cw + off, d = Math.abs(gx - local);
          if (d > half) continue;
          const ef = sm(cl(Math.min(gx, pr.width - gx) / (D.glyph * 2)));
          if (ef <= 0) continue;
          const r2 = hash(j, blk, 1);
          const ch = r2 < 0.5 ? (hash(j, id, 5) < 0.5 ? "0" : "1")
                              : HEX[((r2 * (sub ? 15991 : 997)) | 0) & 15];
          g.fillStyle = "rgba(236,240,244," +
            (D.glyphA * sm(1 - d / half) * ef * (0.35 + hash(j, blk, 2) * 0.65)).toFixed(3) + ")";
          g.fillText(ch, gx, gy);
        }
      }
    };

    let running = false;
    const tick = () => {
      const rr = row.getBoundingClientRect();
      /* The pass belongs to the row's arrival: it starts as the cards come in
         at the bottom edge and finishes as the section itself settles into the
         screen — which is exactly where the cards stand when you arrive from
         the rail, so the reading is complete however you got here. Ending it
         any earlier left the last cards resolving off in the lower corner. */
      const secTop = sec.getBoundingClientRect().top;
      const endTop = Math.min(innerHeight * 0.55, rr.top - secTop);
      const raw = cl((innerHeight - rr.top) / Math.max(1, innerHeight - endTop));
      /* the front starts before the row and ends past it: stopping it on the
         edge would leave the last card's right margin never fully resolved */
      const x = sm(raw) * (rr.width + D.ramp * 1.4) - D.ramp * 0.7;
      const live = raw > 0.002 && raw < 0.998;
      panes.forEach((pane) => decode(pane, pane.getBoundingClientRect(), rr, x, live));
      if (running) requestAnimationFrame(tick);
    };
    /* it only costs anything while the row is on screen */
    new IntersectionObserver((es) => es.forEach((e) => {
      if (e.isIntersecting && !running) { running = true; requestAnimationFrame(tick); }
      else if (!e.isIntersecting && running) { running = false; tick(); }
    }), { rootMargin: "120px" }).observe(row);
  }
}

/* ════ playground ════ */
const playEl = $("#aw-play");
const pgState = baseState({
  providers: [
    { name: "Claude", pct: 93, meta: "2h 22min", weekly: 19 },
    { name: "Codex", pct: 81, meta: "57min" },
    { name: "Copilot", pct: 61, meta: "14h 40min" },
    { name: "Antigravity", pct: 40, meta: "33min" },
  ],
});
function pgRender() {
  renderWidget(playEl, pgState, 1.15);
  const widthGroup = $('[data-only="rows"]');
  if (widthGroup) {
    if (pgState.layout === "rows") widthGroup.removeAttribute("data-hidden");
    else widthGroup.setAttribute("data-hidden", "");
  }
}
pgRender();
$$(".pg .seg").forEach((seg) => {
  const opt = seg.getAttribute("data-opt");
  seg.addEventListener("click", (e) => {
    const btn = e.target.closest("button");
    if (!btn) return;
    $$("button", seg).forEach((b) => b.setAttribute("aria-pressed", b === btn ? "true" : "false"));
    pgState[opt] = btn.getAttribute("data-v");
    pgRender();
  });
});
$$("[data-opt-range]").forEach((r) => {
  r.addEventListener("input", () => {
    const k = r.getAttribute("data-opt-range");
    pgState[k] = +r.value;
    const out = $(`[data-out="${k}"]`);
    if (out) out.textContent = r.value;
    pgRender();
  });
});
/* A press anywhere on the playground must not start a selection: the rail is
   controls and the desk is a thing to drag, and `user-select: none` alone is
   not enough — a drag begun on an unselectable area still lets the browser
   anchor the selection in the content that follows it. Suppressing the default
   on mousedown stops that at the source, and interactive elements keep theirs
   so buttons still focus and sliders still take the pointer. */
{
  const pg = $(".playground .pg");
  if (pg) pg.addEventListener("mousedown", () => {
    /* block only the selection, for as long as this press lasts: preventing
       the default on mousedown itself would also cost the controls their
       focus, and blocking it on the container alone would not help, since a
       drag out of the playground anchors the selection in whatever follows */
    const stop = (e) => e.preventDefault();
    document.addEventListener("selectstart", stop);
    addEventListener("mouseup", () => document.removeEventListener("selectstart", stop), { once: true });
  });
}

(function drag() {
  const desk = $("[data-pg-desk]");
  if (!desk || !playEl) return;
  let sx, sy, ox, oy, dragging = false;
  playEl.addEventListener("pointerdown", (e) => {
    dragging = true; playEl.classList.add("dragging");
    playEl.setPointerCapture(e.pointerId);
    sx = e.clientX; sy = e.clientY;
    const r = playEl.getBoundingClientRect(), d = desk.getBoundingClientRect();
    ox = r.left - d.left; oy = r.top - d.top;
    playEl.style.transform = "none";
    playEl.style.left = ox + "px"; playEl.style.top = oy + "px";
  });
  playEl.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    const d = desk.getBoundingClientRect(), r = playEl.getBoundingClientRect();
    const nx = Math.min(d.width - r.width, Math.max(0, ox + e.clientX - sx));
    const ny = Math.min(d.height - r.height, Math.max(0, oy + e.clientY - sy));
    playEl.style.left = nx + "px"; playEl.style.top = ny + "px";
  });
  const up = () => { dragging = false; playEl.classList.remove("dragging"); };
  playEl.addEventListener("pointerup", up); playEl.addEventListener("pointercancel", up);
})();

/* ════ copy + typing + counts ════ */
const typingRun = new WeakMap();
function typeInto(el, delay) {
  if (!el || reduced) return;
  const text = el.getAttribute("data-type");
  /* claim the element: any run still in flight sees a newer token and stops */
  const run = (typingRun.get(el) || 0) + 1;
  typingRun.set(el, run);
  el.textContent = "";
  let i = 0;
  /* a long command should not take four seconds to appear: the cadence is set
     from the length so every line lands in about the same time, jittered so it
     still reads as typing rather than as a progress bar */
  const base = Math.max(12, Math.min(50, 1400 / text.length));
  setTimeout(function step() {
    if (typingRun.get(el) !== run) return;
    el.textContent = text.slice(0, ++i);
    if (i < text.length) setTimeout(step, base * (0.7 + Math.random() * 0.6));
  }, delay);
}
function copyHandler(text, btnRef) {
  return (e) => {
    const btn = btnRef || e.currentTarget;
    navigator.clipboard.writeText(text).then(() => {
      const old = btn.textContent;
      btn.textContent = "Copied";
      setTimeout(() => { btn.textContent = old; }, 1400);
    });
  };
}
$$("[data-cmd]").forEach((box) => {
  const code = $("code", box), btn = $("button", box);
  btn.addEventListener("click", copyHandler(code.textContent, btn));
});
/* ════ the install terminal ════
   One tab per way of installing that actually works today: the winget package,
   the digest-checking one-liner, and the Scoop bucket. Switching a tab retypes
   the command it carries, and Copy always takes the one on screen. */
const termCode = $("[data-term] .typed");
const termPanel = $("#term-panel");
const termTabs = $$("[data-term] .term-tab");
function showTermTab(tab, retype) {
  if (retype && tab.getAttribute("aria-selected") === "true") return;
  const cmd = tab.getAttribute("data-term-cmd");
  termTabs.forEach((t) => {
    const on = t === tab;
    t.setAttribute("aria-selected", on ? "true" : "false");
    t.tabIndex = on ? 0 : -1;
  });
  if (termPanel) termPanel.setAttribute("aria-labelledby", tab.id);
  termCode.setAttribute("data-type", cmd);
  if (retype && !reduced) typeInto(termCode, 60);
  else termCode.textContent = cmd;
}
if (termCode && termTabs.length) {
  termTabs.forEach((tab, i) => {
    tab.addEventListener("click", () => showTermTab(tab, true));
    /* a tablist is one stop in the tab order; the arrows move within it */
    tab.addEventListener("keydown", (e) => {
      const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
      if (!d) return;
      e.preventDefault();
      const next = termTabs[(i + d + termTabs.length) % termTabs.length];
      next.focus();
      showTermTab(next, true);
    });
  });
}
if (termCode) {
  $("[data-copy-cmd]").addEventListener("click", (e) =>
    copyHandler(termCode.getAttribute("data-type"), e.currentTarget)(e));
}

const nio = new IntersectionObserver((es) => es.forEach((e) => {
  if (!e.isIntersecting) return;
  nio.unobserve(e.target);
  const b = e.target;
  const target = parseFloat(b.getAttribute("data-count"));
  const dec = +b.getAttribute("data-decimals");
  if (reduced) { b.textContent = target.toFixed(dec); return; }
  const t0 = performance.now();
  (function tick(now) {
    const t = Math.min(1, (now - t0) / 1100);
    b.textContent = (target * smooth(t)).toFixed(dec);
    if (t < 1) requestAnimationFrame(tick);
  })(t0);
}), { threshold: 0.01, rootMargin: "0px 0px -12% 0px" });
$$("[data-count]").forEach((b) => nio.observe(b));

if (!reduced && termCode) {
  const tio = new IntersectionObserver((es) => {
    if (!es[0].isIntersecting) return;
    tio.disconnect();
    typeInto(termCode, 300);
  }, { threshold: 0.5 });
  tio.observe($("[data-term]"));
}

/* ════ section rail ════
   Exactly one section is current at any scroll position: the last one whose
   top has crossed the reading line. Deciding that per section, with a trigger
   each, went wrong in both directions — a gap between two sections left
   nothing lit, and a tall pinned scene (the taskbar scrub runs 240vh) stayed
   lit long after the next section had taken the screen, so the rail pointed at
   something other than what was in front of the reader. */
{
  const items = $$(".rail a")
    .map((a) => ({ a, el: document.getElementById(a.getAttribute("data-rail")) }))
    .filter((i) => i.el);
  if (items.length) {
    let curr = null;
    const spy = () => {
      const line = innerHeight * 0.5;   /* the middle of the viewport is what the reader is on */
      let pick = items[0];
      items.forEach((i) => { if (i.el.getBoundingClientRect().top <= line) pick = i; });
      /* the closing section is shorter than the fold: at the end of the page
         it is the one being read, whether or not it reached the line */
      if (innerHeight + Math.ceil(scrollY) >= document.documentElement.scrollHeight - 2)
        pick = items[items.length - 1];
      if (pick === curr) return;
      curr = pick;
      items.forEach((i) => i.a.classList.toggle("on", i === pick));
    };
    addEventListener("scroll", spy, { passive: true });
    addEventListener("resize", spy);
    spy();
  }
}

/* ════ GSAP direction ════ */
if (typeof gsap !== "undefined" && !reduced) {
  gsap.registerPlugin(ScrollTrigger);

  let lenis = null;
  if (typeof Lenis !== "undefined") {
    lenis = new Lenis({ lerp: 0.1, smoothWheel: true, autoRaf: false });
    lenis.on("scroll", ScrollTrigger.update);
    gsap.ticker.add((time) => lenis.raf(time * 1000));
    gsap.ticker.lagSmoothing(0);
  }

  /* A heading split into per-character spans has one character per text node,
     and a page translator has nothing to work with — Google Translate walks
     right past those headings and leaves them in English. So the split is
     TEMPORARY: every line keeps its own text, and gets it back the moment its
     reveal has played (revealChars below), or at once if a translation starts
     before that. The page then holds plain, translatable, selectable text. */
  const rawText = new WeakMap();
  const splitLines = [];

  function unsplitLine(line) {
    const raw = rawText.get(line);
    if (raw === undefined) return;
    rawText.delete(line);
    const i = splitLines.indexOf(line);
    if (i >= 0) splitLines.splice(i, 1);
    line.textContent = raw;
  }
  /* put back the lines these characters came from */
  function unsplitOf(chars) {
    const done = new Set();
    chars.forEach((c) => {
      const line = c.closest(".line");
      if (line && !done.has(line)) { done.add(line); unsplitLine(line); }
    });
  }
  /* reveal a split heading, then hand the plain text back */
  function revealChars(chars, vars) {
    if (!chars.length) return null;
    return gsap.from(chars, { ...vars, onComplete: () => unsplitOf(chars) });
  }
  /* a translation can start at any time, including before a heading has been
     revealed: give every remaining line its text back so the translator sees
     it. Losing one entrance animation is the cheaper half of that trade. */
  const lang0 = document.documentElement.lang;
  new MutationObserver(() => {
    if (/translated/.test(document.documentElement.className) ||
        document.documentElement.lang !== lang0) splitLines.slice().forEach(unsplitLine);
  }).observe(document.documentElement, { attributes: true, attributeFilter: ["class", "lang"] });

  function split(el) {
    const chs = [];
    if (!el) return chs;
    $$(".line", el).forEach((line) => {
      if (line.classList.contains("typed")) return;
      const raw = line.textContent;
      rawText.set(line, raw);
      splitLines.push(line);
      const words = raw.split(" ");
      line.textContent = "";
      words.forEach((word, wi) => {
        const w = document.createElement("span");
        w.className = "w";
        for (const c of word + (wi < words.length - 1 ? " " : "")) {
          const s = document.createElement("span");
          s.className = "ch";
          s.textContent = c;
          w.appendChild(s); chs.push(s);
        }
        line.appendChild(w);
      });
    });
    return chs;
  }

  /* cursor life: real code tokens drift on three acrylic depths, the swarm
     trailing the pointer. Each depth's frost (blur(18)+blur(12) stack of the
     B3 lab stand) is baked into pre-blurred sprites: same look, no extra
     fullscreen backdrop-filter layers. Config picked in design/refs/b3-lab. */
  {
    const TAU = Math.PI * 2;
    const FX = { rate: 0.1, cap: 100, alpha: 0.35, life: 0.8, twinkle: 0.5,
                 trail: 0.5, burst: 160, wander: 10, lag: 0.2, speed: 0.5 };
    const TOKS = ["::", "=>", "fn", "{ }", "//", "->", "%", "if", "&&", "let",
                  "0x1F", "async", ".await", "mut", "match", "#[derive]",
                  "impl", "||", "<>", "u8", "pub", "use"];
    /* font = 9px * layer scale; blur = frost above the layer (+soft 0.5);
       dim = layer alpha * frost tint attenuation; w = depth-0.5 weights */
    const LAYERS = [
      { key: "deep", font: 23, blur: 22, dim: 0.576, w: 0.2174 },
      { key: "mid", font: 14, blur: 12, dim: 0.68, w: 0.5652 },
      { key: "over", font: 9, blur: 0, dim: 0.9, w: 0.2174 },
    ];
    const cv = $(".fx-tokens");
    if (cv && !matchMedia("(prefers-reduced-motion: reduce)").matches) {
      const ctx = cv.getContext("2d");
      let dpr = 1;
      const sprites = new Map();
      const MAXPX = 2.4e6;                 /* beyond this the swarm is drawn smaller */
      const fit = () => {
        dpr = Math.min(2, devicePixelRatio || 1);
        const area = innerWidth * innerHeight * dpr * dpr;
        if (area > MAXPX) dpr *= Math.sqrt(MAXPX / area);
        cv.width = Math.round(innerWidth * dpr); cv.height = Math.round(innerHeight * dpr);
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        sprites.clear();
      };
      fit(); addEventListener("resize", fit);
      const sprite = (txt, L) => {
        const key = txt + "|" + L.key;
        let s = sprites.get(key);
        if (s) return s;
        const pad = Math.ceil(L.blur * 2.2 + 6);
        ctx.font = `${L.font}px Consolas, monospace`;
        const w = Math.ceil(ctx.measureText(txt).width) + pad * 2;
        const h = L.font + pad * 2;
        const c = document.createElement("canvas");
        c.width = w * dpr; c.height = h * dpr;
        const g = c.getContext("2d");
        if (L.blur >= 1) g.filter = `blur(${L.blur * dpr}px)`;
        g.font = `${L.font * dpr}px Consolas, monospace`;
        g.textAlign = "center"; g.textBaseline = "middle";
        g.fillStyle = "rgba(240,246,250,1)";
        g.fillText(txt, c.width / 2, c.height / 2);
        s = { c, w, h };
        sprites.set(key, s);
        return s;
      };
      const pickLayer = () => {
        let r = Math.random();
        for (const L of LAYERS) { if ((r -= L.w) <= 0) return L; }
        return LAYERS[2];
      };
      const st = { x: -400, y: -400, t: 0, seen: false };
      const lagPt = { x: 0, y: 0 };
      const flies = [];
      let carry = 0;
      const spawn = (x, y, vx, vy) => {
        flies.push({ x, y, vx, vy, life: 1, seed: Math.random() * 10,
                     L: pickLayer(), txt: TOKS[(Math.random() * TOKS.length) | 0] });
        while (flies.length > FX.cap) flies.shift();
      };
      let moved = 0, running = false;
      addEventListener("pointermove", (e) => {
        const dvx = e.clientX - st.x, dvy = e.clientY - st.y;
        moved = performance.now();
        if (!running) { running = true; last = performance.now(); requestAnimationFrame(loop); }
        if (!st.seen) { lagPt.x = e.clientX; lagPt.y = e.clientY; }
        st.x = e.clientX; st.y = e.clientY; st.seen = true;
        carry += Math.min(6, Math.hypot(dvx, dvy) * FX.rate);
        while (carry >= 1) {
          carry -= 1;
          const a = Math.random() * TAU, sp = FX.burst * (0.3 + Math.random() * 0.9);
          spawn(lagPt.x, lagPt.y, Math.cos(a) * sp + dvx * 1.6, Math.sin(a) * sp + dvy * 1.6);
        }
      }, { passive: true });
      let last = performance.now();
      function loop(now) {
        /* nothing alive and no hand on the mouse: stop until it moves again */
        if (!flies.length && now - moved > 400) {
          running = false;
          ctx.setTransform(1, 0, 0, 1, 0, 0);
          ctx.clearRect(0, 0, cv.width, cv.height);
          ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
          return;
        }
        requestAnimationFrame(loop);
        const rdt = Math.min(0.05, (now - last) / 1000); last = now;
        const dt = rdt * FX.speed;
        st.t += dt;
        const k = Math.min(1, rdt / FX.lag);
        lagPt.x += (st.x - lagPt.x) * k;
        lagPt.y += (st.y - lagPt.y) * k;
        /* trail: keep a fraction of the previous frame instead of clearing */
        ctx.globalCompositeOperation = "destination-in";
        ctx.fillStyle = `rgba(0,0,0,${FX.trail})`;
        ctx.fillRect(0, 0, innerWidth, innerHeight);
        ctx.globalCompositeOperation = "lighter";
        if (st.seen && flies.length < 3 && now - moved < 400) spawn(st.x + 18, st.y, 0, 0);
        for (let i = flies.length - 1; i >= 0; i--) {
          const f = flies[i];
          f.vx += Math.sin(st.t * 2.1 + f.seed * 7) * FX.wander * dt;
          f.vy += Math.cos(st.t * 1.6 + f.seed * 9) * FX.wander * dt;
          f.vx *= Math.exp(-1.3 * dt); f.vy *= Math.exp(-1.3 * dt);
          f.x += f.vx * dt; f.y += f.vy * dt;
          f.life -= dt / FX.life;
          if (f.life <= 0) { flies.splice(i, 1); continue; }
          const tw = (1 - FX.twinkle) + FX.twinkle * (0.5 + 0.5 * Math.sin(st.t * 6 + f.seed * 20));
          const s = sprite(f.txt, f.L);
          ctx.globalAlpha = Math.max(0, f.life) * tw * f.L.dim * FX.alpha;
          ctx.drawImage(s.c, f.x - s.w / 2, f.y - s.h / 2, s.w, s.h);
        }
        ctx.globalAlpha = 1;
        ctx.globalCompositeOperation = "source-over";
      }
      running = true;
      requestAnimationFrame(loop);
    }
  }

  gsap.to(".rings-bg:not(.rings-alt):not(.rings-far)", { rotation: 360, duration: 340, repeat: -1, ease: "none" });
  gsap.to(".rings-bg:not(.rings-alt):not(.rings-far)", { scrollTrigger: { start: 0, end: "max", scrub: 1.5 }, yPercent: 24 });
  /* the outer pass turns the other way and drifts further, so the right side
     never reads as one rigid wheel */
  gsap.to(".rings-far", { rotation: -360, duration: 610, repeat: -1, ease: "none" });
  gsap.to(".rings-far", { scrollTrigger: { start: 0, end: "max", scrub: 1.2 }, yPercent: 38 });
  /* the far fragment turns the other way, slower, and drifts less with the
     scroll — depth, not a reflection */
  gsap.to(".rings-alt", { rotation: -360, duration: 520, repeat: -1, ease: "none" });
  gsap.to(".rings-alt", { scrollTrigger: { start: 0, end: "max", scrub: 2.2 }, yPercent: -16 });

  /* hero intro */
  const heroChars = split($(".hero-copy .manifesto"));
  gsap.timeline({ defaults: { ease: "power3.out" } })
    .from(".top", { y: -24, opacity: 0, duration: 0.7 }, 0.1)
    .from(".rail", { x: 26, opacity: 0, duration: 0.8 }, 0.9)
    .from(heroChars, { yPercent: 118, duration: 0.9, stagger: 0.016, ease: "power4.out",
                       onComplete: () => unsplitOf(heroChars) }, 0.15)
    .from(".hero-copy .lead", { y: 26, opacity: 0, duration: 0.8 }, 0.55)
    .from(".hero-copy .cmd", { y: 22, opacity: 0, duration: 0.7 }, 0.7)
    .from(".hud-under", { opacity: 0, duration: 0.6 }, 0.85)
    .from(".hero-stage .sat", { y: 60, opacity: 0, duration: 1.1, stagger: 0.14 }, 0.4)
    .from("#aw-hero", { y: 44, opacity: 0, scale: 0.96, duration: 1.0 }, 0.6)
    .from(".stage-cap", { opacity: 0, duration: 0.7 }, 1.2)
    .add(() => typeInto($(".hero-copy .typed"), 0), 0.7);

  /* gentle pointer parallax on the constellation */
  {
    const q = [];
    [[".sat-a", 18], [".sat-b", -22], ["#aw-hero", 8]].forEach(([sel, k]) => {
      const el = $(sel);
      if (el) q.push([gsap.quickTo(el, "x", { duration: 0.7, ease: "power2.out" }),
                      gsap.quickTo(el, "y", { duration: 0.7, ease: "power2.out" }), k]);
    });
    addEventListener("pointermove", (e) => {
      const mx = e.clientX / innerWidth - 0.5, my = e.clientY / innerHeight - 0.5;
      q.forEach(([fx, fy, k]) => { fx(mx * k); fy(my * k * 0.7); });
    }, { passive: true });
  }

  /* shapes scrub */
  ScrollTrigger.create({
    trigger: "#shapes", start: "top top", end: "bottom bottom",
    onUpdate: (self) => shapeProgress(self.progress),
  });
  const shapeChars = split($("#shapes .m-side"));
  revealChars(shapeChars, {
    scrollTrigger: { trigger: "#shapes", start: "top 70%" },
    yPercent: 118, stagger: 0.014, duration: 0.8, ease: "power4.out",
  });

  /* sources */
  const provChars = split($(".prov-head .manifesto"));
  revealChars(provChars, {
    scrollTrigger: { trigger: ".prov-head", start: "top 74%" },
    yPercent: 118, stagger: 0.013, duration: 0.85, ease: "power4.out",
  });
  gsap.from(".pane", {
    scrollTrigger: { trigger: ".panes", start: "top 82%" },
    y: 64, opacity: 0, rotateX: 8, duration: 1.0, stagger: 0.1, ease: "power3.out",
  });
  {
    const rx = gsap.quickTo(".panes", "rotationX", { duration: 0.8, ease: "power2.out" });
    const ry = gsap.quickTo(".panes", "rotationY", { duration: 0.8, ease: "power2.out" });
    addEventListener("pointermove", (e) => {
      ry((e.clientX / innerWidth - 0.5) * 5);
      rx((e.clientY / innerHeight - 0.5) * -4);
    }, { passive: true });
  }

  /* telemetry */
  const teleChars = split($(".tele-head .manifesto"));
  revealChars(teleChars, {
    scrollTrigger: { trigger: ".tele-head", start: "top 74%" },
    yPercent: 118, stagger: 0.014, duration: 0.85, ease: "power4.out",
  });
  gsap.from(".telemetry .ghost", {
    scrollTrigger: { trigger: ".telemetry", start: "top 80%", end: "bottom 40%", scrub: 1 },
    yPercent: 24, opacity: 0,
  });
  gsap.from(".num", {
    scrollTrigger: { trigger: ".numbers", start: "top 82%" },
    y: 40, opacity: 0, duration: 0.8, stagger: 0.09, ease: "power3.out",
  });
  gsap.from(".honesty", {
    scrollTrigger: { trigger: ".honesty", start: "top 84%" },
    y: 40, opacity: 0, duration: 0.9, ease: "power3.out",
  });

  /* taskbar */
  const tbChars = split($(".tb-head .manifesto"));
  revealChars(tbChars, {
    scrollTrigger: { trigger: ".tb-head", start: "top 74%" },
    yPercent: 118, stagger: 0.014, duration: 0.85, ease: "power4.out",
  });

  /* playground */
  const pgChars = split($(".pg-head .manifesto"));
  revealChars(pgChars, {
    scrollTrigger: { trigger: ".pg-head", start: "top 74%" },
    yPercent: 118, stagger: 0.014, duration: 0.85, ease: "power4.out",
  });
  gsap.from([".pg-rail", ".pg-desk"], {
    scrollTrigger: { trigger: ".pg", start: "top 80%" },
    y: 56, opacity: 0, duration: 0.9, stagger: 0.12, ease: "power3.out",
  });

  /* finale */
  const finChars = split($(".finale .manifesto"));
  gsap.from(".fin-icon img", {
    scrollTrigger: { trigger: ".finale", start: "top 70%" },
    scale: 0.6, opacity: 0, filter: "blur(14px)", duration: 1.1, ease: "power3.out",
  });
  revealChars(finChars, {
    scrollTrigger: { trigger: ".finale", start: "top 62%" },
    yPercent: 118, stagger: 0.02, duration: 0.9, ease: "power4.out",
  });
  gsap.from(".term", {
    scrollTrigger: { trigger: ".term", start: "top 82%" },
    y: 44, opacity: 0, duration: 0.9, ease: "power3.out",
  });

  /* smooth the jump; which entry is lit is decided by the rail's own spy */
  let jumpEnd = 0;
  const takeBack = () => endJump();
  function endJump() {
    clearTimeout(jumpEnd);
    jumpEnd = 0;
    removeEventListener("wheel", takeBack);
    removeEventListener("touchstart", takeBack);
    removeEventListener("keydown", takeBack);
    if (!document.documentElement.dataset.jump) return;
    delete document.documentElement.dataset.jump;
    dispatchEvent(new Event("scroll"));     /* let the scenes catch up at once */
  }
  $$('.rail a, a[href^="#"]:not(.skip)').forEach((a) => {
    a.addEventListener("click", (e) => {
      const id = a.getAttribute("data-rail") || (a.getAttribute("href") || "").slice(1);
      const target = id && document.getElementById(id);
      if (!target) return;
      e.preventDefault();
      document.documentElement.dataset.jump = "1";
      if (lenis) lenis.scrollTo(target, { offset: -20, onComplete: endJump });
      else target.scrollIntoView({ behavior: "smooth" });
      /* three ways out, and the last one cannot be postponed */
      addEventListener("wheel", takeBack, { passive: true });
      addEventListener("touchstart", takeBack, { passive: true });
      addEventListener("keydown", takeBack);
      clearTimeout(jumpEnd);
      jumpEnd = setTimeout(endJump, 1300);
    });
  });
}
