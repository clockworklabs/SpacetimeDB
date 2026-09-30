// SpacetimeDB v2.7 release video (v2.6.0 → published 2.7.0 = v2.7.0-hotfix3), 46.5 s @ 120 BPM.
// See DECISIONS.md for what's shown and why.
const L = require('../lib');
const { C, SANS, MONO, clamp, lerp, P, E, pulse, hexA, rr, glass, measure, text, drawMark } = L;
const U = require('../ui');
const { CX, TAU, check, spinner } = U;
const R = require('../release');
const { laneFrame, beforeAfter, planMarker, resultLine, typedCommand, agentChat, linkPulses, slide, panel, enter, status, caption, pill, bubble, code, SYN, header } = R;

const DUR = 46.5;
const T = { intro: 0, mcp: 5.5, lock: 15, uniq: 23.5, dx: 32.5, outro: 40.5, end: DUR };
const SCENES = [
  { t: T.intro, name: 'v2.7' },
  { t: T.mcp, name: 'MCP' },
  { t: T.lock, name: 'DATABASE LOCK' },
  { t: T.uniq, name: 'UNIQUE CONSTRAINTS' },
  { t: T.dx, name: 'ALSO IN 2.7' },
  { t: T.outro, name: 'SPACETIMEDB' },
];

// ======================================================================
// MCP: every database gets an MCP endpoint (local/self-hosted in 2.7)
// ======================================================================
const CHAT = [
  { u: 1.0, kind: 'user', s: 'What tables does my-game have?' },
  { u: 2.0, kind: 'tool', s: 'get_schema', d: '{}' },
  { u: 2.8, kind: 'agent', s: 'Two: player and inventory.' },
  { u: 3.9, kind: 'user', s: 'Give alice 100 gold.' },
  { u: 4.9, kind: 'tool', s: 'call', d: 'add_gold ["alice", 100]' },
  { u: 5.7, kind: 'agent', s: 'Done. alice now has 350 gold.' },
];
const TOOL_RUN = 0.45;
const TOOLS = ['ping', 'get_schema', 'sql', 'call'];
const GOLD = [['alice', 250], ['bob', 120], ['carol', 90]];

