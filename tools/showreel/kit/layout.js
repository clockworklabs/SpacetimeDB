// Layout helpers shared by release-video scenes: scene entry/exit, panels, captions, pills, code lines, headers.
const L = require('../lib');
const { W, H, C, SANS, MONO, clamp, lerp, P, E, spring, pulse, hexA, rr, glass, brandGrad, font, riseText, measure, text, drawMark, A, createCanvas } = L;
const U = require('../ui');
const { CX, CY, TAU, riseParts, kicker, check, spinner, trafficLights } = U;

// ---------- layout helpers ----------
// Slide a scene in from the right and out to the left.
function slide(ctx, u, len) {
  const ent = E.outExpo(P(u, -0.1, 0.35));
  const ex = E.inExpo(P(u, len - 0.4, len));
  ctx.translate((1 - ent) * W - ex * W * 1.1, 0);
}

// Window-like panel with traffic lights and a centered title.
function panel(ctx, x, y, w, h, title) {
  glass(ctx, x, y, w, h, 20);
  trafficLights(ctx, x, y);
  if (title) text(ctx, title, x + w / 2, y + 36, { size: 15, align: 'center', fill: hexA(C.white, 0.45) });
  ctx.fillStyle = 'rgba(255,255,255,0.07)'; ctx.fillRect(x, y + 60, w, 1);
}

// Fade + lift in, starting at u0.
function enter(ctx, u, u0, dy = 30, d = 0.45) {
  const a = E.outExpo(P(u, u0, u0 + d));
  ctx.globalAlpha *= a;
  ctx.translate(0, (1 - a) * dy);
  return a;
}

// Spinner that turns into a check `run` seconds after lt = 0.
function status(ctx, x, y, lt, t, run = 0.4) {
  if (lt < run) spinner(ctx, x + 8, y - 1, 8, t, C.white);
  else check(ctx, x, y, 16, C.green, E.outCubic(clamp((lt - run) / 0.12)));
}

// Sans caption centered under a scene.
function caption(ctx, s, u, u0, y = 985) {
  if (u < u0) return;
  riseText(ctx, s, CX, y, { t: u - u0, size: 30, weight: 500, align: 'center', stagger: 0.006, dur: 0.4, fill: hexA(C.white, 0.72) });
}

// Rounded pill with a label; returns its width.
function pill(ctx, x, y, label, o = {}) {
  const size = o.size ?? 17, weight = o.weight ?? 600, fam = o.fam ?? MONO;
  const w = measure(ctx, label, weight, size, fam) + (o.pad ?? 40), h = o.h ?? 40;
  rr(ctx, x, y - h / 2, w, h, h / 2);
  ctx.fillStyle = o.bg ?? '#0d1317'; ctx.fill();
  ctx.strokeStyle = o.stroke ?? hexA(C.white, 0.25); ctx.lineWidth = 1.5; ctx.stroke();
  text(ctx, label, x + w / 2, y + size * 0.35, { size, weight, fam, align: 'center', fill: o.fill });
  return w;
}

// Text that shrinks to fit a width.
function fitText(ctx, s, x, y, maxW, o) {
  let size = o.size;
  while (size > 12 && measure(ctx, s, o.weight ?? 400, size, o.fam ?? SANS) > maxW) size -= 0.5;
  text(ctx, s, x, y, { ...o, size });
}

// Chat bubble (user side).
function bubble(ctx, s, x, y, align, a) {
  const tw = measure(ctx, s, 500, 24, SANS);
  const bw = tw + 48, bh = 58;
  const bx = align === 'right' ? x - bw : x;
  ctx.save();
  ctx.globalAlpha *= a;
  ctx.translate(0, (1 - a) * 16);
  rr(ctx, bx, y - bh / 2, bw, bh, 18);
  ctx.fillStyle = hexA(C.white, 0.09); ctx.fill();
  text(ctx, s, bx + 24, y + 8, { size: 24, weight: 500, fam: SANS });
  ctx.restore();
}

// Draw a line of code: parts = [[text, color|null], ...] in the showreel's syntax palette.
const SYN = { kw: '#ff80fb', fn: '#4cf490', type: '#fbdc8e', str: '#8ae8ff', attr: '#a880ff', com: 'rgba(244,246,252,0.4)' };
function code(ctx, parts, x, y, size = 22, alpha = 1) {
  let cx = x;
  for (const [s, col] of parts) {
    text(ctx, s, cx, y, { size, fill: col ?? hexA(C.white, 0.88), alpha });
    cx += measure(ctx, s, 500, size, MONO);
  }
  return cx - x;
}

// Standard scene header: kicker + headline (one part filled with the brand gradient).
// The headline shrinks to fit the 1640 px content width, so plain-language headlines can run long.
function header(ctx, u, kick, parts, o = {}) {
  const center = o.align === 'center';
  kicker(ctx, kick, center ? CX : 142, 118, u - 0.05, { size: 16, tracking: 6, align: center ? 'center' : undefined });
  let size = o.size ?? 84;
  const width = sz => parts.reduce((a, p) => a + measure(ctx, p.s, 900, sz, SANS, -1), 0) + sz * 0.27 * (parts.length - 1);
  while (size > 52 && width(size) > 1640) size -= 2;
  riseParts(ctx, parts, center ? CX : 140, 205, { t: u - 0.1, size, weight: 900, stagger: 0.018, align: center ? 'center' : undefined });
}

module.exports = { slide, panel, enter, status, caption, pill, fitText, bubble, code, SYN, header };
