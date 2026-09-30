// SpacetimeDB v2.0 release video (v1.12.0 → v2.0.1, the published "SpacetimeDB 2.0"), 62.5 s @ 120 BPM.
// See DECISIONS.md for what's shown and why.
const L = require('../lib');
const { C, SANS, MONO, clamp, lerp, P, E, pulse, hexA, rr, glass, measure, text, A } = L;
const U = require('../ui');
const { CX, TAU, check, spinner } = U;
const R = require('../release');
const { slide, panel, enter, status, caption, pill, code, SYN, header } = R;

const DUR = 62.5;
const T = { intro: 0, events: 5.5, await: 15, config: 24, tmpl: 33, secure: 41.5, plus: 49, outro: 56.5, end: DUR };
const SCENES = [
  { t: T.intro, name: 'v2.0' },
  { t: T.events, name: 'EVENT TABLES' },
  { t: T.await, name: 'REDUCER CALLS' },
  { t: T.config, name: 'SPACETIME.JSON' },
  { t: T.tmpl, name: 'TEMPLATES' },
  { t: T.secure, name: 'PRIVATE BY DEFAULT' },
  { t: T.plus, name: 'ALSO IN 2.0' },
  { t: T.outro, name: 'SPACETIMEDB' },
];

// ======================================================================
// EVENT TABLES: rows that are delivered as events and don't pile up
// ======================================================================
const EV = { fire: [2.4, 3.8, 5.2], dmg: [42, 17, 8] };
const EVC = [
  [['const ', SYN.kw], ['damageEvent = ', null], ['table', SYN.fn], ['(', null]],
  [['  { public: ', null], ['true', SYN.kw], [', event: ', null], ['true', SYN.kw], [' },', null]],
  [['  { target: t.', null], ['u64', SYN.fn], ['(), damage: t.', null], ['u32', SYN.fn], ['() }', null]],
  [[');', null]],
  [['', null]],
  [['// in a reducer', SYN.com]],
  [['ctx.db.damageEvent.', null], ['insert', SYN.fn], ['({ target, damage });', null]],
];

function sEvents(ctx, t) {
  const u = t - T.events, len = T.await - T.events;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'NEW · EVENT TABLES', [{ s: 'Broadcast events' }, { s: 'with event tables.', fill: 'brand' }]);
  const Cd = { x: 140, y: 270, w: 780, h: 380 };
  ctx.save();
  enter(ctx, u, 0.25, 40);
  panel(ctx, Cd.x, Cd.y, Cd.w, Cd.h, 'src/index.ts');
  EVC.forEach((ln, i) => code(ctx, ln, Cd.x + 36, Cd.y + 114 + i * 36, 20));
  const hi = EV.fire.reduce((m, f) => Math.max(m, pulse(u - f, 3) * (u > f ? 1 : 0)), 0);
  if (hi > 0.01) { rr(ctx, Cd.x + 26, Cd.y + 114 + 6 * 36 - 26, Cd.w - 52, 36, 6); ctx.strokeStyle = hexA(C.green, 0.3 + 0.6 * hi); ctx.lineWidth = 1.5; ctx.stroke(); }
  ctx.restore();
  // the table stays empty
  const St = { x: 140, y: 680, w: 780, h: 180 };
  ctx.save();
  enter(ctx, u, 0.5, 40);
  glass(ctx, St.x, St.y, St.w, St.h, 20);
  text(ctx, 'damage_event', St.x + 32, St.y + 56, { size: 22, weight: 700 });
  const sent = EV.fire.filter(f => u > f).length;
  text(ctx, 'events delivered', St.x + 32, St.y + 116, { size: 17, fill: hexA(C.white, 0.5) });
  text(ctx, String(sent), St.x + 32, St.y + 156, { size: 34, weight: 800, fam: SANS, fill: C.green });
  text(ctx, 'rows left in the table', St.x + 400, St.y + 116, { size: 17, fill: hexA(C.white, 0.5) });
  text(ctx, '0', St.x + 400, St.y + 156, { size: 34, weight: 800, fam: SANS });
  ctx.restore();
  // three subscribed clients: each insert pops as a damage number
  for (let c = 0; c < 3; c++) {
    const W0 = { x: 960, y: 270 + c * 200, w: 820, h: 180 };
    ctx.save();
    enter(ctx, u, 0.4 + c * 0.1, 40);
    glass(ctx, W0.x, W0.y, W0.w, W0.h, 16);
    text(ctx, `player ${c + 1}`, W0.x + 28, W0.y + 42, { size: 16, weight: 600, fill: hexA(C.white, 0.5) });
    const ex = W0.x + 420, ey = W0.y + 105;
    ctx.fillStyle = hexA(C.white, 0.15); ctx.beginPath(); ctx.arc(ex, ey, 34, 0, TAU); ctx.fill();
    ctx.strokeStyle = hexA(C.white, 0.7); ctx.lineWidth = 2; ctx.stroke();
    text(ctx, 'boss', ex, ey + 6, { size: 16, weight: 700, fam: SANS, align: 'center' });
    EV.fire.forEach((f, i) => {
      const lt = u - f - c * 0.05;
      if (lt < 0 || lt > 1.2) return;
      const a = 1 - P(lt, 0.7, 1.2);
      text(ctx, `-${EV.dmg[i]}`, ex + 60 + i * 30, ey - 20 - lt * 50, { size: 30, weight: 900, fam: SANS, fill: C.green, alpha: a });
    });
    text(ctx, 'onInsert', W0.x + W0.w - 28, W0.y + 42, { size: 15, align: 'right', fill: hexA(C.white, 0.4) });
    ctx.restore();
  }
  caption(ctx, 'Insert into an event table and every subscriber gets it as an event. Nothing piles up.', u, 6.0, 950);
  ctx.restore();
}

