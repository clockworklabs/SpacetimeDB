// Agent chat: a window where a user talks to an AI agent that calls SpacetimeDB tools.
// Used by the MCP scenes (2.7, 2.9, 2.10). Pair with a panel on the right showing the effect, joined by linkPulses().
const L = require('../lib');
const { C, SANS, MONO, clamp, lerp, P, E, pulse, hexA, rr, measure, text } = L;
const { TAU } = require('../ui');
const { panel, enter, status, bubble } = require('./layout');

// chat: [{ u, kind: 'user' | 'tool' | 'agent', s, d? }] with scene-local times u.
//   user  → a bubble on the right; agent → a reply typed out on the left;
//   tool  → a pill "spacetimedb · <s>  <d>" whose spinner turns into a check after toolRun seconds.
// ys: one baseline per message. The window enters at enterAt.
function agentChat(ctx, u, t, { x, y, w, h, title, chat, ys, toolRun = 0.45, enterAt = 0.25 }) {
  ctx.save();
  enter(ctx, u, enterAt, 40);
  panel(ctx, x, y, w, h, title);
  chat.forEach((m, i) => {
    const lt = u - m.u;
    if (lt < 0) return;
    const a = E.outExpo(clamp(lt / 0.35));
    const my = ys[i];
    if (m.kind === 'user') bubble(ctx, m.s, x + w - 36, my, 'right', a);
    else if (m.kind === 'agent') {
      ctx.save();
      ctx.globalAlpha *= a;
      ctx.fillStyle = C.white; ctx.beginPath(); ctx.arc(x + 48, my - 1, 6, 0, TAU); ctx.fill();
      const n = Math.floor(P(lt, 0, 0.5) * m.s.length);
      text(ctx, m.s.slice(0, n), x + 70, my + 8, { size: 26, weight: 500, fam: SANS });
      ctx.restore();
    } else {
      const label = `spacetimedb · ${m.s}`;
      const w1 = measure(ctx, label, 700, 17, MONO), w2 = measure(ctx, m.d, 400, 16, MONO);
      const pw = w1 + w2 + 96;
      ctx.save();
      ctx.globalAlpha *= a;
      ctx.translate((1 - a) * 20, 0);
      rr(ctx, x + 36, my - 22, pw, 44, 22);
      ctx.fillStyle = '#0d1317'; ctx.fill();
      ctx.strokeStyle = lt > toolRun ? hexA(C.green, 0.25 + 0.5 * pulse(lt - toolRun, 3)) : hexA(C.white, 0.2); ctx.lineWidth = 1.5; ctx.stroke();
      status(ctx, x + 54, my + 1, lt, t, toolRun);
      text(ctx, label, x + 84, my + 6, { size: 17, weight: 700 });
      text(ctx, m.d, x + 84 + w1 + 14, my + 6, { size: 16, fill: hexA(C.white, 0.55) });
      ctx.restore();
    }
  });
  ctx.restore();
}

// A horizontal line from x0 to x1 at height y; at each time in `calls`, three green dots travel there and back.
function linkPulses(ctx, u, { x0, x1, y, calls, appear = [0.6, 0.9] }) {
  ctx.save();
  ctx.globalAlpha *= E.outCubic(P(u, appear[0], appear[1]));
  ctx.strokeStyle = hexA(C.white, 0.2); ctx.lineWidth = 1.5;
  ctx.beginPath(); ctx.moveTo(x0, y); ctx.lineTo(x1, y); ctx.stroke();
  for (const at of calls) {
    const lt = u - at;
    for (let k = 0; k < 3; k++) {
      const p = (lt - k * 0.1) / 0.35;
      if (p < 0 || p > 2) continue;
      const px = p <= 1 ? lerp(x0, x1, p) : lerp(x1, x0, p - 1);
      ctx.fillStyle = C.green; ctx.beginPath(); ctx.arc(px, y, 4, 0, TAU); ctx.fill();
    }
  }
  ctx.restore();
}

module.exports = { agentChat, linkPulses };
