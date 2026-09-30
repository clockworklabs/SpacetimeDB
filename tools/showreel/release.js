// Shared building blocks for the release ("what's new") videos: layout helpers, the version intro,
// the card grid, the outro and the frame composition (background, FX, HUD). The showreel doesn't use this file.
const L = require('./lib');
const { W, H, C, SANS, MONO, clamp, lerp, P, E, spring, pulse, hexA, rr, glass, brandGrad, font, riseText, measure, text, drawMark, A, createCanvas } = L;
const U = require('./ui');
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

// ---------- intro: version odometer ----------
// pre stays, `from` rolls out, `to` rolls in (e.g. 'v2.' + '4' → '5', or 'v' + '1.12' → '2.0').
function makeIntro({ pre, from, to, sub, T }) {
  return function sIntro(ctx, t) {
    const u = t - T.intro;
    if (u > 5.7) return;
    const lp = E.outExpo(P(u, 0.0, 0.7)), lf = 1 - P(u, 0.5, 1.1);
    if (lf > 0) {
      ctx.save();
      ctx.globalAlpha = lf;
      const w = lp * 1800;
      const g = ctx.createLinearGradient(CX - w / 2, 0, CX + w / 2, 0);
      g.addColorStop(0, hexA(C.white, 0)); g.addColorStop(0.5, C.white); g.addColorStop(1, hexA(C.white, 0));
      ctx.fillStyle = g;
      ctx.fillRect(CX - w / 2, CY - 1.5, w, 3);
      ctx.restore();
    }
    const ex = E.inExpo(P(u, 5.05, 5.5));
    const zoom = 1 + 0.05 * E.inOutSine(P(u, 0, 5.5)) + 0.05 * pulse(u - 2.5, 8) + ex * 0.6;
    ctx.save();
    ctx.globalAlpha = 1 - ex;
    ctx.translate(CX, CY); ctx.scale(zoom, zoom); ctx.translate(-CX, -CY);
    kicker(ctx, 'RELEASE NOTES', CX, 250, u - 0.3, { size: 24, tracking: 14, align: 'center', dur: 0.6 });
    const wt = u - 0.7;
    if (wt > 0) {
      const ww = 560, wh = (ww * 53) / 348;
      const wp = E.outExpo(clamp(wt / 0.6));
      ctx.save();
      ctx.beginPath(); ctx.rect(CX - ww / 2 - 20, 300, (ww + 40) * wp, 140); ctx.clip();
      ctx.drawImage(A.wordmark, CX - ww / 2, 330, ww, wh);
      ctx.restore();
    }
    const vt = u - 1.2;
    if (vt > 0) {
      const size = 250, y = 700, tr = -4;
      const roll = E.inOutExpo(P(u, 2.05, 2.5));
      const wp = measure(ctx, pre, 900, size, SANS, tr), wo = measure(ctx, from, 900, size, SANS, tr), wn = measure(ctx, to, 900, size, SANS, tr);
      const total = wp + lerp(wo, wn, roll);
      const x0 = CX - total / 2;
      const landed = roll >= 1;
      const pfill = landed ? () => brandGrad(ctx, x0, 0, x0 + total, 0) : hexA(C.white, 0.9);
      riseText(ctx, pre, x0, y, { t: vt, size, weight: 900, stagger: 0.04, dur: 0.5, tracking: tr, fill: pfill });
      ctx.save();
      ctx.beginPath(); ctx.rect(x0 + wp - 10, y - size * 0.95, 1200, size * 1.2); ctx.clip();
      if (roll < 1) riseText(ctx, from, x0 + wp, y - roll * size * 1.1, { t: vt - 0.12, size, weight: 900, stagger: 0.04, dur: 0.5, tracking: tr, fill: hexA(C.white, 0.9), mask: false });
      if (roll > 0) {
        font(ctx, 900, size, SANS);
        ctx.fillStyle = landed ? brandGrad(ctx, x0, 0, x0 + total, 0) : C.white;
        ctx.textBaseline = 'alphabetic';
        let cx = x0 + wp;
        for (const ch of to) { ctx.fillText(ch, cx, y + (1 - roll) * size * 1.1); cx += ctx.measureText(ch).width + tr; }
      }
      ctx.restore();
    }
    riseText(ctx, sub, CX, 820, { t: u - 2.9, size: 40, weight: 500, align: 'center', stagger: 0.014, dur: 0.5, fill: hexA(C.white, 0.7), tracking: 0.5 });
    ctx.restore();
  };
}

