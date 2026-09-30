// SpacetimeDB v2.5 release video (v2.4.0 → v2.5.0), 46 s @ 120 BPM. See DECISIONS.md for what's shown and why.
const L = require('../lib');
const { W, C, SANS, MONO, clamp, lerp, P, E, pulse, hexA, rr, glass, measure, text, A } = L;
const U = require('../ui');
const { CX, TAU, check, spinner } = U;
const R = require('../release');
const { laneFrame, beforeAfter } = R;
const { resultLine } = R;
const { typedCommand } = R;
const { slide, panel, enter, status, caption, pill, code, SYN, header } = R;

const DUR = 46;
const T = { intro: 0, proc: 5.5, solid: 14.5, views: 23.5, plus: 32.5, outro: 40, end: DUR };
const SCENES = [
  { t: T.intro, name: 'v2.5' },
  { t: T.proc, name: 'PROCEDURES' },
  { t: T.solid, name: 'SOLIDJS' },
  { t: T.views, name: 'VIEWS' },
  { t: T.plus, name: 'ALSO IN 2.5' },
  { t: T.outro, name: 'SPACETIMEDB' },
];

// ======================================================================
// PROCEDURES: out of beta, no feature flag
// ======================================================================
const PR = { strike: 1.6, gone: 2.2, build: 2.6, built: 3.3, badge: 1.1 };
const RUST = [
  [['#[spacetimedb::procedure]', SYN.attr]],
  [['fn ', SYN.kw], ['fetch_weather', SYN.fn], ['(ctx: &', null], ['mut ', SYN.kw], ['ProcedureContext', SYN.type], [') {', null]],
  [['    let ', SYN.kw], ['res = ctx.http.', null], ['get', SYN.fn], ['(', null], ['"https://api.example.com"', SYN.str], [');', null]],
  [['    ctx.', null], ['with_tx', SYN.fn], ['(|ctx| { ', null], ['/* save it */', SYN.com], [' });', null]],
  [['}', null]],
];
const FLAGS = [['Rust', 'no "unstable" feature'], ['C#', 'no STDB_UNSTABLE pragma'], ['C++', 'no UNSTABLE_FEATURES define']];