// ======================================================================
// REDUCER CALLS: await the result; others don't see your arguments
// ======================================================================
const AW = { ok: 1.4, okDone: 2.0, err: 2.8, errDone: 3.4, peer: 4.4 };

function sAwait(ctx, t) {
  const u = t - T.await, len = T.config - T.await;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'TYPESCRIPT SDK · NEW PROTOCOL', [{ s: 'Await your' }, { s: 'reducer calls.', fill: 'brand' }]);
  const Cd = { x: 140, y: 270, w: 900, h: 590 };
  ctx.save();
  enter(ctx, u, 0.25, 40);
  panel(ctx, Cd.x, Cd.y, Cd.w, Cd.h, 'src/App.tsx');
  const lines = [
    [['try', SYN.kw], [' {', null]],
    [['  await ', SYN.kw], ['conn.reducers.', null], ['setName', SYN.fn], ['({ name });', null]],
    [['  ', null], ['showToast', SYN.fn], ['(', null], ["'Saved'", SYN.str], [');', null]],
    [['} ', null], ['catch', SYN.kw], [' (e) {', null]],
    [['  ', null], ['showError', SYN.fn], ['(e.message);', null]],
    [['}', null]],
  ];
  lines.forEach((ln, i) => code(ctx, ln, Cd.x + 36, Cd.y + 116 + i * 40, 21));
  ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(Cd.x, Cd.y + 380, Cd.w, 1);
  // two calls: one resolves, one rejects with the module's error
  if (u > AW.ok) {
    text(ctx, 'setName({ name: "Alice W." })', Cd.x + 36, Cd.y + 430, { size: 19, fill: hexA(C.white, 0.75) });
    status(ctx, Cd.x + 440, Cd.y + 424, u - AW.ok, t, AW.okDone - AW.ok);
    if (u > AW.okDone) text(ctx, 'resolved', Cd.x + 470, Cd.y + 430, { size: 19, fill: C.green, alpha: E.outCubic(P(u, AW.okDone, AW.okDone + 0.3)) });
  }
  if (u > AW.err) {
    text(ctx, 'setName({ name: "" })', Cd.x + 36, Cd.y + 490, { size: 19, fill: hexA(C.white, 0.75) });
    if (u < AW.errDone) spinner(ctx, Cd.x + 448, Cd.y + 483, 8, t, C.white);
    else {
      const a = E.outCubic(P(u, AW.errDone, AW.errDone + 0.3));
      ctx.save(); ctx.globalAlpha *= a; ctx.strokeStyle = C.red; ctx.lineWidth = 2.5; ctx.beginPath();
      ctx.moveTo(Cd.x + 441, Cd.y + 476); ctx.lineTo(Cd.x + 455, Cd.y + 490); ctx.moveTo(Cd.x + 455, Cd.y + 476); ctx.lineTo(Cd.x + 441, Cd.y + 490); ctx.stroke(); ctx.restore();
      text(ctx, 'SenderError: Name must not be empty', Cd.x + 470, Cd.y + 490, { size: 19, fill: C.red, alpha: a });
    }
  }
  ctx.restore();
  // another user's screen
  const Pn = { x: 1080, y: 270, w: 700, h: 590 };
  ctx.save();
  enter(ctx, u, 0.45, 40);
  glass(ctx, Pn.x, Pn.y, Pn.w, Pn.h, 20);
  text(ctx, 'WHAT BOB SEES', Pn.x + 32, Pn.y + 56, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  const pa = E.outCubic(P(u, AW.peer, AW.peer + 0.4));
  [['1.x', false, 130], ['2.0', true, 410]].forEach(([v, now, oy]) => {
    const y = Pn.y + oy;
    text(ctx, v, Pn.x + 32, y, { size: 15, weight: 700, tracking: 4, fill: now ? C.green : hexA(C.white, 0.5) });
    rr(ctx, Pn.x + 32, y + 20, Pn.w - 64, 56, 10); ctx.fillStyle = hexA(C.white, 0.04); ctx.fill();
    text(ctx, 'alice', Pn.x + 56, y + 56, { size: 19, fill: hexA(C.white, 0.6) });
    text(ctx, u > AW.okDone ? 'Alice W.' : 'alice', Pn.x + 250, y + 56, { size: 19, weight: 600 });
    if (!now && pa > 0) {
      ctx.save(); ctx.globalAlpha *= pa;
      rr(ctx, Pn.x + 32, y + 90, Pn.w - 64, 60, 14); ctx.fillStyle = hexA(C.white, 0.08); ctx.fill();
      text(ctx, 'alice called set_name("Alice W.")', Pn.x + 56, y + 128, { size: 18, fill: hexA(C.white, 0.8) });
      ctx.restore();
      text(ctx, 'Every reducer call was broadcast.', Pn.x + 32, y + 184, { size: 18, fam: SANS, fill: hexA(C.white, 0.5), alpha: pa });
    }
    if (now && pa > 0) text(ctx, 'Just the row change. Your arguments stay yours.', Pn.x + 32, y + 116, { size: 18, fam: SANS, weight: 600, fill: C.green, alpha: pa });
  });
  ctx.restore();
  caption(ctx, 'Reducer calls return a promise, and other users no longer see the arguments you sent.', u, 5.4, 950);
  ctx.restore();
}

