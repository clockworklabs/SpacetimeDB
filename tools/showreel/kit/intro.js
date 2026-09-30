// The release-video intro: wordmark and version odometer.
const L = require('../lib');
const { W, H, C, SANS, MONO, clamp, lerp, P, E, spring, pulse, hexA, rr, glass, brandGrad, font, riseText, measure, text, drawMark, A, createCanvas } = L;
const U = require('../ui');
const { CX, CY, TAU, riseParts, kicker, check, spinner, trafficLights } = U;

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

module.exports = { makeIntro };
