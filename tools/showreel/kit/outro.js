// The release-video outro: logo, wordmark glint, "Version X is out.", upgrade command.
const L = require('../lib');
const { W, H, C, SANS, MONO, clamp, lerp, P, E, spring, pulse, hexA, rr, glass, brandGrad, font, riseText, measure, text, drawMark, A, createCanvas } = L;
const U = require('../ui');
const { CX, CY, TAU, riseParts, kicker, check, spinner, trafficLights } = U;

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

module.exports = { makeOutro };
