// Card screens: 1 to 5 small feature cards under a headline.
const L = require('../lib');
const { W, H, C, SANS, MONO, clamp, lerp, P, E, spring, pulse, hexA, rr, glass, brandGrad, font, riseText, measure, text, drawMark, A, createCanvas } = L;
const U = require('../ui');
const { CX, CY, TAU, riseParts, kicker, check, spinner, trafficLights } = U;
const { slide, fitText, header } = require('./layout');

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

module.exports = { makeCards };
