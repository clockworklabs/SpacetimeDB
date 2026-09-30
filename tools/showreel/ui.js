// Scene-level building blocks shared by every reel (the showreel and the release videos):
// starfield + dot grid background, shockwaves, particle bursts, headline/kicker text, terminal
// rows, check marks, spinners and the HUD frame.
const L = require('./lib');
const { W, H, C, SANS, MONO, clamp, lerp, P, E, pulse, rng, hash, hexA, rr, brandGrad, riseText, measure, text, scramble, drawMark, createCanvas } = L;

const CX = W / 2, CY = H / 2;
const TAU = Math.PI * 2;

// ---------- background ----------
// Warp starfield. warpSpeed(t) is the reel's speed curve; it's integrated once up front.
function makeStarfield(warpSpeed, DUR) {
  const ZT = (() => {
    const dt = 0.002, n = Math.ceil((DUR + 1) / dt) + 2, z = new Float64Array(n);
    for (let i = 1; i < n; i++) z[i] = z[i - 1] + warpSpeed(i * dt) * dt;
    return { dt, z };
  })();
  const Zat = t => { const i = clamp(t / ZT.dt, 0, ZT.z.length - 2); const i0 = Math.floor(i); return lerp(ZT.z[i0], ZT.z[i0 + 1], i - i0); };

  const STARS = (() => {
    const r = rng(7), a = [];
    for (let i = 0; i < 700; i++) {
      const ang = r() * TAU, rad = Math.pow(r(), 0.6) * 1.2 + 0.03;
      a.push({ x: Math.cos(ang) * rad, y: Math.sin(ang) * rad * 0.75, z0: r(), s: 0.5 + r() * 1.3, c: (r() < 0.14 && r(), C.white) });
    }
    return a;
  })();

  return function drawStars(ctx, t) {
    const Z = Zat(t), sp = warpSpeed(t);
    const f = 520, trail = 0.012 + sp * 0.07;
    ctx.save();
    ctx.lineCap = 'round';
    for (const s of STARS) {
      let z = ((s.z0 - Z * 0.35) % 1 + 1) % 1;
      z = z + 0.02;
      const z2 = z + trail;
      const x1 = CX + (s.x / z) * f, y1 = CY + (s.y / z) * f;
      const x2 = CX + (s.x / z2) * f, y2 = CY + (s.y / z2) * f;
      if (x1 < -50 || x1 > W + 50 || y1 < -50 || y1 > H + 50) continue;
      const a = clamp((1 - z) * 1.2) * clamp(z * 12) * 0.75;
      ctx.strokeStyle = hexA(s.c, a);
      ctx.lineWidth = s.s * (1.4 - z);
      ctx.beginPath(); ctx.moveTo(x2, y2); ctx.lineTo(x1 + 0.01, y1); ctx.stroke();
    }
    ctx.restore();
  };
}

// Dot grid, drawn once; scroll it by drawing at an offset.
function makeGrid() {
  const grid = createCanvas(W + 120, H + 120);
  const g = grid.getContext('2d');
  g.fillStyle = 'rgba(244,246,252,0.075)';
  for (let y = 0; y < H + 120; y += 48) for (let x = 0; x < W + 120; x += 48) { g.beginPath(); g.arc(x, y, 1.1, 0, TAU); g.fill(); }
  return grid;
}

// ---------- FX ----------
function fxShock(ctx, t, SHOCKS) {
  for (const s of SHOCKS) {
    const d = t - s.t;
    if (d < 0 || d > 1.4) continue;
    const p = E.outExpo(clamp(d / 1.2));
    ctx.save();
    ctx.globalAlpha = (1 - clamp(d / 1.2)) * 0.6;
    ctx.strokeStyle = C.white;
    ctx.lineWidth = s.w * (1 - p) + 1;
    ctx.beginPath(); ctx.arc(s.x, s.y, 20 + p * s.r, 0, TAU); ctx.stroke();
    ctx.restore();
  }
}

function fxBursts(ctx, t, BURSTS) {
  for (const b of BURSTS) {
    const d = t - b.t;
    if (d < 0 || d > 1.8) continue;
    const r = rng(b.seed);
    ctx.save();
    ctx.globalCompositeOperation = 'lighter';
    ctx.lineCap = 'round';
    for (let i = 0; i < b.n; i++) {
      let ang = r() * TAU;
      if (b.dir != null) ang = b.dir + (r() - 0.5) * 1.6;
      const v = (300 + r() * 1500) * (b.v ?? 1), life = 0.6 + r() * 1.1, sz = 1 + r() * 2.8;
      const col = [C.white, C.white, C.white, C.green][Math.floor(r() * 4)];
      if (d > life) continue;
      const k = 3.2;
      const dist = u => (v * (1 - Math.exp(-k * u))) / k;
      const d1 = dist(d), d0 = dist(Math.max(0, d - 0.04));
      const x1 = b.x + Math.cos(ang) * d1, y1 = b.y + Math.sin(ang) * d1 + 60 * d * d;
      const x0 = b.x + Math.cos(ang) * d0, y0 = b.y + Math.sin(ang) * d0 + 60 * d * d;
      ctx.strokeStyle = hexA(col, (1 - d / life) * 0.95);
      ctx.lineWidth = sz;
      ctx.beginPath(); ctx.moveTo(x0, y0); ctx.lineTo(x1 + 0.01, y1); ctx.stroke();
    }
    ctx.restore();
  }
}