// ======================================================================
// SPACETIME.JSON: config once; generate/publish without flags; dev starts your client
// ======================================================================
const CF = { strike: 1.0, short: 1.6, json: 2.2, dev: [3.0, 3.4], l1: 3.8, l2: 4.2, l3: 4.6, app: 5.0 };
const LONG = ['spacetime generate --lang typescript --out-dir src/module_bindings', 'spacetime publish --server maincloud --module-path spacetimedb my-database'];
const SHORT = ['spacetime generate', 'spacetime publish'];

function sConfig(ctx, t) {
  const u = t - T.config, len = T.tmpl - T.config;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'CLI · SPACETIME.JSON', [{ s: 'Configure your project' }, { s: 'once, in spacetime.json.', fill: 'brand' }]);
  // long commands collapse into short ones
  const Tm = { x: 140, y: 270, w: 1640, h: 180 };
  ctx.save();
  enter(ctx, u, 0.25, 40);
  panel(ctx, Tm.x, Tm.y, Tm.w, Tm.h, 'before → after');
  const sh = E.inOutCubic(P(u, CF.short - 0.3, CF.short));
  LONG.forEach((s, i) => {
    const y = Tm.y + 110 + i * 40;
    text(ctx, '$', Tm.x + 36, y, { size: 20, weight: 600, fill: hexA(C.white, 0.5) });
    const full = measure(ctx, s, 500, 20, MONO), short = measure(ctx, SHORT[i], 500, 20, MONO);
    ctx.save(); ctx.beginPath(); ctx.rect(Tm.x + 62, y - 26, lerp(full, short, sh) + 4, 40); ctx.clip();
    text(ctx, sh < 0.5 ? s : SHORT[i], Tm.x + 62, y, { size: 20, fill: sh < 0.5 ? hexA(C.white, 0.85) : C.white });
    const st = P(u, CF.strike, CF.strike + 0.3) * (1 - sh);
    if (st > 0 && sh < 0.5) { ctx.fillStyle = hexA(C.white, 0.5); ctx.fillRect(Tm.x + 62 + short, y - 7, (full - short) * st, 2); }
    ctx.restore();
  });
  ctx.restore();
  // the config file init writes
  const J = { x: 140, y: 480, w: 620, h: 380 };
  ctx.save();
  enter(ctx, u, CF.json - 0.3, 40);
  panel(ctx, J.x, J.y, J.w, J.h, 'spacetime.json');
  ['{', '  "server": "maincloud",', '  "module-path": "./spacetimedb"', '}'].forEach((s, i) => text(ctx, s, J.x + 36, J.y + 120 + i * 40, { size: 21, fill: hexA(C.white, 0.85) }));
  text(ctx, 'Written by spacetime init.', J.x + 36, J.y + 320, { size: 18, fam: SANS, fill: hexA(C.white, 0.55) });
  ctx.restore();
  // spacetime dev: module + client together
  const D = { x: 800, y: 480, w: 980, h: 380 };
  ctx.save();
  enter(ctx, u, CF.dev[0] - 0.3, 40);
  panel(ctx, D.x, D.y, D.w, D.h, 'terminal');
  const dc = 'spacetime dev';
  text(ctx, '$', D.x + 36, D.y + 114, { size: 20, weight: 600, fill: hexA(C.white, 0.5) });
  text(ctx, dc.slice(0, Math.floor(P(u, CF.dev[0], CF.dev[1]) * dc.length)), D.x + 62, D.y + 114, { size: 20 });
  [[CF.l1, 'module built and published'], [CF.l2, 'client bindings generated'], [CF.l3, 'Starting client: npm run dev']].forEach(([at, s], i) => {
    const a = E.outCubic(P(u, at, at + 0.25));
    if (a <= 0) return;
    check(ctx, D.x + 40, D.y + 150 + i * 40, 14, C.green, a);
    text(ctx, s, D.x + 68, D.y + 156 + i * 40, { size: 19, fill: i === 2 ? C.white : hexA(C.white, 0.75), alpha: a });
  });
  // the app pops up
  const aa = E.outBack(clamp((u - CF.app) / 0.4));
  if (aa > 0) {
    ctx.save(); ctx.translate(D.x + D.w - 220, D.y + 250); ctx.scale(aa, aa);
    rr(ctx, -160, -90, 320, 180, 12); ctx.fillStyle = '#16222a'; ctx.fill(); ctx.strokeStyle = hexA(C.green, 0.6); ctx.lineWidth = 1.5; ctx.stroke();
    text(ctx, 'localhost:5173', 0, -56, { size: 14, align: 'center', fill: hexA(C.white, 0.5) });
    text(ctx, 'your app, live', 0, 6, { size: 22, weight: 700, fam: SANS, align: 'center' });
    ctx.restore();
  }
  ctx.restore();
  caption(ctx, 'spacetime.json remembers your setup, and spacetime dev now starts your client too.', u, 5.6, 950);
  ctx.restore();
}

