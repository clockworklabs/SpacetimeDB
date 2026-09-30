// Small widgets: a centered command chip and a connection status pill.
const L = require('../lib');
const { C, MONO, hexA, rr, measure, text } = L;
const { TAU, spinner } = require('../ui');
const { typedCommand } = require('./terminal');

// A rounded bar centered on cx with "$ <cmd>" typed out between c0 and c1 (omit c0 to show it whole). top = the bar's top edge.
function commandChip(ctx, cmd, cx, top, u, c0, c1, o = {}) {
  const size = o.size ?? 22;
  const w = measure(ctx, cmd, 500, size, MONO) + 100;
  rr(ctx, cx - w / 2, top, w, 64, 32); ctx.fillStyle = '#121A1F'; ctx.fill();
  ctx.strokeStyle = o.stroke ?? hexA(C.white, 0.15); ctx.lineWidth = 1.5; ctx.stroke();
  typedCommand(ctx, cmd, cx - w / 2 + 32, top + 40, u, c0, c1, { size });
}

// Connection status pill, right-aligned at `right`. state: 'online' (green dot), 'lost' (red dot) or 'retry' (spinner).
const STATUS_LABEL = { online: 'connected', lost: 'connection lost', retry: 'reconnecting…' };
function statusPill(ctx, right, top, state, t, label = STATUS_LABEL[state]) {
  const lw = measure(ctx, label, 600, 16, MONO) + 62, sx = right - lw;
  rr(ctx, sx, top, lw, 38, 19); ctx.fillStyle = '#0d1317'; ctx.fill();
  ctx.strokeStyle = state === 'online' ? hexA(C.green, 0.5) : state === 'lost' ? hexA(C.red, 0.7) : hexA(C.white, 0.3); ctx.lineWidth = 1.5; ctx.stroke();
  if (state === 'retry') spinner(ctx, sx + 24, top + 19, 7, t, C.white);
  else { ctx.fillStyle = state === 'online' ? C.green : C.red; ctx.beginPath(); ctx.arc(sx + 24, top + 19, 6, 0, TAU); ctx.fill(); }
  text(ctx, label, sx + 42, top + 25, { size: 16, weight: 600 });
  return lw;
}

module.exports = { commandChip, statusPill };