// ---------- text ----------
function riseParts(ctx, parts, x, y, o) {
  const tr = o.letter ?? (o.size > 40 ? -1 : 0);
  const widths = parts.map(p => measure(ctx, p.s, o.weight ?? 900, o.size, o.fam ?? SANS, tr));
  const sp = o.gap ?? o.size * 0.27;
  const total = widths.reduce((a, b) => a + b, 0) + sp * (parts.length - 1);
  let sx = o.align === 'center' ? x - total / 2 : o.align === 'right' ? x - total : x;
  let idx = 0;
  parts.forEach((p, i) => {
    const st = o.stagger ?? 0.02;
    const fill = p.fill === 'brand' ? () => brandGrad(ctx, sx, 0, sx + widths[i], 0) : p.fill || o.fill;
    riseText(ctx, p.s, sx, y, { ...o, tracking: tr, align: 'left', fill, t: o.t - idx * st, outT: o.outT != null ? o.outT - idx * st * 0.6 : undefined, glow: p.glow ?? o.glow });
    idx += [...p.s].length;
    sx += widths[i] + sp;
  });
  return total;
}

function kicker(ctx, str, x, y, t, o = {}) {
  if (t < 0) return;
  const s = scramble(str, clamp(t / (o.dur ?? 0.5)), Math.floor(t * 30) * 7 + str.length);
  text(ctx, s, x, y, { size: o.size ?? 18, weight: 600, tracking: o.tracking ?? 5, fill: o.fill ?? hexA(C.white, 0.55), alpha: o.alpha ?? 1, align: o.align });
}

// ---------- small widgets ----------
function check(ctx, x, y, s, col, p = 1) {
  ctx.save();
  ctx.strokeStyle = col; ctx.lineWidth = 2.6; ctx.lineCap = 'round'; ctx.lineJoin = 'round';
  ctx.beginPath();
  const pts = [[x, y], [x + s * 0.35, y + s * 0.35], [x + s, y - s * 0.45]];
  ctx.moveTo(pts[0][0], pts[0][1]);
  const l1 = 0.4;
  if (p < l1) ctx.lineTo(lerp(pts[0][0], pts[1][0], p / l1), lerp(pts[0][1], pts[1][1], p / l1));
  else { ctx.lineTo(pts[1][0], pts[1][1]); const q = (p - l1) / (1 - l1); ctx.lineTo(lerp(pts[1][0], pts[2][0], q), lerp(pts[1][1], pts[2][1], q)); }
  ctx.stroke();
  ctx.restore();
}
function spinner(ctx, x, y, r, t, col) {
  ctx.save();
  ctx.strokeStyle = col; ctx.lineWidth = 2.6; ctx.lineCap = 'round';
  ctx.beginPath(); ctx.arc(x, y, r, t * 12, t * 12 + 4.2); ctx.stroke();
  ctx.restore();
}
function trafficLights(ctx, x, y) {
  ['#3a4449', '#3a4449', '#3a4449'].forEach((c, i) => { ctx.fillStyle = c; ctx.beginPath(); ctx.arc(x + 30 + i * 22, y + 30, 6.5, 0, TAU); ctx.fill(); });
}
function chip(ctx, x, y, label, sc, rot, col, alpha = 1) {
  ctx.save();
  ctx.globalAlpha *= alpha;
  ctx.translate(x, y); ctx.rotate(rot); ctx.scale(sc, sc);
  const w = measure(ctx, label, 600, 19, MONO) + 50;
  rr(ctx, -w / 2, -24, w, 48, 12);
  ctx.fillStyle = '#0d1118'; ctx.fill();
  ctx.strokeStyle = hexA(C.white, 0.4); ctx.lineWidth = 2;
  ctx.shadowColor = col === 'brand' ? C.purple : col; ctx.shadowBlur = 25; ctx.stroke();
  ctx.shadowBlur = 0;
  text(ctx, label, 0, 7, { size: 19, weight: 600, align: 'center' });
  ctx.restore();
}
// terminal rows: [{cmd, c0, c1, res:[[u, text, col, xoff]]}]
function termRows(ctx, tm, u, rows, rowH = 96) {
  rows.forEach((r, i) => {
    const y0 = tm.y + (r.y ?? (46 + i * rowH));
    if (u < r.c0 - 0.05) return;
    const cn = Math.floor(P(u, r.c0, r.c1) * r.cmd.length);
    text(ctx, '$', tm.x + 28, y0, { size: 20, fill: C.pink, weight: 600 });
    text(ctx, r.cmd.slice(0, cn), tm.x + 54, y0, { size: 20, weight: 500 });
    for (const [ru, s, col, ox] of r.res) {
      if (u < ru) continue;
      const pp = E.outCubic(P(u, ru, ru + 0.25));
      check(ctx, tm.x + 30 + ox, y0 + 40, 14, C.green, pp);
      text(ctx, s, tm.x + 54 + ox, y0 + 46, { size: 18, fill: col, alpha: pp });
    }
  });
}
// A point of light at screen center that swells from u0 to u1 (scene exit into the next).
function starPoint(ctx, u0, u1, u) {
  if (u > u0) {
    const pt = P(u, u0, u1);
    ctx.save();
    const r = lerp(2, 16, E.inExpo(pt));
    ctx.fillStyle = C.white; ctx.shadowColor = C.white; ctx.shadowBlur = 60;
    ctx.beginPath(); ctx.arc(CX, CY, r, 0, TAU); ctx.fill();
    ctx.restore();
  }
}