// ---------- cards: four "plus" cards ----------
// list: [{k: KICKER, title: [l1, l2], d: [l1, l2]}]; toOutro: collapse into a point for the outro instead of sliding out.
function makeCards({ start, end, kick, parts, list, toOutro }) {
  function cards(ctx, u) {
    const n = list.length, cw = n > 4 ? 320 : n > 3 ? 400 : 480, chh = 370, gap = 28;
    const x0 = CX - (n * cw + (n - 1) * gap) / 2, y0 = 330;
    list.forEach((f, i) => {
      const lt = u - (0.4 + i * 0.22);
      if (lt < 0) return;
      const p = E.outExpo(clamp(lt / 0.6));
      const x = x0 + i * (cw + gap);
      ctx.save();
      ctx.globalAlpha *= clamp(lt * 4);
      ctx.translate(x, y0 + (1 - p) * 60);
      glass(ctx, 0, 0, cw, chh, 20);
      text(ctx, f.k, 32, 56, { size: 14, weight: 700, tracking: 3, fill: hexA(C.white, 0.5) });
      check(ctx, cw - 58, 50, 16, C.green, E.outCubic(clamp((lt - 0.4) / 0.25)));
      f.title.forEach((s, k) => fitText(ctx, s, 32, 138 + k * 42, cw - 64, { size: 31, weight: 800, fam: SANS }));
      ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(32, 230, cw - 64, 1);
      f.d.forEach((s, k) => fitText(ctx, s, 32, 290 + k * 34, cw - 64, { size: 22, weight: 400, fam: SANS, fill: hexA(C.white, 0.7) }));
      ctx.restore();
    });
  }
  return function sCards(ctx, t) {
    const u = t - start;
    const len = end - start;
    if (u < -0.1 || u > len) return;
    ctx.save();
    if (toOutro) {
      const ex = E.inExpo(P(u, len - 0.45, len - 0.05));
      const ent = E.outExpo(P(u, -0.1, 0.35));
      ctx.translate((1 - ent) * W, 0);
      ctx.translate(CX, CY); ctx.scale(1 - ex, 1 - ex); ctx.translate(-CX, -CY);
      ctx.globalAlpha *= 1 - ex;
    } else slide(ctx, u, len);
    header(ctx, u, kick, parts, { align: 'center' });
    cards(ctx, u);
    ctx.restore();
    if (toOutro) U.starPoint(ctx, len - 0.45, len, u);
  };
}

// ---------- outro ----------
function makeOutro({ version, T }) {
  let GLINT = null;
  return function sOutro(ctx, t) {
    const u = t - T.outro;
    if (u < 0) return;
    const sc = lerp(1.35, 1, spring(u, 1.2, 5.5));
    drawMark(ctx, CX, 400, 300 * sc, { sweep: E.outExpo(P(u, 0, 0.65)), ring: E.outExpo(P(u, 0.06, 0.6)), rot: -0.35 * (1 - E.outExpo(clamp(u / 0.9))) });
    const wt = u - 0.55;
    if (wt > 0) {
      const ww = 560, wh = (ww * 53) / 348;
      if (!GLINT) GLINT = createCanvas(ww + 40, Math.ceil(wh) + 20);
      const g = GLINT.getContext('2d');
      g.setTransform(1, 0, 0, 1, 0, 0);
      g.globalCompositeOperation = 'source-over';
      g.clearRect(0, 0, GLINT.width, GLINT.height);
      g.drawImage(A.wordmark, 20, 10, ww, wh);
      const gp = P(u, 0.95, 1.55);
      if (gp > 0 && gp < 1) {
        g.globalCompositeOperation = 'source-atop';
        const gx = lerp(-200, ww + 240, E.inOutCubic(gp));
        const lg = g.createLinearGradient(gx - 120, 0, gx + 120, 0);
        lg.addColorStop(0, 'rgba(76,244,144,0)'); lg.addColorStop(0.5, 'rgba(76,244,144,1)'); lg.addColorStop(1, 'rgba(76,244,144,0)');
        g.fillStyle = lg;
        g.save(); g.transform(1, 0, -0.4, 1, 0, 0); g.fillRect(-100, 0, GLINT.width + 300, GLINT.height); g.restore();
      }
      const wp = E.outExpo(clamp(wt / 0.6));
      ctx.save();
      ctx.beginPath(); ctx.rect(CX - ww / 2 - 20, 580, (ww + 40) * wp, 200); ctx.clip();
      ctx.translate((1 - wp) * -40, 0);
      ctx.drawImage(GLINT, CX - ww / 2 - 20, 600);
      ctx.restore();
    }
    riseParts(ctx, [{ s: 'Version' }, { s: version, fill: 'brand' }, { s: 'is out.' }], CX, 790, { t: u - 1.1, size: 56, weight: 800, align: 'center', stagger: 0.02 });
    const ct = E.outExpo(P(u, 1.8, 2.3));
    if (ct > 0) {
      const s1 = 'spacetime version upgrade', s2 = 'github.com/clockworklabs/SpacetimeDB/releases';
      const w1 = measure(ctx, s1, 600, 20, MONO), w2 = measure(ctx, s2, 400, 18, MONO);
      const pw = w1 + w2 + 110, ph = 58;
      ctx.save();
      ctx.globalAlpha = ct;
      ctx.translate(CX, 900 + (1 - ct) * 30);
      rr(ctx, -pw / 2, -ph / 2, pw, ph, ph / 2);
      ctx.fillStyle = 'rgba(255,255,255,0.05)'; ctx.fill();
      ctx.strokeStyle = hexA(C.white, 0.2); ctx.lineWidth = 1.5; ctx.stroke();
      text(ctx, s1, -pw / 2 + 32, 7, { size: 20, weight: 600 });
      ctx.fillStyle = hexA(C.white, 0.2); ctx.fillRect(-pw / 2 + 32 + w1 + 22, -14, 1, 28);
      text(ctx, s2, -pw / 2 + 32 + w1 + 46, 6, { size: 18, weight: 400, fill: hexA(C.white, 0.65) });
      ctx.restore();
    }
  };
}

