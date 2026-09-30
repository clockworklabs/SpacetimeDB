// Frame composition for a release video: background, starfield, FX, HUD, fades.
const L = require('../lib');
const { W, H, C, SANS, MONO, clamp, lerp, P, E, spring, pulse, hexA, rr, glass, brandGrad, font, riseText, measure, text, drawMark, A, createCanvas } = L;
const U = require('../ui');
const { CX, CY, TAU, riseParts, kicker, check, spinner, trafficLights } = U;

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

module.exports = { makeReel };