function sMcp(ctx, t) {
  const u = t - T.mcp, len = T.lock - T.mcp;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'NEW · MCP ENDPOINT', [{ s: 'An MCP endpoint for' }, { s: 'every database.', fill: 'brand' }]);

  const cp = { x: 140, y: 270, w: 1000, h: 620 };
  agentChat(ctx, u, t, { ...cp, title: 'your AI agent', chat: CHAT, ys: [370, 455, 525, 625, 710, 780], toolRun: TOOL_RUN });

  // the database's MCP endpoint and its live data
  const dp = { x: 1190, y: 270, w: 590, h: 620 };
  ctx.save();
  enter(ctx, u, 0.45, 40);
  glass(ctx, dp.x, dp.y, dp.w, dp.h, 20);
  text(ctx, 'MCP ENDPOINT', dp.x + 32, dp.y + 50, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  rr(ctx, dp.x + 24, dp.y + 70, dp.w - 48, 46, 10); ctx.fillStyle = hexA(C.white, 0.05); ctx.fill();
  text(ctx, 'POST', dp.x + 42, dp.y + 100, { size: 16, weight: 700, fill: C.green });
  text(ctx, 'localhost:3000/v1/database/my-game/mcp', dp.x + 96, dp.y + 100, { size: 16, fill: hexA(C.white, 0.85) });
  text(ctx, 'TOOLS', dp.x + 32, dp.y + 164, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  let tx = dp.x + 32;
  TOOLS.forEach(n => {
    const m = CHAT.find(c => c.kind === 'tool' && c.s === n);
    const hit = m ? E.outCubic(P(u - m.u, 0, 0.2)) * (1 - 0.6 * P(u - m.u, 1.6, 2.4)) : 0;
    const w = pill(ctx, tx, dp.y + 200, n, { size: 17, h: 42, stroke: hit > 0 ? hexA(C.green, 0.3 + 0.6 * hit) : hexA(C.white, 0.2) });
    tx += w + 12;
  });
  ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(dp.x, dp.y + 250, dp.w, 1);
  text(ctx, 'player', dp.x + 32, dp.y + 296, { size: 18, weight: 700 });
  text(ctx, 'gold', dp.x + dp.w - 48, dp.y + 296, { size: 14, weight: 600, tracking: 2, align: 'right', fill: hexA(C.white, 0.4) });
  const ca = u - CHAT[4].u - TOOL_RUN;
  GOLD.forEach(([n, g], i) => {
    const ry = dp.y + 316 + i * 52;
    const hit = i === 0 && ca > 0 ? pulse(ca, 2.5) : 0;
    rr(ctx, dp.x + 24, ry, dp.w - 48, 42, 8); ctx.fillStyle = hexA(C.white, 0.03 + 0.08 * hit); ctx.fill();
    if (i === 0 && ca > 0) { ctx.strokeStyle = hexA(C.green, 0.3 + 0.6 * hit); ctx.lineWidth = 1.5; ctx.stroke(); }
    text(ctx, n, dp.x + 48, ry + 28, { size: 19, fill: hexA(C.white, 0.85) });
    text(ctx, String(i === 0 && ca > 0 ? g + 100 : g), dp.x + dp.w - 48, ry + 28, { size: 19, weight: 700, align: 'right' });
  });
  text(ctx, 'Runs with your identity,', dp.x + 32, dp.y + dp.h - 70, { size: 19, weight: 500, fam: SANS, fill: hexA(C.white, 0.65), alpha: E.outCubic(P(u, 6.3, 6.7)) });
  text(ctx, 'exactly like the HTTP API.', dp.x + 32, dp.y + dp.h - 42, { size: 19, weight: 500, fam: SANS, fill: hexA(C.white, 0.65), alpha: E.outCubic(P(u, 6.3, 6.7)) });
  ctx.restore();

  // link between the agent and the panel: data pulses while a tool runs
  linkPulses(ctx, u, { x0: cp.x + cp.w, x1: dp.x, y: 580, calls: [CHAT[1].u, CHAT[4].u] });
  caption(ctx, 'Every database now has an MCP endpoint: schema, SQL and reducer calls for AI agents.', u, 6.6, 960);
  ctx.restore();
}

// ======================================================================
// LOCK: a locked database can't be deleted or wiped
// ======================================================================
const LK = { rows: [
  { c0: 0.5, c1: 1.1, cmd: 'spacetime lock my-game', out: 1.4, ok: true, s: 'Database c200a4…e91f is now locked. It cannot be deleted until unlocked.' },
  { c0: 2.2, c1: 2.8, cmd: 'spacetime delete my-game', out: 3.1, ok: false, s: '403 Forbidden' },
  { c0: 3.6, c1: 4.3, cmd: 'spacetime publish -c=always my-game', out: 4.6, ok: false, s: '403 Forbidden' },
  { c0: 5.3, c1: 5.9, cmd: 'spacetime unlock my-game', out: 6.2, ok: true, s: '' },
] };

function padlock(ctx, x, y, s, open, shake) {
  ctx.save();
  ctx.translate(x + Math.sin(shake * 40) * 8 * shake, y);
  ctx.strokeStyle = C.white; ctx.lineWidth = s * 0.12; ctx.lineCap = 'round';
  ctx.beginPath(); ctx.arc(0, -s * 0.35 - open * s * 0.3, s * 0.32, Math.PI, 0); ctx.lineTo(s * 0.32, -s * 0.35 - open * s * 0.3 + (open > 0 ? s * 0.05 : s * 0.15)); ctx.stroke();
  ctx.beginPath(); ctx.moveTo(-s * 0.32, -s * 0.35 - open * s * 0.3); ctx.lineTo(-s * 0.32, -s * 0.2); ctx.stroke();
  rr(ctx, -s * 0.5, -s * 0.2, s, s * 0.8, s * 0.12); ctx.fillStyle = C.white; ctx.fill();
  ctx.fillStyle = '#121A1F'; ctx.beginPath(); ctx.arc(0, s * 0.15, s * 0.09, 0, TAU); ctx.fill();
  ctx.restore();
}

function sLock(ctx, t) {
  const u = t - T.lock, len = T.uniq - T.lock;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'NEW · SPACETIME LOCK', [{ s: 'Lock a database' }, { s: 'against deletion.', fill: 'brand' }]);

  const Tm = { x: 140, y: 270, w: 1080, h: 560 };
  ctx.save();
  enter(ctx, u, 0.25, 40);
  panel(ctx, Tm.x, Tm.y, Tm.w, Tm.h, 'terminal');
  LK.rows.forEach((r, i) => {
    if (u < r.c0 - 0.05) return;
    const y0 = Tm.y + 120 + i * 108;
    typedCommand(ctx, r.cmd, Tm.x + 32, y0, u, r.c0, r.c1);
    const oa = E.outCubic(P(u, r.out, r.out + 0.25));
    if (oa <= 0) return;
    if (r.ok) {
      check(ctx, Tm.x + 60, y0 + 38, 14, C.green, oa);
      if (r.s) R.fitText(ctx, r.s, Tm.x + 88, y0 + 44, Tm.w - 120, { size: 18, fam: MONO, weight: 500, fill: hexA(C.white, 0.8), alpha: oa });
      else text(ctx, 'unlocked', Tm.x + 88, y0 + 44, { size: 18, fill: C.green, alpha: oa });
    } else {
      resultLine(ctx, r.s, Tm.x + 60, y0 + 44, oa, { ok: false, weight: 600 });
    }
  });
  ctx.restore();

  // the database, with its padlock
  const D = { x: 1260, y: 270, w: 520, h: 560 };
  ctx.save();
  enter(ctx, u, 0.45, 40);
  glass(ctx, D.x, D.y, D.w, D.h, 20);
  const locked = u > LK.rows[0].out && u < LK.rows[3].out;
  drawMark(ctx, D.x + D.w / 2, D.y + 200, 180, { color: hexA(C.white, locked ? 0.95 : 0.6) });
  const open = u < LK.rows[0].out ? 1 - E.outBack(clamp((u - LK.rows[0].out + 0.3) / 0.3)) * 0 : u > LK.rows[3].out ? E.outCubic(P(u, LK.rows[3].out, LK.rows[3].out + 0.3)) : 0;
  const pa = E.outBack(clamp((u - LK.rows[0].out + 0.2) / 0.35));
  const shake = Math.max(pulse(u - LK.rows[1].out, 4) * (u > LK.rows[1].out ? 1 : 0), pulse(u - LK.rows[2].out, 4) * (u > LK.rows[2].out ? 1 : 0));
  if (pa > 0) { ctx.save(); ctx.globalAlpha *= pa; padlock(ctx, D.x + D.w / 2 + 70, D.y + 280, 90, u < LK.rows[0].out ? 1 : open, shake); ctx.restore(); }
  text(ctx, 'my-game', D.x + D.w / 2, D.y + 420, { size: 30, weight: 800, fam: SANS, align: 'center' });
  text(ctx, locked ? 'locked' : u > LK.rows[3].out ? 'unlocked' : 'live', D.x + D.w / 2, D.y + 462, { size: 18, weight: 600, align: 'center', fill: locked ? C.green : hexA(C.white, 0.55) });
  ctx.restore();
  caption(ctx, 'A locked database can’t be deleted or wiped until someone unlocks it.', u, 6.6, 930);
  ctx.restore();
}

