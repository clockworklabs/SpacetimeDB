// SpacetimeDB v2.1 release video (v2.0.1 → v2.1.0), 53 s @ 120 BPM. See DECISIONS.md for what's shown and why.
const L = require('../lib');
const { C, SANS, MONO, clamp, lerp, P, E, pulse, hexA, rr, glass, measure, text } = L;
const U = require('../ui');
const { CX, TAU, check, spinner } = U;
const R = require('../release');
const { laneFrame, beforeAfter } = R;
const { slide, panel, enter, caption, pill, code, SYN, header } = R;

const DUR = 53;
const T = { intro: 0, rust: 5.5, http: 14.5, unreal: 23.5, cli: 32, more: 39.5, outro: 47, end: DUR };
const SCENES = [
  { t: T.intro, name: 'v2.1' },
  { t: T.rust, name: 'RUST IN THE BROWSER' },
  { t: T.http, name: 'HTTP TIMEOUTS' },
  { t: T.unreal, name: 'UNREAL ON 2.0' },
  { t: T.cli, name: 'CLI' },
  { t: T.more, name: 'ALSO IN 2.1' },
  { t: T.outro, name: 'SPACETIMEDB' },
];

// ======================================================================
// RUST IN THE BROWSER: the Rust client SDK compiles to wasm with the `browser` feature
// ======================================================================
const RB = { live: 2.6, row: 4.0 };
const TOML = [
  [['[dependencies]', null]],
  [['spacetimedb-sdk', null], [' = { version = ', null], ['"2.1"', SYN.str], [', default-features = ', null], ['false', SYN.kw], [',', null]],
  [['                  features = [', null], ['"browser"', SYN.str], ['] }', null]],
];
const RS = [
  [['let ', SYN.kw], ['conn = ', null], ['DbConnection', SYN.type], ['::', null], ['builder', SYN.fn], ['()', null]],
  [['    .', null], ['with_uri', SYN.fn], ['(', null], ['"http://localhost:3000"', SYN.str], [')', null]],
  [['    .', null], ['with_database_name', SYN.fn], ['(', null], ['"my-game"', SYN.str], [')', null]],
  [['    .', null], ['build', SYN.fn], ['()', null]],
  [['    .await', SYN.kw], ['?;', null]],
  [['conn.', null], ['run_background_task', SYN.fn], ['();', null]],
];
const LIVE = ['alice joined', 'bob joined', 'carol joined'];

function liveWin(ctx, x, y, w, h, title, sub, u) {
  panel(ctx, x, y, w, h, title);
  text(ctx, sub, x + 32, y + 104, { size: 15, weight: 700, tracking: 3, fill: hexA(C.white, 0.45) });
  const la = E.outCubic(P(u, RB.live, RB.live + 0.3));
  ctx.fillStyle = la > 0 ? C.green : hexA(C.white, 0.3); ctx.beginPath(); ctx.arc(x + w - 44, y + 98, 6, 0, TAU); ctx.fill();
  const rows = LIVE.slice(0, u > RB.row ? 3 : u > RB.live ? 2 : 0);
  rows.forEach((s, i) => {
    const isNew = i === 2;
    const a = isNew ? E.outExpo(clamp((u - RB.row) / 0.35)) : la;
    const fl = isNew ? pulse(u - RB.row, 2.5) : 0;
    ctx.save(); ctx.globalAlpha *= a; ctx.translate((1 - a) * 20, 0);
    const ry = y + 130 + i * 52;
    rr(ctx, x + 24, ry, w - 48, 42, 8); ctx.fillStyle = hexA(C.white, 0.04 + 0.07 * fl); ctx.fill();
    if (isNew) { ctx.strokeStyle = hexA(C.green, 0.3 + 0.6 * fl); ctx.lineWidth = 1.5; ctx.stroke(); }
    text(ctx, s, x + 48, ry + 28, { size: 19, fill: hexA(C.white, 0.85) });
    ctx.restore();
  });
}

