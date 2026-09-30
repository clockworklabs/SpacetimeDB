// SpacetimeDB v2.8 release video (published 2.7.0 = v2.7.0-hotfix3 → v2.8.0), 29.5 s @ 120 BPM.
// See DECISIONS.md for what's shown and why.
const L = require('../lib');
const { C, SANS, MONO, clamp, lerp, P, E, pulse, hexA, rr, glass, measure, text, drawMark } = L;
const U = require('../ui');
const { CX, TAU, check, spinner } = U;
const R = require('../release');
const { laneFrame, beforeAfter } = R;
const { slide, panel, enter, caption, pill, code, SYN, header } = R;

const DUR = 29.5;
const T = { intro: 0, sub: 5.5, tab: 15, outro: 23.5, end: DUR };
const SCENES = [
  { t: T.intro, name: 'v2.8' },
  { t: T.sub, name: 'SUBMODULES' },
  { t: T.tab, name: 'RECONNECT ON RETURN' },
  { t: T.outro, name: 'SPACETIMEDB' },
];

// ======================================================================
// SUBMODULES: mount another TypeScript module under a namespace
// ======================================================================
const SM = { type: [0.6, 1.8], snap: 2.6, names: 3.4 };
const TS = [
  [['import ', SYN.kw], ['* as authLib ', null], ['from ', SYN.kw], ["'auth_lib'", SYN.str], [';', null]],
  [['', null]],
  [['const ', SYN.kw], ['spacetimedb = ', null], ['schema', SYN.fn], ['({', null]],
  [['  players,', null]],
  [['  myauth: authLib,', null]],
  [['});', null]],
  [['export default ', SYN.kw], ['spacetimedb;', null]],
];
const NAMES = [['SQL', 'SELECT * FROM myauth.users'], ['Reducer', 'myauth.verify_token'], ['Client', 'tables.myauth.users']];

function box(ctx, x, y, w, h, title, items, o = {}) {
  rr(ctx, x, y, w, h, 16);
  ctx.fillStyle = o.fill ?? '#121A1F'; ctx.fill();
  ctx.strokeStyle = o.stroke ?? hexA(C.white, 0.2); ctx.lineWidth = 1.5; ctx.stroke();
  drawMark(ctx, x + 36, y + 38, 30, { color: hexA(C.white, 0.8) });
  text(ctx, title, x + 62, y + 46, { size: 22, weight: 700 });
  items.forEach(([k, s], i) => {
    text(ctx, k, x + 28, y + 96 + i * 38, { size: 14, weight: 700, tracking: 2, fill: hexA(C.white, 0.4) });
    text(ctx, s, x + 128, y + 96 + i * 38, { size: 19, fill: hexA(C.white, 0.85) });
  });
}