function sProc(ctx, t) {
  const u = t - T.proc, len = T.solid - T.proc;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'PROCEDURES · RUST, C# AND C++', [{ s: 'Procedures, out of' }, { s: 'beta.', fill: 'brand' }]);
  // a BETA badge after the headline peels off and falls away
  const bx = 140 + measure(ctx, 'Procedures, out of beta.', 900, 84, SANS) + 40;
  const fall = E.inCubic(P(u, PR.badge, PR.badge + 0.8));
  if (u > 0.6 && fall < 1) {
    ctx.save();
    ctx.globalAlpha *= E.outCubic(P(u, 0.6, 0.9)) * (1 - fall);
    ctx.translate(bx + 60, 175 + fall * 260); ctx.rotate(fall * 0.8);
    pill(ctx, -60, 0, 'BETA', { size: 20, weight: 700, h: 48, pad: 56, stroke: hexA(C.white, 0.5) });
    ctx.restore();
  }

  // Cargo.toml: the unstable feature is struck and removed
  const F = { x: 140, y: 270, w: 800, h: 250 };
  ctx.save();
  enter(ctx, u, 0.3, 40);
  panel(ctx, F.x, F.y, F.w, F.h, 'Cargo.toml');
  text(ctx, '[dependencies]', F.x + 36, F.y + 120, { size: 20, fill: hexA(C.white, 0.6) });
  const a = 'spacetimedb = { version = "2.*"', b = ', features = ["unstable"]', c = ' }';
  const wa = measure(ctx, a, 500, 20, MONO), wb = measure(ctx, b, 500, 20, MONO);
  const gone = E.inOutCubic(P(u, PR.gone, PR.gone + 0.4));
  const y = F.y + 164;
  text(ctx, a, F.x + 36, y, { size: 20 });
  ctx.save();
  ctx.beginPath(); ctx.rect(F.x + 36 + wa, y - 30, wb * (1 - gone), 44); ctx.clip();
  text(ctx, b, F.x + 36 + wa, y, { size: 20, fill: C.red, alpha: 1 - gone });
  const st = P(u, PR.strike, PR.strike + 0.3);
  if (st > 0) { ctx.fillStyle = hexA(C.red, 0.9); ctx.fillRect(F.x + 36 + wa, y - 7, wb * st, 2.5); }
  ctx.restore();
  text(ctx, c, F.x + 36 + wa + wb * (1 - gone), y, { size: 20 });
  ctx.restore();

  // the procedure itself: calls an API, then writes in its own transaction
  const S = { x: 980, y: 270, w: 800, h: 480 };
  ctx.save();
  enter(ctx, u, 0.45, 40);
  panel(ctx, S.x, S.y, S.w, S.h, 'src/lib.rs');
  RUST.forEach((ln, i) => code(ctx, ln, S.x + 36, S.y + 118 + i * 40, 19));
  // build status
  const bt = u - PR.build;
  if (bt > 0) {
    ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(S.x, S.y + S.h - 100, S.w, 1);
    text(ctx, '$ cargo build', S.x + 36, S.y + S.h - 46, { size: 19, fill: hexA(C.white, 0.6) });
    status(ctx, S.x + 250, S.y + S.h - 52, u - PR.build, t, PR.built - PR.build);
    if (u > PR.built) text(ctx, 'compiles, no feature flag', S.x + 282, S.y + S.h - 46, { size: 19, fill: C.green, alpha: E.outCubic(P(u, PR.built, PR.built + 0.3)) });
  }
  ctx.restore();

  // what each language no longer needs
  const cy = 590;
  FLAGS.forEach(([lang, s], i) => {
    const lt = u - (3.6 + i * 0.15);
    if (lt < 0) return;
    const al = E.outExpo(clamp(lt / 0.35));
    ctx.save();
    ctx.globalAlpha *= al;
    ctx.translate(0, (1 - al) * 16);
    const yy = cy + i * 64;
    rr(ctx, 140, yy - 26, 800, 52, 12); ctx.fillStyle = hexA(C.white, 0.04); ctx.fill();
    check(ctx, 164, yy + 1, 14, C.green, E.outCubic(clamp((lt - 0.2) / 0.2)));
    text(ctx, lang, 196, yy + 8, { size: 22, weight: 700, fam: SANS });
    text(ctx, s, 270, yy + 7, { size: 19, fill: hexA(C.white, 0.75) });
    ctx.restore();
  });
  caption(ctx, 'Procedures can call other websites and run their own transactions, with no feature flag.', u, 4.4, 900);
  ctx.restore();
}

// ======================================================================
// SOLIDJS: a template, and two windows that stay in sync
// ======================================================================
const SO = { type: [0.5, 1.4], ran: 1.8, name: [3.0, 3.5], click: 3.9, sync: 4.1 };
const FRAMEWORKS = ['react', 'vue', 'svelte', 'angular'];