function sRust(ctx, t) {
  const u = t - T.rust, len = T.http - T.rust;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'RUST CLIENT SDK', [{ s: 'Rust clients now' }, { s: 'run in the browser.', fill: 'brand' }]);
  const Tl = { x: 140, y: 270, w: 960, h: 200 };
  ctx.save(); enter(ctx, u, 0.25, 40);
  panel(ctx, Tl.x, Tl.y, Tl.w, Tl.h, 'Cargo.toml');
  TOML.forEach((ln, i) => code(ctx, ln, Tl.x + 32, Tl.y + 110 + i * 34, 18));
  const hb = E.outCubic(P(u, 1.0, 1.4));
  if (hb > 0) { const x = Tl.x + 32 + measure(ctx, '                  features = [', 500, 18, MONO) - 4; rr(ctx, x, Tl.y + 110 + 2 * 34 - 24, (measure(ctx, '"browser"', 500, 18, MONO) + 8) * hb, 34, 6); ctx.strokeStyle = hexA(C.green, 0.8); ctx.lineWidth = 1.5; ctx.stroke(); }
  ctx.restore();
  const Cd = { x: 140, y: 500, w: 960, h: 360 };
  ctx.save(); enter(ctx, u, 0.45, 40);
  panel(ctx, Cd.x, Cd.y, Cd.w, Cd.h, 'src/lib.rs');
  RS.forEach((ln, i) => code(ctx, ln, Cd.x + 32, Cd.y + 108 + i * 34, 19));
  text(ctx, 'In the browser, build() is async.', Cd.x + 32, Cd.y + Cd.h - 28, { size: 18, fam: SANS, fill: hexA(C.white, 0.55), alpha: E.outCubic(P(u, 1.8, 2.2)) });
  ctx.restore();
  ctx.save(); enter(ctx, u, 0.6, 40); liveWin(ctx, 1140, 270, 640, 290, 'my-game (native)', 'DESKTOP', u); ctx.restore();
  ctx.save(); enter(ctx, u, 0.75, 40); liveWin(ctx, 1140, 580, 640, 290, 'localhost:8080', 'BROWSER · WASM', u); ctx.restore();
  caption(ctx, 'Rust clients can compile to WebAssembly and connect straight from a web page.', u, 5.0, 950);
  ctx.restore();
}

// ======================================================================
// HTTP TIMEOUTS: procedure HTTP calls wait 30 s by default, up to 180 s
// ======================================================================
const HT = { run: [1.2, 4.6], reply: 12 }; // scene-local sweep; the reply arrives at 12 s (simulated)
const AX = 40; // seconds shown on the axis

function tLane(ctx, x, y, w, h, u, now) {
  laneFrame(ctx, x, y, w, h, { now, version: now ? 'v2.1' : 'v2.0', title: now ? '30 s by default, up to 3 minutes' : '0.5 s by default, 10 s at most' });
  const x0 = x + 60, x1 = x + w - 60, ly = y + 140;
  const X = s => lerp(x0, x1, s / AX);
  ctx.fillStyle = hexA(C.white, 0.12); ctx.fillRect(x0, ly + 30, x1 - x0, 1.5);
  [0, 10, 20, 30, 40].forEach(s => { ctx.fillStyle = hexA(C.white, 0.2); ctx.fillRect(X(s), ly + 24, 1.5, 12); text(ctx, `${s} s`, X(s), ly + 60, { size: 14, align: 'center', fill: hexA(C.white, 0.4) }); });
  // limit markers
  const marks = now ? [[30, 'default']] : [[0.5, 'default'], [10, 'max']];
  marks.forEach(([s, l]) => { ctx.fillStyle = hexA(now ? C.green : C.red, 0.7); ctx.fillRect(X(s), ly - 30, 2, 60); text(ctx, l, X(s) + 8, ly - 18, { size: 14, weight: 600, fill: hexA(now ? C.green : C.red, 0.9) }); });
  if (now) text(ctx, 'max 180 s →', x1, ly - 18, { size: 14, weight: 600, align: 'right', fill: hexA(C.green, 0.9) });
  // the request bar
  const sim = lerp(0, 16, P(u, HT.run[0], HT.run[1]));
  if (u > HT.run[0]) {
    const end = now ? Math.min(sim, HT.reply) : Math.min(sim, 0.5);
    rr(ctx, X(0), ly - 12, Math.max(6, X(end) - X(0)), 24, 6);
    ctx.fillStyle = hexA(now ? C.green : C.red, 0.2); ctx.fill(); ctx.strokeStyle = hexA(now ? C.green : C.red, 0.8); ctx.lineWidth = 1.5; ctx.stroke();
    if (!now && sim > 0.5) text(ctx, 'timed out before the answer', X(0.5) + 16, ly + 6, { size: 17, weight: 600, fill: C.red, alpha: E.outCubic(P(sim, 0.5, 2)) });
    if (now && sim > HT.reply) { check(ctx, X(HT.reply) + 10, ly, 14, C.green, E.outCubic(P(sim, HT.reply, HT.reply + 1))); text(ctx, 'LLM answered at 12 s', X(HT.reply) + 34, ly + 6, { size: 17, weight: 600, fill: C.green, alpha: E.outCubic(P(sim, HT.reply, HT.reply + 1)) }); }
  }
}