// ---------- HUD ----------
// Corner brackets, logo + reel label, timecode, scene name and progress ticks.
function drawHUD(ctx, t, rt, { scenes, dur, label }) {
  const a = E.outCubic(P(t, 0.3, 0.9)) * (1 - P(t, dur - 1.6, dur - 1.0));
  if (a <= 0) return;
  ctx.save();
  ctx.globalAlpha = a;
  const m = 40, l = 22;
  ctx.strokeStyle = hexA(C.white, 0.35); ctx.lineWidth = 1.5;
  [[m, m, 1, 1], [W - m, m, -1, 1], [m, H - m, 1, -1], [W - m, H - m, -1, -1]].forEach(([x, y, sx, sy]) => { ctx.beginPath(); ctx.moveTo(x, y + sy * l); ctx.lineTo(x, y); ctx.lineTo(x + sx * l, y); ctx.stroke(); });
  drawMark(ctx, 84, 74, 30, {});
  text(ctx, 'SPACETIMEDB', 106, 80, { size: 14, weight: 700, tracking: 4 });
  text(ctx, label, 250, 80, { size: 14, weight: 500, tracking: 3, fill: hexA(C.white, 0.4) });
  // timecode + rec blink show real (on-screen) time
  const fr = Math.floor(rt * 60);
  const tc = `00:${String(Math.floor(rt / 60)).padStart(2, '0')}:${String(Math.floor(rt % 60)).padStart(2, '0')}:${String(fr % 60).padStart(2, '0')}`;
  text(ctx, tc, W - 76, 80, { size: 14, weight: 600, tracking: 3, align: 'right' });
  ctx.fillStyle = hexA(C.white, 0.6);
  ctx.globalAlpha = a * (Math.floor(rt * 2) % 2 ? 0.35 : 1);
  ctx.beginPath(); ctx.arc(W - 262, 75, 5, 0, TAU); ctx.fill();
  ctx.globalAlpha = a;
  let k = 0; scenes.forEach((s, i) => { if (t >= s.t) k = i; });
  const st = scenes[k].t;
  const n = scenes.length;
  const name = `${String(k + 1).padStart(2, '0')} — ${scenes[k].name}`;
  text(ctx, scramble(name, clamp((t - st) / 0.4), Math.floor(t * 30) * 3), 76, H - 66, { size: 14, weight: 600, tracking: 4, fill: hexA(C.white, 0.6) });
  const px = W - 396, pw = 320, py = H - 72;
  ctx.fillStyle = hexA(C.white, 0.15); ctx.fillRect(px, py, pw, 2);
  ctx.fillStyle = hexA(C.white, 0.8); ctx.fillRect(px, py, pw * (t / dur), 2);
  scenes.forEach(s => { ctx.fillStyle = hexA(C.white, t >= s.t ? 0.8 : 0.3); ctx.fillRect(px + pw * (s.t / dur), py - 5, 1.5, 12); });
  text(ctx, `${String(k + 1).padStart(2, '0')}/${String(n).padStart(2, '0')}`, W - 76, H - 66, { size: 14, weight: 600, tracking: 3, align: 'right', fill: hexA(C.white, 0.6) });
  ctx.restore();
}

module.exports = { CX, CY, TAU, makeStarfield, makeGrid, fxShock, fxBursts, riseParts, kicker, check, spinner, trafficLights, chip, termRows, starPoint, drawHUD };
