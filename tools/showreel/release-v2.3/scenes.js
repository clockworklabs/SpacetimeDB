// SpacetimeDB v2.3 release video (v2.2.0 → v2.3.0), 37.5 s @ 120 BPM. See DECISIONS.md for what's shown and why.
const L = require('../lib');
const { C, SANS, MONO, clamp, lerp, P, E, pulse, hexA, rr, glass, measure, text, rng } = L;
const U = require('../ui');
const { CX, TAU, check, spinner } = U;
const R = require('../release');
const { laneFrame, beforeAfter, resultLine, typedCommand, slide, panel, enter, caption, pill, header } = R;

const DUR = 37.5;
const T = { intro: 0, godot: 5.5, pipe: 15, plus: 24, outro: 31.5, end: DUR };
const SCENES = [
  { t: T.intro, name: 'v2.3' },
  { t: T.godot, name: 'GODOT' },
  { t: T.pipe, name: 'PIPELINING' },
  { t: T.plus, name: 'ALSO IN 2.3' },
  { t: T.outro, name: 'SPACETIMEDB' },
];

// ======================================================================
// GODOT: an official SDK package + the Blackholio tutorial game
// ======================================================================
const GD = { cmd: [0.6, 1.8], added: 2.1, play: 2.6 };
const GCMD = 'dotnet add package SpacetimeDB.ClientSDK.Godot';
const FILES = [['res://', 0], ['scenes/', 1], ['scripts/', 1], ['GameManager.cs', 2], ['PlayerController.cs', 2], ['module_bindings/', 2]];
const FOOD = (() => { const r = rng(23), a = []; for (let i = 0; i < 70; i++) a.push([r(), r(), r()]); return a; })();
const BLOBS = [['alice', 46, 0.3, 0.4, 0.9], ['bob', 34, 0.7, 0.6, 1.3], ['carol', 26, 0.5, 0.25, 1.7]];

// The Blackholio arena at (x, y, w, h); `k` = seconds of play.
function arena(ctx, x, y, w, h, k, s = 1) {
  ctx.save();
  rr(ctx, x, y, w, h, 10); ctx.clip();
  ctx.fillStyle = '#0a0f12'; ctx.fillRect(x, y, w, h);
  ctx.fillStyle = hexA(C.white, 0.05);
  for (let gx = x; gx < x + w; gx += 40 * s) ctx.fillRect(gx, y, 1, h);
  for (let gy = y; gy < y + h; gy += 40 * s) ctx.fillRect(x, gy, w, 1);
  // food, eaten as blobs pass over it
  const pos = BLOBS.map(([, r, bx, by, sp]) => [x + w * (bx + 0.22 * Math.sin(k * 0.5 * sp + bx * 6)), y + h * (by + 0.18 * Math.cos(k * 0.4 * sp + by * 5)), r * s * (1 + 0.04 * k)]);
  FOOD.forEach(([fx, fy, c], i) => {
    const px = x + fx * w, py = y + fy * h;
    if (pos.some(([bx, by, br]) => Math.hypot(px - bx, py - by) < br)) return;
    ctx.fillStyle = [C.green, C.white, hexA(C.white, 0.6)][i % 3]; ctx.beginPath(); ctx.arc(px, py, 3.5 * s, 0, TAU); ctx.fill();
  });
  BLOBS.forEach(([n], i) => {
    const [bx, by, br] = pos[i];
    ctx.fillStyle = hexA(i === 0 ? C.green : C.white, i === 0 ? 0.35 : 0.18); ctx.beginPath(); ctx.arc(bx, by, br, 0, TAU); ctx.fill();
    ctx.strokeStyle = hexA(i === 0 ? C.green : C.white, 0.8); ctx.lineWidth = 2 * s; ctx.stroke();
    text(ctx, n, bx, by + 6 * s, { size: 15 * s, weight: 700, fam: SANS, align: 'center' });
  });
  ctx.restore();
}