function browser(ctx, x, y, w, h, u, t, typing) {
  panel(ctx, x, y, w, h, 'localhost:5173');
  text(ctx, 'SpacetimeDB SolidJS App', x + 32, y + 118, { size: 26, weight: 800, fam: SANS });
  text(ctx, 'Status:', x + 32, y + 160, { size: 19, fam: SANS, fill: hexA(C.white, 0.7) });
  text(ctx, 'Connected', x + 104, y + 160, { size: 19, weight: 700, fam: SANS, fill: C.green });
  // input + button
  rr(ctx, x + 32, y + 186, w - 230, 46, 8); ctx.fillStyle = hexA(C.white, 0.05); ctx.fill();
  ctx.strokeStyle = hexA(C.white, 0.2); ctx.lineWidth = 1.5; ctx.stroke();
  let typed = '';
  if (typing) { const n = Math.floor(P(u, SO.name[0], SO.name[1]) * 3); typed = u < SO.click ? 'Ada'.slice(0, n) : ''; }
  text(ctx, typed || 'Enter name', x + 48, y + 216, { size: 18, fam: SANS, fill: typed ? C.white : hexA(C.white, 0.35) });
  const press = typing ? pulse(u - SO.click, 6) * (u > SO.click ? 1 : 0) : 0;
  rr(ctx, x + w - 184, y + 186, 152, 46, 8); ctx.fillStyle = hexA(C.white, 0.1 + 0.2 * press); ctx.fill();
  text(ctx, 'Add Person', x + w - 108, y + 216, { size: 18, weight: 600, fam: SANS, align: 'center' });
  // list
  const people = ['Grace', 'Linus'];
  const added = u > SO.sync;
  text(ctx, `People (${people.length + (added ? 1 : 0)})`, x + 32, y + 290, { size: 22, weight: 700, fam: SANS });
  people.concat(added ? ['Ada'] : []).forEach((p, i) => {
    const isNew = i === 2;
    const a = isNew ? E.outExpo(clamp((u - SO.sync) / 0.35)) : 1;
    const flash = isNew ? pulse(u - SO.sync, 2.5) : 0;
    ctx.save();
    ctx.globalAlpha *= a;
    ctx.translate((1 - a) * 20, 0);
    const ry = y + 314 + i * 50;
    rr(ctx, x + 32, ry, w - 64, 40, 8); ctx.fillStyle = hexA(C.white, 0.03 + 0.08 * flash); ctx.fill();
    if (isNew) { ctx.strokeStyle = hexA(C.green, 0.3 + 0.6 * flash); ctx.lineWidth = 1.5; ctx.stroke(); }
    text(ctx, '•  ' + p, x + 48, ry + 27, { size: 19, fam: SANS });
    ctx.restore();
  });
}

function sSolid(ctx, t) {
  const u = t - T.solid, len = T.views - T.solid;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'NEW · SOLIDJS SUPPORT', [{ s: 'SolidJS support, with' }, { s: 'a starter template.', fill: 'brand' }]);

  // terminal: one command starts the template
  const Tm = { x: 140, y: 270, w: 700, h: 190 };
  ctx.save();
  enter(ctx, u, 0.25, 40);
  panel(ctx, Tm.x, Tm.y, Tm.w, Tm.h, 'terminal');
  const cmd = 'spacetime dev --template solid-ts';
  typedCommand(ctx, cmd, Tm.x + 32, Tm.y + 112, u, SO.type[0], SO.type[1]);
  if (u > SO.ran) {
    const ra = E.outCubic(P(u, SO.ran, SO.ran + 0.3));
    resultLine(ctx, 'SolidJS app + TypeScript module, running', Tm.x + 34, Tm.y + 156, ra, { gap: 24 });
  }
  ctx.restore();

  // the hook that keeps the list live
  const Cd = { x: 140, y: 490, w: 700, h: 270 };
  ctx.save();
  enter(ctx, u, 0.5, 40);
  panel(ctx, Cd.x, Cd.y, Cd.w, Cd.h, 'src/App.tsx');
  code(ctx, [['import ', SYN.kw], ['{ useTable, useReducer } ', null], ['from ', SYN.kw], ["'spacetimedb/solid'", SYN.str], [';', null]], Cd.x + 32, Cd.y + 120, 18);
  code(ctx, [['const ', SYN.kw], ['[people] = ', null], ['useTable', SYN.fn], ['(() => tables.person);', null]], Cd.x + 32, Cd.y + 170, 18);
  code(ctx, [['const ', SYN.kw], ['addPerson = ', null], ['useReducer', SYN.fn], ['(reducers.add);', null]], Cd.x + 32, Cd.y + 210, 18);
  ctx.restore();

  // two windows of the running app: add in one, both update
  ctx.save(); enter(ctx, u, 1.9, 40); browser(ctx, 890, 270, 430, 490, u, t, true); ctx.restore();
  ctx.save(); enter(ctx, u, 2.05, 40); browser(ctx, 1350, 270, 430, 490, u, t, false); ctx.restore();
  // a data pulse from window to window through the database
  const sp = P(u, SO.click, SO.sync + 0.1);
  if (sp > 0 && sp < 1) { ctx.fillStyle = C.green; ctx.beginPath(); ctx.arc(lerp(1320, 1350, sp), 515, 5, 0, TAU); ctx.fill(); }

  // framework row: Solid joins the rest
  const fa = u - 4.8;
  if (fa > 0) {
    const y0 = 850;
    text(ctx, 'WEB FRAMEWORKS', 140, y0 + 8, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5), alpha: E.outCubic(clamp(fa / 0.3)) });
    FRAMEWORKS.forEach((n, i) => {
      const a = E.outExpo(clamp((fa - i * 0.08) / 0.35));
      ctx.save(); ctx.globalAlpha *= a * 0.6;
      ctx.drawImage(A.logos[n], 400 + i * 90, y0 - 30, 52, 52);
      ctx.restore();
    });
    const sa = E.outBack(clamp((fa - 0.5) / 0.4));
    if (sa > 0) {
      ctx.save(); ctx.translate(800, y0); ctx.scale(sa, sa);
      pill(ctx, 0, 0, '+ SolidJS', { size: 20, weight: 700, h: 52, pad: 48, stroke: hexA(C.green, 0.8), fam: SANS });
      ctx.restore();
    }
  }
  ctx.restore();
}