function sSub(ctx, t) {
  const u = t - T.sub, len = T.tab - T.sub;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'NEW · TYPESCRIPT SUBMODULES', [{ s: 'TypeScript modules can' }, { s: 'mount other modules.', fill: 'brand' }]);

  // code
  const Cd = { x: 140, y: 270, w: 700, h: 400 };
  ctx.save();
  enter(ctx, u, 0.25, 40);
  panel(ctx, Cd.x, Cd.y, Cd.w, Cd.h, 'my-game/src/index.ts');
  const shown = P(u, SM.type[0], SM.type[1]) * TS.length;
  TS.forEach((ln, i) => { const a = clamp(shown - i); if (a > 0) code(ctx, ln, Cd.x + 36, Cd.y + 116 + i * 38, 21, a); });
  const hl = E.outCubic(P(u, SM.snap - 0.3, SM.snap));
  if (hl > 0) { rr(ctx, Cd.x + 26, Cd.y + 116 + 4 * 38 - 28, (measure(ctx, '  myauth: authLib,', 500, 21, MONO) + 20) * hl, 40, 6); ctx.strokeStyle = hexA(C.green, 0.8); ctx.lineWidth = 1.5; ctx.stroke(); }
  ctx.restore();

  // the modules: auth_lib slides into a slot of my-game
  const G = { x: 900, y: 270, w: 880, h: 400 };
  ctx.save();
  enter(ctx, u, 0.45, 40);
  box(ctx, G.x, G.y, G.w, G.h, 'my-game', [['TABLE', 'players']]);
  // slot
  const slot = { x: G.x + 380, y: G.y + 120, w: 470, h: 250 };
  ctx.save();
  ctx.setLineDash([6, 6]); ctx.strokeStyle = hexA(C.white, 0.25 * (1 - P(u, SM.snap, SM.snap + 0.2))); ctx.lineWidth = 1.5;
  rr(ctx, slot.x, slot.y, slot.w, slot.h, 14); ctx.stroke();
  ctx.restore();
  text(ctx, 'myauth', slot.x + 18, slot.y - 14, { size: 17, weight: 700, fill: u > SM.snap ? C.green : hexA(C.white, 0.4) });
  const fly = E.inOutCubic(P(u, 1.2, SM.snap));
  const lx = lerp(slot.x + 60, slot.x, fly), ly = lerp(slot.y + 330, slot.y, fly);
  if (u > 1.0) {
    ctx.save();
    ctx.globalAlpha *= E.outCubic(P(u, 1.0, 1.3));
    const lock = pulse(u - SM.snap, 3) * (u > SM.snap ? 1 : 0);
    box(ctx, lx, ly, slot.w, slot.h, 'auth_lib', [['TABLE', 'users'], ['TABLE', 'sessions'], ['REDUCER', 'verify_token']], { stroke: u > SM.snap ? hexA(C.green, 0.4 + 0.5 * lock) : hexA(C.white, 0.3), fill: '#16222a' });
    ctx.restore();
  }
  ctx.restore();

  // namespaced names everywhere
  NAMES.forEach(([k, s], i) => {
    const lt = u - (SM.names + i * 0.2);
    if (lt < 0) return;
    const a = E.outExpo(clamp(lt / 0.35));
    ctx.save(); ctx.globalAlpha *= a; ctx.translate(0, (1 - a) * 16);
    const x = 140 + i * 560, y = 730;
    rr(ctx, x, y, 540, 70, 12); ctx.fillStyle = hexA(C.white, 0.04); ctx.fill();
    ctx.strokeStyle = hexA(C.white, 0.12); ctx.lineWidth = 1.5; ctx.stroke();
    text(ctx, k, x + 24, y + 30, { size: 13, weight: 700, tracking: 3, fill: hexA(C.white, 0.45) });
    R.fitText(ctx, s, x + 24, y + 56, 492, { size: 19, fam: MONO, weight: 500 });
    ctx.restore();
  });
  caption(ctx, 'A TypeScript module can mount another one: its tables, reducers and views come along.', u, 4.4, 890);
  if (u > 5.2) text(ctx, 'TypeScript modules · needs a 2.8 server', CX, 950, { size: 20, weight: 500, fam: SANS, align: 'center', fill: hexA(C.white, 0.5), alpha: E.outCubic(P(u, 5.2, 5.6)) });
  ctx.restore();
}

// ======================================================================
// RECONNECT ON RETURN: coming back to a background tab reconnects right away
// ======================================================================
const TB = { away: 1.2, back: 2.6, fix: 3.1 };

