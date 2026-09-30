// SpacetimeDB v2.6 release video (v2.5.0 → v2.6.0), 35.5 s @ 120 BPM. See DECISIONS.md for what's shown and why.
const L = require('../lib');
const { C, SANS, MONO, clamp, lerp, P, E, pulse, hexA, rr, glass, measure, text } = L;
const U = require('../ui');
const { CX, TAU, check, spinner } = U;
const R = require('../release');
const { statusPill, slide, panel, enter, status, caption, pill, code, SYN, header } = R;

const DUR = 35.5;
const T = { intro: 0, recon: 5.5, cpp: 14.5, plus: 23.5, outro: 29.5, end: DUR };
const SCENES = [
  { t: T.intro, name: 'v2.6' },
  { t: T.recon, name: 'REACT RECONNECT' },
  { t: T.cpp, name: 'C++ QUERY BUILDER' },
  { t: T.plus, name: 'ALSO IN 2.6' },
  { t: T.outro, name: 'SPACETIMEDB' },
];

// ======================================================================
// REACT: the provider reconnects with backoff and re-subscribes
// ======================================================================
// scene-local u. Attempts after 1 s and 2 s fail (network still down); the 4 s one succeeds.
const RC = { drop: 1.4, back: 3.4, tries: [[2.0, false, '1 s'], [2.8, false, '2 s'], [4.2, true, '4 s']], msg: 4.8 };
const MSGS = [['alice', 'anyone up for a match?'], ['bob', 'in 5 min'], ['carol', 'count me in']];

function recState(u) {
  if (u < RC.drop) return 'online';
  if (u < RC.tries[2][0]) return u - RC.drop < 0.5 ? 'lost' : 'retry';
  return 'online';
}