function sGodot(ctx, t) {
  const u = t - T.godot, len = T.pipe - T.godot;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'NEW · GODOT SDK', [{ s: 'An official SDK' }, { s: 'for Godot.', fill: 'brand' }]);

  // editor mock: file dock + terminal
  const Ed = { x: 140, y: 270, w: 700, h: 600 };
  ctx.save();
  enter(ctx, u, 0.25, 40);
  panel(ctx, Ed.x, Ed.y, Ed.w, Ed.h, 'Godot 4.6 (.NET) · blackholio');
  text(ctx, 'FILESYSTEM', Ed.x + 32, Ed.y + 104, { size: 13, weight: 700, tracking: 3, fill: hexA(C.white, 0.45) });
  FILES.forEach(([f, d], i) => text(ctx, f, Ed.x + 40 + d * 24, Ed.y + 144 + i * 34, { size: 18, fill: hexA(C.white, f.endsWith('.cs') ? 0.9 : 0.6) }));
  ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(Ed.x, Ed.y + 380, Ed.w, 1);
  text(ctx, 'TERMINAL', Ed.x + 32, Ed.y + 420, { size: 13, weight: 700, tracking: 3, fill: hexA(C.white, 0.45) });
  if (u > GD.cmd[0] - 0.05) {
    typedCommand(ctx, GCMD, Ed.x + 32, Ed.y + 466, u, GD.cmd[0], GD.cmd[1], { size: 18 });
  }
  if (u > GD.added) {
    const a = E.outCubic(P(u, GD.added, GD.added + 0.3));
    resultLine(ctx, 'SpacetimeDB SDK added', Ed.x + 58, Ed.y + 510, a);
  }
  ctx.restore();

  // the game, running in two windows that share one database
  const G = { x: 880, y: 270, w: 900, h: 600 };
  ctx.save();
  enter(ctx, u, 0.45, 40);
  glass(ctx, G.x, G.y, G.w, G.h, 20);
  text(ctx, 'Blackhol.io', G.x + 32, G.y + 52, { size: 24, weight: 800, fam: SANS });
  text(ctx, 'the tutorial game', G.x + 200, G.y + 52, { size: 18, fam: SANS, fill: hexA(C.white, 0.5) });
  const k = Math.max(0, u - GD.play);
  const pa = E.outCubic(P(u, GD.play, GD.play + 0.4));
  ctx.save(); ctx.globalAlpha *= 0.3 + 0.7 * pa;
  arena(ctx, G.x + 24, G.y + 80, 560, 496, k);
  arena(ctx, G.x + 604, G.y + 80, 272, 240, k, 0.5);
  ctx.restore();
  text(ctx, 'second player', G.x + 604, G.y + 350, { size: 16, fam: SANS, fill: hexA(C.white, 0.55), alpha: pa });
  text(ctx, 'same world, live', G.x + 604, G.y + 376, { size: 16, fam: SANS, fill: hexA(C.white, 0.55), alpha: pa });
  ctx.restore();
  caption(ctx, 'An official Godot SDK and a step-by-step tutorial to build a multiplayer game.', u, 4.4, 950);
  if (u > 5.2) text(ctx, 'Godot 4.6.2+ with .NET · modules in Rust or C#', CX, 1000, { size: 20, weight: 500, fam: SANS, align: 'center', fill: hexA(C.white, 0.5), alpha: E.outCubic(P(u, 5.2, 5.6)) });
  ctx.restore();
}

// ======================================================================
// PIPELINING: requests no longer wait on each other; replies are batched
// ======================================================================
const PL = { start: 1.0, end: 7.2 };

// A small request/response box travelling along a lane.
function packet(ctx, x, y, col, w = 30, label) {
  rr(ctx, x - w / 2, y - 13, w, 26, 6); ctx.fillStyle = hexA(col, 0.22); ctx.fill();
  ctx.strokeStyle = hexA(col, 0.9); ctx.lineWidth = 1.5; ctx.stroke();
  if (label) text(ctx, label, x, y + 5, { size: 13, weight: 700, align: 'center', fill: col });
}

