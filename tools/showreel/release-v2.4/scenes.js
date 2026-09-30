// SpacetimeDB v2.4 release video (v2.3.0 → v2.4.0), 36.5 s @ 120 BPM. See DECISIONS.md for what's shown and why.
const L = require('../lib');
const { C, SANS, MONO, clamp, lerp, P, E, pulse, hexA, rr, glass, measure, text, drawMark } = L;
const U = require('../ui');
const { CX, TAU, check, spinner } = U;
const R = require('../release');
const { commandChip, typedCommand, slide, panel, enter, status, caption, pill, code, SYN, header } = R;

const DUR = 36.5;
const T = { intro: 0, http: 5.5, tmpl: 15, plus: 24, outro: 30.5, end: DUR };
const SCENES = [
  { t: T.intro, name: 'v2.4' },
  { t: T.http, name: 'HTTP HANDLERS' },
  { t: T.tmpl, name: 'TEMPLATES' },
  { t: T.plus, name: 'ALSO IN 2.4' },
  { t: T.outro, name: 'SPACETIMEDB' },
];

// ======================================================================
// HTTP HANDLERS: a module serves its own routes (beta)
// ======================================================================
const HT = { code: [0.5, 1.9], curl: [2.3, 3.1], req: 3.3, res: 3.9 };
const TSC = [
  [['export const ', SYN.kw], ['say_hello = spacetimedb.', null], ['httpHandler', SYN.fn], ['((_ctx, _req) => {', null]],
  [['  return new ', SYN.kw], ['SyncResponse', SYN.type], ['(', null], ['"Hello!"', SYN.str], [');', null]],
  [['});', null]],
  [['', null]],
  [['export const ', SYN.kw], ['router = spacetimedb.', null], ['httpRouter', SYN.fn], ['(', null]],
  [['  new ', SYN.kw], ['Router', SYN.type], ['().', null], ['get', SYN.fn], ['(', null], ['"/say-hello"', SYN.str], [', say_hello)', null]],
  [[');', null]],
];
const CURL = 'curl localhost:3000/v1/database/my-game/route/say-hello';

function sHttp(ctx, t) {
  const u = t - T.http, len = T.tmpl - T.http;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'NEW · HTTP HANDLERS · BETA', [{ s: 'Modules can serve' }, { s: 'their own HTTP routes.', fill: 'brand' }]);

  const Cd = { x: 140, y: 270, w: 1000, h: 400 };
  ctx.save();
  enter(ctx, u, 0.25, 40);
  panel(ctx, Cd.x, Cd.y, Cd.w, Cd.h, 'my-game/src/index.ts');
  const shown = P(u, HT.code[0], HT.code[1]) * TSC.length;
  TSC.forEach((ln, i) => { const a = clamp(shown - i); if (a > 0) code(ctx, ln, Cd.x + 36, Cd.y + 116 + i * 38, 21, a); });
  ctx.restore();

  const Tm = { x: 140, y: 700, w: 1000, h: 190 };
  ctx.save();
  enter(ctx, u, 0.5, 40);
  panel(ctx, Tm.x, Tm.y, Tm.w, Tm.h, 'terminal');
  if (u > HT.curl[0] - 0.05) {
    typedCommand(ctx, CURL, Tm.x + 32, Tm.y + 110, u, HT.curl[0], HT.curl[1]);
  }
  if (u > HT.res) text(ctx, 'Hello!', Tm.x + 58, Tm.y + 150, { size: 20, weight: 700, fill: C.green, alpha: E.outCubic(P(u, HT.res, HT.res + 0.25)) });
  ctx.restore();

  // the route map: who can call in
  const M = { x: 1180, y: 270, w: 600, h: 620 };
  ctx.save();
  enter(ctx, u, 0.45, 40);
  glass(ctx, M.x, M.y, M.w, M.h, 20);
  text(ctx, 'YOUR MODULE’S ROUTES', M.x + 32, M.y + 56, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  const cx = M.x + M.w / 2, cy = M.y + 380;
  drawMark(ctx, cx, cy, 120, { color: C.white });
  text(ctx, 'my-game', cx, cy + 100, { size: 24, weight: 800, fam: SANS, align: 'center' });
  // route pill
  const ra = E.outBack(clamp((u - HT.code[1]) / 0.35));
  if (ra > 0) { ctx.save(); ctx.translate(cx, M.y + 130); ctx.scale(ra, ra); const w = measure(ctx, 'GET /say-hello', 600, 19, MONO) + 44; pill(ctx, -w / 2, 0, 'GET /say-hello', { size: 19, h: 46, stroke: hexA(C.green, 0.7) }); ctx.restore(); }
  // callers
  const callers = [['curl', M.x + 90, M.y + 250], ['webhooks', M.x + M.w - 120, M.y + 250], ['bots', M.x + 90, M.y + 520], ['any HTTP client', M.x + M.w - 140, M.y + 520]];
  callers.forEach(([s, x, y], i) => {
    const a = E.outExpo(clamp((u - 1.4 - i * 0.12) / 0.35));
    if (a <= 0) return;
    ctx.save(); ctx.globalAlpha *= a;
    ctx.strokeStyle = hexA(C.white, 0.18); ctx.lineWidth = 1.5; ctx.setLineDash([5, 6]);
    ctx.beginPath(); ctx.moveTo(x, y); ctx.lineTo(cx, cy); ctx.stroke(); ctx.setLineDash([]);
    const w = measure(ctx, s, 600, 17, SANS) + 36;
    pill(ctx, x - w / 2, y, s, { size: 17, fam: SANS, h: 40, bg: '#121A1F' });
    ctx.restore();
  });
  // the curl request travels in and the answer travels back
  const rq = P(u, HT.req, HT.res);
  if (rq > 0 && rq < 1) { const [, x, y] = callers[0]; const p = rq < 0.5 ? rq * 2 : 2 - rq * 2; ctx.fillStyle = C.green; ctx.beginPath(); ctx.arc(lerp(x, cx, p), lerp(y, cy, p), 6, 0, TAU); ctx.fill(); }
  ctx.restore();
  caption(ctx, 'Modules can define their own HTTP routes, for webhooks, bots or plain curl.', u, 4.6, 960);
  ctx.restore();
}

