// Before/after comparisons: two lanes showing the old behavior and the new one (used in 2.1, 2.2, 2.3, 2.5, 2.7, 2.8).
const L = require('../lib');
const { C, SANS, hexA, glass, text } = L;
const { enter } = require('./layout');

// A lane's frame: glass box (green-edged for the new version), version tag and a one-line title.
function laneFrame(ctx, x, y, w, h, { now, version, title, titleX = 102, titleSize = 22 }) {
  glass(ctx, x, y, w, h, 20, now ? { stroke0: hexA(C.green, 0.45) } : {});
  text(ctx, version, x + 32, y + 50, { size: 15, weight: 700, tracking: 4, fill: now ? C.green : hexA(C.white, 0.5) });
  text(ctx, title, x + titleX, y + 50, { size: titleSize, weight: 700, fam: SANS, fill: hexA(C.white, now ? 0.95 : 0.7) });
}

// Places the two lanes: old ([x, y, w, h], dimmed) and now ([x, y, w, h]), entering at enterAt[0] and enterAt[1].
// drawLane(ctx, x, y, w, h, now) draws one lane, usually starting with laneFrame().
function beforeAfter(ctx, u, drawLane, { old, now, enterAt = [0.3, 0.45] }) {
  ctx.save(); ctx.globalAlpha *= 0.85; enter(ctx, u, enterAt[0], 40); drawLane(ctx, ...old, false); ctx.restore();
  ctx.save(); enter(ctx, u, enterAt[1], 40); drawLane(ctx, ...now, true); ctx.restore();
}

module.exports = { laneFrame, beforeAfter };