function pipeLane(ctx, x, y, w, h, u, t, now) {
  laneFrame(ctx, x, y, w, h, { now, version: now ? 'v2.3' : 'v2.2', title: now ? 'Next request goes right in' : 'Each request waits its turn' });
  const x0 = x + 150, x1 = x + w - 170, ly = y + 130, ry = y + 175;
  text(ctx, 'client', x + 36, ly + 30, { size: 17, weight: 600, fill: hexA(C.white, 0.6) });
  rr(ctx, x1 + 20, ly - 34, 120, 110, 12); ctx.fillStyle = hexA(C.white, 0.05); ctx.fill();
  ctx.strokeStyle = hexA(C.white, 0.2); ctx.lineWidth = 1.5; ctx.stroke();
  text(ctx, 'database', x1 + 80, ly + 30, { size: 16, weight: 600, align: 'center', fill: hexA(C.white, 0.7) });
  ctx.fillStyle = hexA(C.white, 0.1); ctx.fillRect(x0, ly, x1 - x0, 1.5); ctx.fillRect(x0, ry, x1 - x0, 1.5);
  const lt = u - PL.start;
  if (lt < 0 || u > PL.end + 0.6) return;
  const go = 0.6, back = 0.6;
  ctx.save();
  ctx.beginPath(); ctx.rect(x0 - 20, y + 90, x1 - x0 + 40, h - 100); ctx.clip();
  if (!now) {
    // stop-and-wait: one round trip at a time
    const cyc = go + 0.3 + back;
    const k = Math.floor(lt / cyc), f = lt - k * cyc;
    if (f < go) packet(ctx, lerp(x0, x1, f / go), ly, C.white);
    else if (f < go + 0.3) spinner(ctx, x1 - 16, ly, 8, t, C.white);
    else packet(ctx, lerp(x1, x0, (f - go - 0.3) / back), ry, C.green);
    // the next requests queue up behind the one in flight
    for (let q = 0; q < 3; q++) packet(ctx, x0 + 16 + q * 34, ly + 44, hexA(C.white, 0.5), 24);
    text(ctx, 'queued', x0 + 124, ly + 50, { size: 14, fill: hexA(C.white, 0.45) });
  } else {
    // pipelined: a request every 0.15 s; replies come back several per frame
    for (let k = 0; k < 60; k++) {
      const s = k * 0.15;
      const f = lt - s;
      if (f < 0) break;
      if (k === 6) {
        // a slow procedure steps aside to wait on an outbound HTTP call
        if (f < go) packet(ctx, lerp(x0, x1, f / go), ly, C.white, 90, 'procedure');
        else if (f < go + 2.2) { packet(ctx, x1 - 60, ly + 44, C.white, 90, 'procedure'); spinner(ctx, x1 + 4, ly + 44, 7, t, C.white); }
        else if (f < go + 2.2 + back) packet(ctx, lerp(x1, x0, (f - go - 2.2) / back), ry, C.green, 90, 'procedure');
        continue;
      }
      if (f < go) packet(ctx, lerp(x0, x1, f / go), ly, C.white);
    }
    // batched replies
    for (let b = 0; b < 20; b++) {
      const s = go + 0.1 + b * 0.6;
      const f = lt - s;
      if (f < 0 || f > back) continue;
      packet(ctx, lerp(x1, x0, f / back), ry, C.green, 96, '4 replies');
    }
  }
  ctx.restore();
}

function sPipe(ctx, t) {
  const u = t - T.pipe, len = T.plus - T.pipe;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'SERVER · PIPELINING', [{ s: 'Requests no longer' }, { s: 'wait in line.', fill: 'brand' }]);
  beforeAfter(ctx, u, (ctx, x, y, w, h, now) => pipeLane(ctx, x, y, w, h, u, t, now), { old: [140, 270, 1640, 260], now: [140, 560, 1640, 260], enterAt: [0.3, 0.45] });
  caption(ctx, 'The server takes the next request without waiting, and a slow procedure no longer holds the line.', u, 4.4, 900);
  if (u > 5.2) text(ctx, 'Several replies per network message: TypeScript SDK clients', CX, 950, { size: 20, weight: 500, fam: SANS, align: 'center', fill: hexA(C.white, 0.5), alpha: E.outCubic(P(u, 5.2, 5.6)) });
  ctx.restore();
}

// ======================================================================
// PLUS cards, then the outro
// ======================================================================
const PLUS = [
  { k: 'VUE AND TANSTACK', title: ['useProcedure,', 'beyond React'], d: ['Call procedures with one typed', 'hook, like React apps already can.'] },
  { k: 'UNITY SDK', title: ['Unity 6', 'WebGL builds'], d: ['Web builds made with Unity 6', 'can connect to SpacetimeDB.'] },
  { k: 'SPACETIME INIT', title: ['AI rules for', 'every language'], d: ['Generated agent instructions now', 'cover C++, Unity and Unreal too.'] },
];

const intro = R.makeIntro({ pre: 'v2.', from: '2', to: '3', sub: 'What’s new since 2.2', T });
const plus = R.makeCards({ start: T.plus, end: T.outro, kick: 'SDKS AND TOOLING', parts: [{ s: 'Also in' }, { s: '2.3.', fill: 'brand' }], list: PLUS, toOutro: true });
const outro = R.makeOutro({ version: '2.3', T });
const { frame, setSlow } = R.makeReel({ DUR, T, SCENES, label: '/  RELEASE v2.3', draw: [intro, sGodot, sPipe, plus, outro], seed: 3 });

module.exports = { frame, setSlow, DUR, T, SCENES, GD, PL, PLUS };