// ======================================================================
// TEMPLATES: three ready-to-run starter apps
// ======================================================================
const TP = { cmd: [0.4, 1.2], cards: 1.6, reply: 3.6 };
const TCMD = 'spacetime dev --template llm-chat-ts';

function tCard(ctx, x, y, w, h, id, title, u, t, draw) {
  glass(ctx, x, y, w, h, 20);
  text(ctx, id, x + 28, y + 48, { size: 16, weight: 700, fill: C.green });
  text(ctx, title, x + 28, y + 84, { size: 22, weight: 700, fam: SANS });
  ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(x, y + 108, w, 1);
  draw(x, y + 108);
}

function sTmpl(ctx, t) {
  const u = t - T.tmpl, len = T.plus - T.tmpl;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'NEW · STARTER TEMPLATES', [{ s: 'Three new' }, { s: 'starter templates.', fill: 'brand' }]);
  // command
  ctx.save();
  enter(ctx, u, 0.2, 30);
  commandChip(ctx, TCMD, CX, 262, u, TP.cmd[0], TP.cmd[1]);
  ctx.restore();

  const W3 = 520, G3 = 40, x0 = CX - (3 * W3 + 2 * G3) / 2, y0 = 370, h = 500;
  const cards = [
    ['llm-chat-ts', 'AI chat app', (x, y) => {
      const ua = E.outExpo(clamp((u - TP.cards - 0.6) / 0.35));
      if (ua > 0) { ctx.save(); ctx.globalAlpha *= ua; R.bubble(ctx, 'Name three uses for a lighthouse.', x + W3 - 24, y + 70, 'right', 1); ctx.restore(); }
      if (u > TP.cards + 1.0 && u < TP.reply) spinner(ctx, x + 44, y + 150, 8, t, C.white);
      const ra = E.outExpo(clamp((u - TP.reply) / 0.35));
      if (ra > 0) {
        ctx.save(); ctx.globalAlpha *= ra;
        ['1. Guide ships at night', '2. Mark a dangerous coast', '3. A very tall lamp'].forEach((s, i) => text(ctx, s, x + 32, y + 160 + i * 36, { size: 20, fam: SANS, weight: 500 }));
        ctx.restore();
      }
      text(ctx, 'Calls an LLM from a procedure.', x + 28, y + 330, { size: 18, fam: SANS, fill: hexA(C.white, 0.55) });
      text(ctx, 'Bring your own API key.', x + 28, y + 358, { size: 18, fam: SANS, fill: hexA(C.white, 0.55) });
    }],
    ['hangman-react-ts', 'Multiplayer Hangman', (x, y) => {
      const word = 'SPACE', guessed = 'SAE'.slice(0, 1 + Math.floor(clamp((u - TP.cards - 0.6) / 0.8) * 2.99));
      [...word].forEach((ch, i) => {
        const lx = x + 70 + i * 78, ly = y + 150;
        ctx.fillStyle = hexA(C.white, 0.4); ctx.fillRect(lx, ly + 10, 56, 3);
        if (guessed.includes(ch)) text(ctx, ch, lx + 28, ly, { size: 40, weight: 800, fam: SANS, align: 'center' });
      });
      text(ctx, 'Guessed: ' + guessed.split('').join(' '), x + 28, y + 230, { size: 18, fill: hexA(C.white, 0.6) });
      text(ctx, 'One shared game for everyone.', x + 28, y + 330, { size: 18, fam: SANS, fill: hexA(C.white, 0.55) });
    }],
    ['money-exchange-react-ts', 'Money exchange', (x, y) => {
      const sent = u > TP.cards + 2.0;
      [['you', sent ? '$75.00' : '$100.00'], ['sam', sent ? '$125.00' : '$100.00']].forEach(([n, v], i) => {
        const ry = y + 50 + i * 60;
        rr(ctx, x + 24, ry, W3 - 48, 48, 8); ctx.fillStyle = hexA(C.white, 0.04 + (sent ? 0.05 * pulse(u - TP.cards - 2.0, 2) : 0)); ctx.fill();
        text(ctx, n, x + 48, ry + 31, { size: 20, fill: hexA(C.white, 0.8) });
        text(ctx, v, x + W3 - 48, ry + 31, { size: 20, weight: 700, align: 'right' });
      });
      const ta = E.outCubic(P(u, TP.cards + 1.6, TP.cards + 2.0));
      if (ta > 0) text(ctx, 'transfer $25.00 → sam', x + 28, y + 210, { size: 18, fill: C.green, alpha: ta });
      text(ctx, 'Play money, private accounts.', x + 28, y + 330, { size: 18, fam: SANS, fill: hexA(C.white, 0.55) });
    }],
  ];
  cards.forEach(([id, title, draw], i) => {
    const lt = u - (TP.cards + i * 0.18);
    if (lt < 0) return;
    const a = E.outExpo(clamp(lt / 0.5));
    ctx.save(); ctx.globalAlpha *= a; ctx.translate(0, (1 - a) * 50);
    tCard(ctx, x0 + i * (W3 + G3), y0, W3, h, id, title, u, t, draw);
    ctx.restore();
  });
  caption(ctx, 'Three new templates: an AI chat, a multiplayer game and a money-transfer demo.', u, 4.6, 960);
  ctx.restore();
}

// ======================================================================
// PLUS cards, then the outro
// ======================================================================
const PLUS = [
  { k: 'RUST, C# AND C++ MODULES', title: ['Leaner', 'reducer calls'], d: ['Reducers run on their own thread,', 'without async overhead.'] },
  { k: 'SAMPLE GAME', title: ['Blackholio,', 'complete in Godot'], d: ['Leaderboard and splitting, plus', 'a new TypeScript version.'] },
];

const intro = R.makeIntro({ pre: 'v2.', from: '3', to: '4', sub: 'What’s new since 2.3', T });
const plus = R.makeCards({ start: T.plus, end: T.outro, kick: 'MODULES AND SAMPLES', parts: [{ s: 'Also in' }, { s: '2.4.', fill: 'brand' }], list: PLUS, toOutro: true });
const outro = R.makeOutro({ version: '2.4', T });
const { frame, setSlow } = R.makeReel({ DUR, T, SCENES, label: '/  RELEASE v2.4', draw: [intro, sHttp, sTmpl, plus, outro], seed: 4 });

module.exports = { frame, setSlow, DUR, T, SCENES, HT, TP, PLUS };