// ======================================================================
// VIEWS: a primary key turns delete + insert into one update
// ======================================================================
const VW = { change: 2.4 };
const BOARD = [['alice', 12], ['bob', 10], ['carol', 8]];

function board(ctx, x, y, w, h, u, now) {
  laneFrame(ctx, x, y, w, h, { now, version: now ? 'v2.5' : 'v2.4', title: now ? 'One update' : 'Delete, then insert', titleX: now ? 110 : 102, titleSize: 21 });
  text(ctx, 'leaderboard', x + 32, y + 104, { size: 18, weight: 700 });
  const lt = u - VW.change;
  BOARD.forEach(([n, s], i) => {
    const ry = y + 124 + i * 50;
    const isBob = n === 'bob';
    let a = 1, score = s, hi = 0;
    if (isBob && lt > 0) {
      if (now) { score = lt > 0.2 ? 14 : 10; hi = pulse(lt - 0.2, 2.5); }
      else { a = lt < 0.35 ? 1 - lt / 0.35 : lt < 0.75 ? 0 : clamp((lt - 0.75) / 0.35); score = lt < 0.35 ? 10 : 14; }
    }
    ctx.save();
    ctx.globalAlpha *= a;
    rr(ctx, x + 24, ry, w - 48, 42, 8); ctx.fillStyle = hexA(C.white, 0.03 + 0.07 * hi); ctx.fill();
    if (hi > 0.01) { ctx.strokeStyle = hexA(C.green, 0.3 + 0.6 * hi); ctx.lineWidth = 1.5; ctx.stroke(); }
    text(ctx, n, x + 48, ry + 28, { size: 19, fill: hexA(C.white, 0.85) });
    text(ctx, String(score), x + w - 48, ry + 28, { size: 19, weight: 700, align: 'right' });
    ctx.restore();
  });
  // client event log
  ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(x, y + 290, w, 1);
  text(ctx, 'YOUR APP RECEIVES', x + 32, y + 330, { size: 13, weight: 700, tracking: 3, fill: hexA(C.white, 0.45) });
  const evs = now ? [[0.25, 'onUpdate', 'bob  10 → 14']] : [[0.1, 'onDelete', 'bob  10'], [0.8, 'onInsert', 'bob  14']];
  evs.forEach(([at, cb, s], i) => {
    const ea = E.outExpo(clamp((lt - at) / 0.3));
    if (ea <= 0) return;
    ctx.save(); ctx.globalAlpha *= ea; ctx.translate((1 - ea) * 20, 0);
    const ey = y + 374 + i * 42;
    text(ctx, cb, x + 32, ey, { size: 19, weight: 700, fill: now ? C.green : hexA(C.white, 0.85) });
    text(ctx, s, x + 32 + measure(ctx, cb, 700, 19, MONO) + 18, ey, { size: 19, fill: hexA(C.white, 0.7) });
    ctx.restore();
  });
}