function sHttp(ctx, t) {
  const u = t - T.http, len = T.unreal - T.http;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'PROCEDURES · OUTBOUND HTTP', [{ s: 'Longer HTTP timeouts' }, { s: 'for procedures.', fill: 'brand' }]);
  ctx.save(); enter(ctx, u, 0.25, 30);
  const ln = [['const ', SYN.kw], ['reply = ctx.http.', null], ['fetch', SYN.fn], ['(', null], ["'https://llm.example.com/v1/chat'", SYN.str], [', …);', null]];
  const w = ln.reduce((s, [p]) => s + measure(ctx, p, 500, 21, MONO), 0);
  rr(ctx, CX - w / 2 - 32, 260, w + 64, 62, 14); ctx.fillStyle = '#121A1F'; ctx.fill(); ctx.strokeStyle = hexA(C.white, 0.12); ctx.lineWidth = 1.5; ctx.stroke();
  code(ctx, ln, CX - w / 2, 299, 21);
  ctx.restore();
  beforeAfter(ctx, u, (ctx, x, y, w, h, now) => tLane(ctx, x, y, w, h, u, now), { old: [140, 350, 1640, 230], now: [140, 600, 1640, 230], enterAt: [0.4, 0.55] });
  caption(ctx, 'HTTP calls from procedures now wait long enough for slow APIs, and failures say why.', u, 5.2, 900);
  if (u > 6.0) text(ctx, 'e.g. “error trying to connect: dns error: …”', CX, 955, { size: 20, align: 'center', fill: hexA(C.white, 0.5), alpha: E.outCubic(P(u, 6.0, 6.4)) });
  ctx.restore();
}

// ======================================================================
// UNREAL ON 2.0: v2 protocol, event tables end to end; C++ modules on the 2.0 module format
// ======================================================================
const UR = { eat: 2.6, pop: 2.9 };

function sUnreal(ctx, t) {
  const u = t - T.unreal, len = T.cli - T.unreal;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'UNREAL SDK · C++ MODULES', [{ s: 'Event tables,' }, { s: 'now in Unreal.', fill: 'brand' }]);
  const Cd = { x: 140, y: 270, w: 900, h: 300 };
  ctx.save(); enter(ctx, u, 0.25, 40);
  panel(ctx, Cd.x, Cd.y, Cd.w, Cd.h, 'server-cpp/src/lib.cpp');
  code(ctx, [['SPACETIMEDB_TABLE', SYN.fn], ['(', null], ['ConsumeEntityEvent', SYN.type], [',', null]], Cd.x + 32, Cd.y + 118, 20);
  code(ctx, [['                  consume_entity_event, Public, ', null], ['true', SYN.kw], [');', null]], Cd.x + 32, Cd.y + 154, 20);
  code(ctx, [['// 4th argument: an event table', SYN.com]], Cd.x + 32, Cd.y + 206, 20);
  const hk = E.outCubic(P(u, 1.0, 1.4));
  if (hk > 0) { const x = Cd.x + 32 + measure(ctx, '                  consume_entity_event, Public, ', 500, 20, MONO) - 4; rr(ctx, x, Cd.y + 130, (measure(ctx, 'true', 500, 20, MONO) + 8) * hk, 34, 6); ctx.strokeStyle = hexA(C.green, 0.8); ctx.lineWidth = 1.5; ctx.stroke(); }
  ctx.restore();
  // protocol upgrade
  const Pr = { x: 140, y: 600, w: 900, h: 260 };
  ctx.save(); enter(ctx, u, 0.45, 40);
  glass(ctx, Pr.x, Pr.y, Pr.w, Pr.h, 20);
  text(ctx, 'UNREAL SDK PROTOCOL', Pr.x + 32, Pr.y + 56, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  const sw = E.inOutCubic(P(u, 1.6, 2.0));
  text(ctx, 'v1.bsatn.spacetimedb', Pr.x + 32, Pr.y + 116, { size: 22, fill: hexA(C.white, 0.5 * (1 - sw) + 0.2) });
  if (sw > 0) { ctx.fillStyle = hexA(C.white, 0.5); ctx.fillRect(Pr.x + 32, Pr.y + 108, measure(ctx, 'v1.bsatn.spacetimedb', 500, 22, MONO) * sw, 2); }
  text(ctx, '→', Pr.x + 330, Pr.y + 116, { size: 22, fill: hexA(C.white, 0.5) });
  text(ctx, 'v2.bsatn.spacetimedb', Pr.x + 380, Pr.y + 116, { size: 22, weight: 700, fill: C.green, alpha: sw });
  ['event tables', 'multi-module codegen'].forEach((s, i) => {
    const a = E.outExpo(clamp((u - 2.2 - i * 0.15) / 0.35));
    if (a <= 0) return;
    ctx.save(); ctx.globalAlpha *= a;
    check(ctx, Pr.x + 36, Pr.y + 170 + i * 44, 14, C.green, 1);
    text(ctx, s, Pr.x + 64, Pr.y + 176 + i * 44, { size: 20, fam: SANS, weight: 600 });
    ctx.restore();
  });
  ctx.restore();
  // game: one blob eats another; the consume event pops on the client
  const G = { x: 1080, y: 270, w: 700, h: 590 };
  ctx.save(); enter(ctx, u, 0.6, 40);
  glass(ctx, G.x, G.y, G.w, G.h, 20);
  text(ctx, 'Blackholio · Unreal client', G.x + 32, G.y + 52, { size: 20, weight: 700, fam: SANS });
  const ax = G.x + 260 + E.inOutCubic(P(u, 1.2, UR.eat)) * 150, ay = G.y + 320;
  const eaten = u > UR.eat;
  const big = 70 + (eaten ? 10 * E.outBack(clamp((u - UR.eat) / 0.4)) : 0);
  if (!eaten) { ctx.fillStyle = hexA(C.white, 0.2); ctx.beginPath(); ctx.arc(G.x + 470, ay + 10, 26, 0, TAU); ctx.fill(); ctx.strokeStyle = hexA(C.white, 0.7); ctx.lineWidth = 2; ctx.stroke(); }
  ctx.fillStyle = hexA(C.green, 0.3); ctx.beginPath(); ctx.arc(ax, ay, big, 0, TAU); ctx.fill(); ctx.strokeStyle = hexA(C.green, 0.9); ctx.lineWidth = 2.5; ctx.stroke();
  const pt = u - UR.pop;
  if (pt > 0 && pt < 2.2) text(ctx, 'ConsumeEntityEvent', ax, ay - big - 20 - pt * 30, { size: 20, weight: 700, align: 'center', fill: C.green, alpha: 1 - P(pt, 1.4, 2.2) });
  if (pt > 0) {
    ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(G.x, G.y + G.h - 110, G.w, 1);
    text(ctx, 'OnInsert', G.x + 32, G.y + G.h - 56, { size: 19, weight: 700, fill: C.green, alpha: clamp(pt / 0.3) });
    text(ctx, 'ConsumeEntityEvent { consumed, consumer }', G.x + 140, G.y + G.h - 56, { size: 19, fill: hexA(C.white, 0.75), alpha: clamp(pt / 0.3) });
  }
  ctx.restore();
  caption(ctx, 'Unreal clients now speak the 2.0 protocol, so event tables work end to end.', u, 4.6, 950);
  ctx.restore();
}