// ======================================================================
// UNIQUE: add #[unique] to a live table without wiping it
// ======================================================================
const UQ = { pub: 1.6, out: 2.4, badge: 3.0, dup: 4.6 };
const USERS = [['1', 'alice@example.com'], ['2', 'bob@example.com'], ['3', 'carol@example.com']];

function uLane(ctx, x, y, w, h, u, now) {
  laneFrame(ctx, x, y, w, h, { now, version: now ? 'v2.7' : 'v2.6', title: now ? 'Migrated in place' : 'Manual migration' });
  text(ctx, 'user', x + 32, y + 104, { size: 18, weight: 700 });
  const ba = now ? E.outBack(clamp((u - UQ.badge) / 0.35)) : 0;
  text(ctx, 'id', x + 48, y + 138, { size: 14, weight: 600, tracking: 2, fill: hexA(C.white, 0.4) });
  text(ctx, 'email', x + 130, y + 138, { size: 14, weight: 600, tracking: 2, fill: hexA(C.white, 0.4) });
  if (ba > 0) { ctx.save(); ctx.translate(x + 218, y + 132); ctx.scale(ba, ba); pill(ctx, 0, 0, 'unique', { size: 13, h: 24, pad: 22, stroke: hexA(C.green, 0.8) }); ctx.restore(); }
  USERS.forEach(([id, e], i) => {
    const ry = y + 154 + i * 48;
    rr(ctx, x + 24, ry, w - 48, 40, 8); ctx.fillStyle = hexA(C.white, 0.04); ctx.fill();
    text(ctx, id, x + 48, ry + 27, { size: 18, fill: hexA(C.white, 0.6) });
    text(ctx, e, x + 130, ry + 27, { size: 18, fill: hexA(C.white, 0.85) });
  });
  // publish output
  const oy = y + 330;
  if (u > UQ.pub) {
    text(ctx, '$ spacetime publish my-game', x + 32, oy, { size: 18, fill: hexA(C.white, 0.6) });
    const oa = E.outCubic(P(u, UQ.out, UQ.out + 0.3));
    if (oa > 0) {
      if (now) {
        planMarker(ctx, x + 34, oy + 40, C.green, oa);
        text(ctx, '  Created unique constraint user_email_key', x + 32, oy + 40, { size: 17, fill: C.green, alpha: oa });
        text(ctx, '  on [email] of table user', x + 32, oy + 68, { size: 17, fill: C.green, alpha: oa });
      } else {
        text(ctx, 'Adding a unique constraint user_email_key', x + 32, oy + 40, { size: 17, fill: C.red, alpha: oa });
        text(ctx, 'requires a manual migration', x + 32, oy + 68, { size: 17, fill: C.red, alpha: oa });
      }
    }
  }
  const na = E.outCubic(P(u, UQ.dup, UQ.dup + 0.3));
  if (na > 0) {
    if (now) text(ctx, 'Duplicates? Publish stops and lists them.', x + 32, y + h - 32, { size: 19, weight: 500, fam: SANS, fill: hexA(C.white, 0.7), alpha: na });
    else text(ctx, 'Usual way out: wipe the database.', x + 32, y + h - 32, { size: 19, weight: 500, fam: SANS, fill: hexA(C.white, 0.6), alpha: na });
  }
}