function sRecon(ctx, t) {
  const u = t - T.recon, len = T.cpp - T.recon;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'REACT APPS', [{ s: 'React apps now' }, { s: 'reconnect automatically.', fill: 'brand' }]);

  // the app
  const B = { x: 140, y: 270, w: 920, h: 590 };
  const st = recState(u);
  ctx.save();
  enter(ctx, u, 0.25, 40);
  panel(ctx, B.x, B.y, B.w, B.h, 'my-chat.app');
  statusPill(ctx, B.x + B.w - 36, B.y + 88, st, t);
  text(ctx, '#lobby', B.x + 40, B.y + 116, { size: 24, weight: 800, fam: SANS });
  // messages; they dim while offline and a new one arrives after the reconnect
  const dim = st === 'online' ? 0 : 0.55;
  const msgs = MSGS.concat(u > RC.msg ? [['dave', 'back online, joining now']] : []);
  msgs.forEach(([who, s], i) => {
    const isNew = i === 3, a = isNew ? E.outExpo(clamp((u - RC.msg) / 0.35)) : 1;
    const flash = isNew ? pulse(u - RC.msg, 2.5) : 0;
    ctx.save();
    ctx.globalAlpha *= a * (1 - dim);
    ctx.translate(0, (1 - a) * 16);
    const my = B.y + 170 + i * 86;
    rr(ctx, B.x + 40, my, B.w - 80, 70, 12); ctx.fillStyle = hexA(C.white, 0.04 + 0.06 * flash); ctx.fill();
    if (isNew) { ctx.strokeStyle = hexA(C.green, 0.3 + 0.6 * flash); ctx.lineWidth = 1.5; ctx.stroke(); }
    text(ctx, who, B.x + 64, my + 30, { size: 17, weight: 700, fill: hexA(C.white, 0.55) });
    text(ctx, s, B.x + 64, my + 56, { size: 22, weight: 500, fam: SANS });
    ctx.restore();
  });
  // after the reconnect: subscriptions are applied again
  const ra = E.outCubic(P(u, RC.tries[2][0] + 0.2, RC.tries[2][0] + 0.5));
  if (ra > 0) {
    check(ctx, B.x + 44, B.y + B.h - 44, 14, C.green, ra);
    text(ctx, 'subscriptions back, no page reload', B.x + 72, B.y + B.h - 38, { size: 18, fill: C.green, alpha: ra });
  }
  ctx.restore();

  // reconnect attempts on a time line: 1 s, 2 s, 4 s … (doubling, up to 30 s)
  const G = { x: 1100, y: 270, w: 680, h: 590 };
  ctx.save();
  enter(ctx, u, 0.45, 40);
  glass(ctx, G.x, G.y, G.w, G.h, 20);
  text(ctx, 'RECONNECT ATTEMPTS', G.x + 36, G.y + 56, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  text(ctx, 'Backoff doubles, from 1 s up to 30 s', G.x + 36, G.y + 98, { size: 22, weight: 600, fam: SANS, fill: hexA(C.white, 0.85) });
  // network lane
  const lx0 = G.x + 60, lx1 = G.x + G.w - 60;
  const X = uu => lerp(lx0, lx1, clamp((uu - RC.drop + 0.4) / (RC.msg - RC.drop + 0.6)));
  const ny = G.y + 200;
  text(ctx, 'network', G.x + 36, ny - 26, { size: 16, weight: 600, fill: hexA(C.white, 0.5) });
  const now = X(u);
  ctx.lineWidth = 4; ctx.lineCap = 'round';
  // up, down, up again (drawn up to the playhead)
  const seg = (a, b, col) => { const xa = X(a), xb = Math.min(X(b), now); if (xb > xa) { ctx.strokeStyle = col; ctx.beginPath(); ctx.moveTo(xa, ny); ctx.lineTo(xb, ny); ctx.stroke(); } };
  seg(RC.drop - 0.4, RC.drop, hexA(C.green, 0.8));
  seg(RC.drop, RC.back, hexA(C.red, 0.8));
  seg(RC.back, RC.msg + 0.2, hexA(C.green, 0.8));
  ctx.lineCap = 'butt';
  // attempts
  const ay = G.y + 330;
  text(ctx, 'reconnect', G.x + 36, ay - 50, { size: 16, weight: 600, fill: hexA(C.white, 0.5) });
  ctx.fillStyle = hexA(C.white, 0.12); ctx.fillRect(lx0, ay, lx1 - lx0, 1.5);
  RC.tries.forEach(([at, ok, d], i) => {
    const lt = u - at;
    if (lt < 0) return;
    const k = E.outBack(clamp(lt / 0.25));
    const x = X(at);
    ctx.save();
    ctx.translate(x, ay); ctx.scale(k, k);
    ctx.fillStyle = ok ? C.green : C.red; ctx.beginPath(); ctx.arc(0, 0, 11, 0, TAU); ctx.fill();
    ctx.restore();
    text(ctx, `+${d}`, x, ay + 44, { size: 17, weight: 600, align: 'center', fill: ok ? C.green : hexA(C.white, 0.6), alpha: clamp(lt / 0.3) });
    text(ctx, ok ? 'connected' : 'failed', x, ay + 70, { size: 15, align: 'center', fill: ok ? C.green : hexA(C.white, 0.45), alpha: clamp(lt / 0.3) });
  });
  // playhead
  if (u > RC.drop - 0.4 && u < RC.msg + 0.6) { ctx.fillStyle = hexA(C.white, 0.35); ctx.fillRect(now, G.y + 150, 1.5, 290); }
  text(ctx, 'Apps using SpacetimeDBProvider', G.x + 36, G.y + G.h - 72, { size: 19, weight: 500, fam: SANS, fill: hexA(C.white, 0.7), alpha: E.outCubic(P(u, 5.0, 5.4)) });
  text(ctx, 'reconnect and re-subscribe on their own.', G.x + 36, G.y + G.h - 42, { size: 19, weight: 500, fam: SANS, fill: hexA(C.white, 0.7), alpha: E.outCubic(P(u, 5.0, 5.4)) });
  ctx.restore();
  ctx.restore();
}

// ======================================================================
// C++: views as type-checked queries
// ======================================================================
const CQ = { type: [0.7, 2.6], build: 3.0, built: 3.8 };
const CPP = [
  [['SPACETIMEDB_VIEW', SYN.fn], ['(', null], ['Query', SYN.type], ['<', null], ['User', SYN.type], ['>, online_member_users, Public,', null]],
  [['                 AnonymousViewContext', SYN.type], [' ctx) {', null]],
  [['    return ', SYN.kw], ['ctx.from[user_membership].', null], ['right_semijoin', SYN.fn], ['(', null]],
  [['        ctx.from[user],', null]],
  [['        [](', null], ['const auto', SYN.kw], ['& m, ', null], ['const auto', SYN.kw], ['& u) {', null]],
  [['            return ', SYN.kw], ['m.user_identity.', null], ['eq', SYN.fn], ['(u.identity);', null]],
  [['        })', null]],
  [['        .', null], ['where', SYN.fn], ['([](', null], ['const auto', SYN.kw], ['& u) { ', null], ['return ', SYN.kw], ['u.online; });', null]],
  [['}', null]],
];
const ONLINE = ['alice', 'dave', 'erin'];