function tabLane(ctx, x, y, w, h, u, t, now) {
  laneFrame(ctx, x, y, w, h, { now, version: now ? 'v2.8' : 'v2.7', title: now ? 'Reconnects on return' : 'Waits for the next retry' });
  // mini browser
  const bx = x + 32, by = y + 86, bw = w - 64, bh = 250;
  const away = u > TB.away && u < TB.back;
  rr(ctx, bx, by, bw, bh, 12); ctx.fillStyle = away ? hexA(C.white, 0.02) : '#0f171b'; ctx.fill();
  ctx.strokeStyle = hexA(C.white, 0.15); ctx.lineWidth = 1.5; ctx.stroke();
  let st = 'live';
  if (u > TB.away) st = 'dead';
  if (u > TB.fix && now) st = 'live2';
  const stLbl = st === 'live' || st === 'live2' ? 'connected' : u > TB.back ? 'reconnecting…' : 'socket closed';
  const ok = st !== 'dead';
  const lw = measure(ctx, stLbl, 600, 15, MONO) + 56;
  rr(ctx, bx + bw - 20 - lw, by + 18, lw, 34, 17); ctx.fillStyle = '#0d1317'; ctx.fill();
  ctx.strokeStyle = ok ? hexA(C.green, 0.5) : hexA(C.white, 0.3); ctx.lineWidth = 1.5; ctx.stroke();
  if (!ok && u > TB.back) spinner(ctx, bx + bw - lw + 2, by + 35, 7, t, C.white);
  else { ctx.fillStyle = ok ? C.green : C.red; ctx.beginPath(); ctx.arc(bx + bw - lw + 2, by + 35, 6, 0, TAU); ctx.fill(); }
  text(ctx, stLbl, bx + bw - lw + 18, by + 40, { size: 15, weight: 600 });
  text(ctx, 'Live board', bx + 24, by + 44, { size: 20, weight: 800, fam: SANS, alpha: away ? 0.4 : 1 });
  const rows = [['alice', 3], ['bob', 5], ['carol', 2]];
  rows.forEach(([n, v], i) => {
    const ry = by + 76 + i * 52;
    const upd = now && i === 1 && u > TB.fix + 0.3;
    const fl = upd ? pulse(u - TB.fix - 0.3, 2.5) : 0;
    rr(ctx, bx + 20, ry, bw - 40, 42, 8); ctx.fillStyle = hexA(C.white, 0.03 + 0.07 * fl); ctx.fill();
    if (upd) { ctx.strokeStyle = hexA(C.green, 0.3 + 0.6 * fl); ctx.lineWidth = 1.5; ctx.stroke(); }
    text(ctx, n, bx + 40, ry + 28, { size: 18, fill: hexA(C.white, away ? 0.35 : 0.85) });
    text(ctx, String(upd ? 6 : v), bx + bw - 40, ry + 28, { size: 18, weight: 700, align: 'right', fill: hexA(C.white, away ? 0.35 : 0.9) });
  });
  if (away) text(ctx, 'tab in the background', bx + bw / 2, by + bh / 2 + 8, { size: 20, weight: 600, fam: SANS, align: 'center', fill: hexA(C.white, 0.7) });
  // what happens when the user comes back
  const ev = now ? [[TB.back, 'tab visible again'], [TB.fix, 'reconnects right away, live again']] : [[TB.back, 'tab visible again'], [TB.fix, 'waits for its next scheduled retry']];
  ev.forEach(([at, s], i) => {
    const a = E.outExpo(clamp((u - at) / 0.35));
    if (a <= 0) return;
    ctx.save(); ctx.globalAlpha *= a;
    const ey = by + bh + 50 + i * 40;
    if (i === 1 && now) check(ctx, x + 36, ey - 6, 14, C.green, a);
    else { ctx.fillStyle = hexA(C.white, 0.5); ctx.beginPath(); ctx.arc(x + 44, ey - 6, 4, 0, TAU); ctx.fill(); }
    text(ctx, s, x + 66, ey, { size: 19, fill: i === 1 && now ? C.green : hexA(C.white, 0.75) });
    ctx.restore();
  });
}

function sTab(ctx, t) {
  const u = t - T.tab, len = T.outro - T.tab;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'REACT, SVELTE AND SOLID APPS · 2.7.1', [{ s: 'Web apps reconnect when' }, { s: 'you return to the tab.', fill: 'brand' }]);
  beforeAfter(ctx, u, (ctx, x, y, w, h, now) => tabLane(ctx, x, y, w, h, u, t, now), { old: [140, 290, 800, 470], now: [980, 290, 800, 470], enterAt: [0.3, 0.45] });
  caption(ctx, 'Returning to a tab, waking the laptop or getting Wi-Fi back now reconnects right away.', u, 4.0, 870);
  ctx.restore();
}

// ======================================================================
// PLUS cards, then the outro
// ======================================================================
const intro = R.makeIntro({ pre: 'v2.', from: '7', to: '8', sub: 'What’s new since 2.7', T });
const outro = R.makeOutro({ version: '2.8', T });
const { frame, setSlow } = R.makeReel({ DUR, T, SCENES, label: '/  RELEASE v2.8', draw: [intro, sSub, sTab, outro], seed: 8 });

module.exports = { frame, setSlow, DUR, T, SCENES, SM, TB };
