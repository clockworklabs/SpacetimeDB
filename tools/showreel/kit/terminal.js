// Terminal pieces: a typed command line, a result line (check or cross + text) and the migration plan's "▸" marker.
const L = require('../lib');
const { C, P, hexA, text } = L;
const { check } = require('../ui');

// "$ <cmd>" typed out between scene-local times c0 and c1 (omit c0 to show it whole). The prompt sits size+6 px before the command.
function typedCommand(ctx, cmd, x, y, u, c0, c1, o = {}) {
  const size = o.size ?? 20;
  text(ctx, '$', x, y, { size, weight: 600, fill: hexA(C.white, 0.5) });
  const n = c0 == null ? cmd.length : Math.floor(P(u, c0, c1) * cmd.length);
  text(ctx, cmd.slice(0, n), x + size + 6, y, { size });
}

// A result under a command: green check (ok) or red cross, then the text `gap` px to the right, fading in with alpha a.
function resultLine(ctx, s, x, y, a, o = {}) {
  const ok = o.ok ?? true, size = o.size ?? 18, gap = o.gap ?? 28;
  if (ok) check(ctx, x, y - 6, 14, C.green, a);
  else {
    ctx.save(); ctx.globalAlpha *= a;
    ctx.strokeStyle = C.red; ctx.lineWidth = 2.5; ctx.beginPath();
    ctx.moveTo(x, y - 14); ctx.lineTo(x + 14, y); ctx.moveTo(x + 14, y - 14); ctx.lineTo(x, y); ctx.stroke();
    ctx.restore();
  }
  if (s) text(ctx, s, x + gap, y, { size, weight: o.weight, fill: o.fill ?? (ok ? C.green : C.red), alpha: a });
}

// The "▸" marker that prefixes migration-plan lines (the mono font has no glyph for it). (x, y) = baseline-left of the text.
function planMarker(ctx, x, y, col, a = 1) {
  ctx.save(); ctx.globalAlpha *= a; ctx.fillStyle = col;
  ctx.beginPath(); ctx.moveTo(x, y - 12); ctx.lineTo(x + 10, y - 6); ctx.lineTo(x, y); ctx.closePath(); ctx.fill();
  ctx.restore();
}

module.exports = { typedCommand, resultLine, planMarker };