function sCpp(ctx, t) {
  const u = t - T.cpp, len = T.plus - T.cpp;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'C++ MODULES', [{ s: 'Typed queries for' }, { s: 'C++ modules.', fill: 'brand' }]);

  const Ed = { x: 140, y: 270, w: 1000, h: 590 };
  ctx.save();
  enter(ctx, u, 0.25, 40);
  panel(ctx, Ed.x, Ed.y, Ed.w, Ed.h, 'src/lib.cpp');
  // lines reveal top to bottom while "typing"
  const shown = P(u, CQ.type[0], CQ.type[1]) * CPP.length;
  CPP.forEach((ln, i) => { const a = clamp(shown - i); if (a > 0) code(ctx, ln, Ed.x + 32, Ed.y + 112 + i * 38, 17, a); });
  ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(Ed.x, Ed.y + Ed.h - 96, Ed.w, 1);
  if (u > CQ.build) {
    text(ctx, '$ spacetime build', Ed.x + 32, Ed.y + Ed.h - 42, { size: 19, fill: hexA(C.white, 0.6) });
    status(ctx, Ed.x + 262, Ed.y + Ed.h - 48, u - CQ.build, t, CQ.built - CQ.build);
    if (u > CQ.built) text(ctx, 'filters and joins checked by the compiler', Ed.x + 294, Ed.y + Ed.h - 42, { size: 19, fill: C.green, alpha: E.outCubic(P(u, CQ.built, CQ.built + 0.3)) });
  }
  ctx.restore();

  // the view, live on a client
  const V = { x: 1180, y: 270, w: 600, h: 590 };
  ctx.save();
  enter(ctx, u, 0.45, 40);
  glass(ctx, V.x, V.y, V.w, V.h, 20);
  text(ctx, 'LIVE VIEW', V.x + 32, V.y + 56, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  text(ctx, 'online_member_users', V.x + 32, V.y + 104, { size: 26, weight: 700 });
  ONLINE.forEach((n, i) => {
    const a = E.outExpo(clamp((u - CQ.built - 0.2 - i * 0.12) / 0.35));
    if (a <= 0) return;
    ctx.save(); ctx.globalAlpha *= a; ctx.translate((1 - a) * 30, 0);
    const ry = V.y + 144 + i * 56;
    rr(ctx, V.x + 24, ry, V.w - 48, 46, 8); ctx.fillStyle = hexA(C.white, 0.04); ctx.fill();
    ctx.fillStyle = C.green; ctx.beginPath(); ctx.arc(V.x + 52, ry + 23, 6, 0, TAU); ctx.fill();
    text(ctx, n, V.x + 72, ry + 30, { size: 20, fill: hexA(C.white, 0.9) });
    text(ctx, 'member', V.x + V.w - 48, ry + 30, { size: 16, align: 'right', fill: hexA(C.white, 0.45) });
    ctx.restore();
  });
  // every module language now has it
  const la = u - 5.0;
  if (la > 0) {
    ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(V.x, V.y + 380, V.w, 1);
    text(ctx, 'QUERY BUILDER IN', V.x + 32, V.y + 426, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5), alpha: E.outCubic(clamp(la / 0.3)) });
    let x = V.x + 32;
    ['Rust', 'C#', 'TypeScript', 'C++'].forEach((s, i) => {
      const a = E.outExpo(clamp((la - i * 0.1) / 0.35));
      const w = measure(ctx, s, 700, 19, SANS) + 40;
      if (a > 0) {
        ctx.save(); ctx.globalAlpha *= a;
        pill(ctx, x, V.y + 480, s, { size: 19, weight: 700, fam: SANS, h: 46, stroke: s === 'C++' ? hexA(C.green, 0.8) : hexA(C.white, 0.25) });
        ctx.restore();
      }
      x += w + 12;
    });
  }
  ctx.restore();
  caption(ctx, 'C++ views can return typed queries, like Rust, C# and TypeScript already could.', u, 5.6, 960);
  ctx.restore();
}

// ======================================================================
// PLUS cards, then the outro
// ======================================================================
const PLUS = [
  { k: 'C# MODULES', title: ['Timestamp', 'primary keys'], d: ['C# tables can be keyed by time,', 'like Rust and TypeScript ones.'] },
];

const intro = R.makeIntro({ pre: 'v2.', from: '5', to: '6', sub: 'What’s new since 2.5', T });
const plus = R.makeCards({ start: T.plus, end: T.outro, kick: 'C# MODULES', parts: [{ s: 'Also in' }, { s: '2.6.', fill: 'brand' }], list: PLUS, toOutro: true });
const outro = R.makeOutro({ version: '2.6', T });
const { frame, setSlow } = R.makeReel({ DUR, T, SCENES, label: '/  RELEASE v2.6', draw: [intro, sRecon, sCpp, plus, outro], seed: 6 });

module.exports = { frame, setSlow, DUR, T, SCENES, RC, CQ, PLUS };