// ======================================================================
// TEMPLATES: ten new starters, plus Angular and TanStack bindings
// ======================================================================
const NEW = [['nextjs', 'Next.js'], ['nuxt', 'Nuxt'], ['angular', 'Angular'], ['tanstack', 'TanStack Start'], ['remix', 'Remix'], ['html5', 'Browser script'], ['bun', 'Bun'], ['deno', 'Deno'], ['nodejs', 'Node.js'], ['cpp', 'C++ module']];
const OLD = ['react', 'vue', 'svelte'];

function sTmpl(ctx, t) {
  const u = t - T.tmpl, len = T.secure - T.tmpl;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'NEW · STARTER TEMPLATES', [{ s: 'Ten new' }, { s: 'starter templates.', fill: 'brand' }]);
  const cw = 300, ch = 170, gap = 28, x0 = CX - (5 * cw + 4 * gap) / 2, y0 = 280;
  NEW.forEach(([logo, name], i) => {
    const a = E.outExpo(clamp((u - 0.5 - i * 0.12) / 0.4));
    if (a <= 0) return;
    const x = x0 + (i % 5) * (cw + gap), y = y0 + Math.floor(i / 5) * (ch + gap);
    ctx.save(); ctx.globalAlpha *= a; ctx.translate(0, (1 - a) * 30);
    glass(ctx, x, y, cw, ch, 16);
    ctx.drawImage(A.logos[logo], x + cw / 2 - 36, y + 26, 72, 72);
    text(ctx, name, x + cw / 2, y + 136, { size: 21, weight: 700, fam: SANS, align: 'center' });
    ctx.restore();
  });
  // already supported
  const oa = E.outCubic(P(u, 2.2, 2.6));
  if (oa > 0) {
    ctx.save(); ctx.globalAlpha *= oa;
    const y = y0 + 2 * (ch + gap) + 40;
    text(ctx, 'ALREADY SUPPORTED', x0, y + 8, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.45) });
    OLD.forEach((n, i) => { ctx.save(); ctx.globalAlpha *= 0.5; ctx.drawImage(A.logos[n], x0 + 250 + i * 70, y - 22, 44, 44); ctx.restore(); });
    text(ctx, 'plus spacetimedb/angular and spacetimedb/tanstack', x0 + 520, y + 8, { size: 19, fam: SANS, weight: 500, fill: hexA(C.white, 0.7) });
    ctx.restore();
  }
  const ta = E.outExpo(clamp((u - 3.0) / 0.4));
  if (ta > 0) {
    const s = 'spacetime dev --template nextjs-ts';
    const w = measure(ctx, s, 500, 22, MONO) + 100;
    ctx.save(); ctx.globalAlpha *= ta;
    rr(ctx, CX - w / 2, 820, w, 64, 32); ctx.fillStyle = '#121A1F'; ctx.fill(); ctx.strokeStyle = hexA(C.green, 0.6); ctx.lineWidth = 1.5; ctx.stroke();
    text(ctx, '$', CX - w / 2 + 32, 860, { size: 22, weight: 600, fill: hexA(C.white, 0.5) });
    text(ctx, s, CX - w / 2 + 60, 860, { size: 22 });
    ctx.restore();
  }
  caption(ctx, 'Ten new starter templates: a working real-time app in your stack, in one command.', u, 4.2, 960);
  ctx.restore();
}