// ---------- the reel ----------
// T: scene start times with `intro: 0` and `outro`; SCENES: [{t, name}] for the HUD; draw: scene functions in order.
function makeReel({ DUR, T, SCENES, label, draw, seed = 1 }) {
  const K = { ver: T.intro + 2.5, outro: T.outro };
  const starts = SCENES.map(s => s.t).filter(s => s !== T.intro && s !== T.outro);
  const FLASHES = [{ t: K.ver, a: 0.5, k: 6 }, { t: K.outro, a: 0.9, k: 4.5 }];
  const SHAKES = [{ t: K.ver, a: 18 }, { t: K.outro, a: 20 }, ...starts.map(s => ({ t: s + 0.1, a: 5 }))];
  const SHOCKS = [
    { t: K.ver, x: CX, y: 610, col: C.green, r: 1400, w: 50 },
    { t: K.outro, x: CX, y: 420, col: C.green, r: 1600, w: 70 },
    { t: K.outro + 0.06, x: CX, y: 420, col: C.purple, r: 1200, w: 26 },
  ];
  const BURSTS = [{ t: K.ver, x: CX, y: 610, n: 120, seed: 11 + seed }, { t: K.outro, x: CX, y: 420, n: 160, seed: 77 + seed }];

  function warpSpeed(t) {
    let s = 0.08;
    s += 1.0 * (1 - E.outCubic(P(t, 0, 3.0))) * P(t, 0, 0.3);
    s += 2.2 * pulse(t - K.ver, 2.5);
    for (const k of starts) s += 0.5 * pulse(t - k, 4);
    s += 2.0 * pulse(t - K.outro, 2.5);
    s += 2.6 * E.inCubic(P(t, K.outro + 1.5, K.outro + 5.0));
    return s;
  }
  let GRID = null, drawStars = null;
  let SLOW = 1;
  const setSlow = n => { SLOW = n; };
  function shakeOffset(t, rt = t) {
    let a = 0;
    for (const s of SHAKES) if (t >= s.t) a += s.a * pulse((t - s.t) * SLOW, 9);
    return [a * (Math.sin(rt * 91) * 0.6 + Math.sin(rt * 143 + 1.3) * 0.4), a * (Math.cos(rt * 107) * 0.6 + Math.sin(rt * 67 + 0.4) * 0.4)];
  }
  // t = scene time (0..DUR); rt = real on-screen time (differs only when slowed down)
  function frame(ctx, t, rt = t) {
    if (!GRID) { GRID = U.makeGrid(); drawStars = U.makeStarfield(warpSpeed, DUR); }
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = 'source-over';
    ctx.fillStyle = C.bg;
    ctx.fillRect(0, 0, W, H);
    const ga = 1 - 0.8 * (1 - P(t, 4, 5.5)) - 0.8 * P(t, K.outro - 0.3, K.outro + 0.1);
    ctx.save();
    ctx.globalAlpha = clamp(ga);
    ctx.drawImage(GRID, -((t * 14) % 48) - 48, -((t * 6) % 48) - 48);
    ctx.restore();
    drawStars(ctx, t);
    const [sx, sy] = shakeOffset(t, rt);
    ctx.save();
    ctx.translate(sx, sy);
    for (const d of draw) d(ctx, t);
    U.fxShock(ctx, t, SHOCKS);
    U.fxBursts(ctx, t, BURSTS);
    ctx.restore();
    let fl = 0;
    for (const f of FLASHES) if (t >= f.t) fl += f.a * pulse((t - f.t) * SLOW, f.k);
    fl += 0.5 * E.inExpo(P(t, K.outro - 0.2 / SLOW, K.outro)) * (t < K.outro ? 1 : 0);
    if (fl > 0.003) { ctx.fillStyle = `rgba(244,246,252,${clamp(fl)})`; ctx.fillRect(0, 0, W, H); }
    U.drawHUD(ctx, t, rt, { scenes: SCENES, dur: DUR, label });
    const fo = P(t, DUR - 0.8, DUR);
    if (fo > 0) { ctx.fillStyle = `rgba(0,0,0,${E.inOutSine(fo)})`; ctx.fillRect(0, 0, W, H); }
  }
  return { frame, setSlow, K };
}

module.exports = { slide, panel, enter, status, caption, pill, fitText, bubble, code, SYN, header, makeIntro, makeCards, makeOutro, makeReel };