function sUniq(ctx, t) {
  const u = t - T.uniq, len = T.dx - T.uniq;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'MIGRATIONS', [{ s: 'Add unique constraints' }, { s: 'to live tables.', fill: 'brand' }]);
  // the one-line change
  ctx.save();
  enter(ctx, u, 0.3, 30);
  const w1 = measure(ctx, '#[unique]', 500, 22, MONO), w2 = measure(ctx, ' email: String,', 500, 22, MONO);
  const x0 = CX - (w1 + w2) / 2;
  rr(ctx, x0 - 32, 260, w1 + w2 + 64, 64, 14); ctx.fillStyle = '#121A1F'; ctx.fill();
  ctx.strokeStyle = hexA(C.white, 0.12); ctx.lineWidth = 1.5; ctx.stroke();
  const ia = E.outCubic(P(u, 0.8, 1.2));
  text(ctx, '#[unique]', x0, 300, { size: 22, fill: SYN.attr, alpha: ia });
  text(ctx, ' email: String,', x0 + w1, 300, { size: 22 });
  if (ia > 0) { rr(ctx, x0 - 6, 272, (w1 + 12) * ia, 40, 6); ctx.strokeStyle = hexA(C.green, 0.8); ctx.stroke(); }
  ctx.restore();
  beforeAfter(ctx, u, (ctx, x, y, w, h, now) => uLane(ctx, x, y, w, h, u, now), { old: [140, 360, 800, 500], now: [980, 360, 800, 500], enterAt: [0.5, 0.65] });
  caption(ctx, 'Add #[unique] or #[primary_key] to a live table and republish. Your rows stay.', u, 5.2, 950);
  ctx.restore();
}

// ======================================================================
// CARDS, then the outro
// ======================================================================
const DX = [
  { k: 'C# MODULES', title: ['.NET 10', 'for C# modules'], d: ['Compiled ahead of time', 'on Windows and Linux.'] },
  { k: 'UNREAL SDK', title: ['Typed queries', 'in Unreal'], d: ['C++ and Blueprint,', 'no SQL strings.'] },
  { k: 'SVELTE', title: ['Sign in without', 'a reload'], d: ['reconnect(builder)', 'swaps tokens in Svelte.'] },
  { k: 'CLI', title: ['SQL output', 'as JSON'], d: ['spacetime sql', '--format json'] },
  { k: 'TYPESCRIPT MODULES', title: ['Schedules', 'across files'], d: ['onSchedule links a reducer', 'to its schedule table.'] },
];
const intro = R.makeIntro({ pre: 'v2.', from: '6', to: '7', sub: 'What’s new since 2.6', T });
const dx = R.makeCards({ start: T.dx, end: T.outro, kick: 'MORE FEATURES', parts: [{ s: 'Also in' }, { s: '2.7.', fill: 'brand' }], list: DX, toOutro: true });
const outro = R.makeOutro({ version: '2.7', T });
const { frame, setSlow } = R.makeReel({ DUR, T, SCENES, label: '/  RELEASE v2.7', draw: [intro, sMcp, sLock, sUniq, dx, outro], seed: 7 });

module.exports = { frame, setSlow, DUR, T, SCENES, CHAT, TOOL_RUN, LK, UQ };