// ======================================================================
// CARDS, then the outro
// ======================================================================
const SECURE = [
  { k: 'SCHEDULED FUNCTIONS', title: ['Scheduled jobs', 'are private'], d: ['Clients can’t call them', 'directly; the owner still can.'] },
  { k: 'CODEGEN', title: ['Private tables stay', 'out of client code'], d: ['Opt back in with', 'spacetime generate --include-private.'] },
  { k: 'PROCEDURES', title: ['No calls into', 'private networks'], d: ['HTTP to private IP ranges', 'is refused, even after DNS.'] },
];
const PLUS = [
  { k: 'TYPESCRIPT', title: ['TypeScript modules,', 'out of beta'], d: ['No more beta warning', 'when you publish.'] },
  { k: 'TYPESCRIPT MODULES', title: ['Just', 'export it'], d: ['Reducers are named exports,', 'and console.log pretty-prints.'] },
  { k: 'QUERIES', title: ['Typed queries,', 'no .build()'], d: ['tables.user.where(…) in', 'TypeScript, Rust and C#.'] },
  { k: 'SUBSCRIPTIONS', title: ['Confirmed reads', 'by default'], d: ['Updates arrive once they’re', 'durable, on v2 connections.'] },
];

const intro = R.makeIntro({ pre: 'v', from: '1.12', to: '2.0', sub: 'What’s new since 1.12', T });
const secure = R.makeCards({ start: T.secure, end: T.plus, kick: 'SECURITY', parts: [{ s: 'Secure' }, { s: 'by default.', fill: 'brand' }], list: SECURE });
const plus = R.makeCards({ start: T.plus, end: T.outro, kick: 'MORE FEATURES', parts: [{ s: 'Also in' }, { s: '2.0.', fill: 'brand' }], list: PLUS, toOutro: true });
const outro = R.makeOutro({ version: '2.0', T });
const { frame, setSlow } = R.makeReel({ DUR, T, SCENES, label: '/  RELEASE v2.0', draw: [intro, sEvents, sAwait, sConfig, sTmpl, secure, plus, outro], seed: 20 });

module.exports = { frame, setSlow, DUR, T, SCENES, EV, AW, CF };