function sViews(ctx, t) {
  const u = t - T.views, len = T.plus - T.views;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'VIEWS · RUST, C# AND TYPESCRIPT', [{ s: 'Views written as code can' }, { s: 'have a primary key.', fill: 'brand' }]);
  // the one attribute that does it
  ctx.save();
  enter(ctx, u, 0.3, 30);
  const pre = '#[spacetimedb::view(accessor = leaderboard, public, ', key = 'primary_key = id', post = ')]';
  const w1 = measure(ctx, pre, 500, 22, MONO), w2 = measure(ctx, key, 500, 22, MONO), w3 = measure(ctx, post, 500, 22, MONO);
  const x0 = CX - (w1 + w2 + w3) / 2, y0 = 300;
  rr(ctx, x0 - 32, y0 - 40, w1 + w2 + w3 + 64, 64, 14); ctx.fillStyle = '#121A1F'; ctx.fill();
  ctx.strokeStyle = hexA(C.white, 0.12); ctx.lineWidth = 1.5; ctx.stroke();
  text(ctx, pre, x0, y0, { size: 22, fill: SYN.attr });
  text(ctx, key, x0 + w1, y0, { size: 22, weight: 700, fill: C.white });
  text(ctx, post, x0 + w1 + w2, y0, { size: 22, fill: SYN.attr });
  const hk = E.outCubic(P(u, 1.1, 1.5));
  if (hk > 0) { rr(ctx, x0 + w1 - 6, y0 - 28, (w2 + 12) * hk, 40, 6); ctx.strokeStyle = hexA(C.green, 0.8); ctx.stroke(); }
  ctx.restore();
  beforeAfter(ctx, u, (ctx, x, y, w, h, now) => board(ctx, x, y, w, h, u, now), { old: [140, 380, 800, 480], now: [980, 380, 800, 480], enterAt: [0.5, 0.65] });
  caption(ctx, 'Views written as code can name a primary key, so apps get one update instead of two events.', u, 4.2, 950);
  ctx.restore();
}

// ======================================================================
// PLUS cards, then the outro
// ======================================================================
const PLUS = [
  { k: 'CLI', title: ['Paste identities', 'as they are'], d: ['spacetime call takes 0x… or', 'c200… with no JSON wrapping.'] },
  { k: 'CLI', title: ['Clean republish', 'from your config'], d: ['publish -c=always reads the', 'database from spacetime.json.'] },
  { k: 'PERFORMANCE', title: ['Faster bulk', 'inserts'], d: ['Inserting many text-heavy rows', 'stays fast as tables grow.'] },
];

const intro = R.makeIntro({ pre: 'v2.', from: '4', to: '5', sub: 'What’s new since 2.4', T });
const plus = R.makeCards({ start: T.plus, end: T.outro, kick: 'CLI AND PERFORMANCE', parts: [{ s: 'Also in' }, { s: '2.5.', fill: 'brand' }], list: PLUS, toOutro: true });
const outro = R.makeOutro({ version: '2.5', T });
const { frame, setSlow } = R.makeReel({ DUR, T, SCENES, label: '/  RELEASE v2.5', draw: [intro, sProc, sSolid, sViews, plus, outro], seed: 5 });

module.exports = { frame, setSlow, DUR, T, SCENES, PR, SO, VW, PLUS };