// ======================================================================
// CARDS, then the outro
// ======================================================================
const CLI = [
  { k: 'SPACETIME INIT · DEV', title: ['Find a template', 'by typing'], d: ['Every template, grouped by', 'language: type to filter.'] },
  { k: 'SPACETIME LOGS', title: ['Only the', 'errors, please'], d: ['--level warn shows warnings', 'and up; --level-exact just one.'] },
  { k: 'UPDATES', title: ['Never miss', 'a release'], d: ['Once a day the CLI tells you', 'when a new version is out.'] },
  { k: 'LOGIN', title: ['Switch accounts', 'in one step'], d: ['spacetime login while logged in', 'now replaces the old session.'] },
];
const MORE = [
  { k: 'AI AGENTS', title: ['SpacetimeDB', 'skills'], d: ['npx skills add', 'clockworklabs/SpacetimeDB'] },
  { k: 'MODULES', title: ['Switch languages,', 'keep your data'], d: ['Republish Rust as TypeScript', '(or back); rows stay.'] },
  { k: 'QUERY BUILDER', title: ['Bare booleans', 'in where()'], d: ['where(u => u.online)', 'instead of .eq(true).'] },
];

const intro = R.makeIntro({ pre: 'v2.', from: '0', to: '1', sub: 'What’s new since 2.0', T });
const cli = R.makeCards({ start: T.cli, end: T.more, kick: 'ALSO IN 2.1', parts: [{ s: 'A smoother' }, { s: 'CLI.', fill: 'brand' }], list: CLI });
const more = R.makeCards({ start: T.more, end: T.outro, kick: 'MORE FEATURES', parts: [{ s: 'Also in' }, { s: '2.1.', fill: 'brand' }], list: MORE, toOutro: true });
const outro = R.makeOutro({ version: '2.1', T });
const { frame, setSlow } = R.makeReel({ DUR, T, SCENES, label: '/  RELEASE v2.1', draw: [intro, sRust, sHttp, sUnreal, cli, more, outro], seed: 21 });

module.exports = { frame, setSlow, DUR, T, SCENES, RB, HT, UR };
