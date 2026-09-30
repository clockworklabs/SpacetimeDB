// SpacetimeDB v2.10 release video (v2.9.0 → v2.10.0), 27 s @ 120 BPM. See DECISIONS.md for what's shown and why.
const L = require('../lib');
const { W, C, SANS, MONO, clamp, lerp, P, E, pulse, hexA, rr, glass, measure, text } = L;
const U = require('../ui');
const { CX, TAU, riseParts, kicker, check, spinner } = U;
const R = require('../release');
const { agentChat, linkPulses } = R;
const { slide, panel, enter, status, caption, pill, bubble, header } = R;

const DUR = 27;
const T = { intro: 0, mcp: 5.5, plus: 14.5, outro: 21, end: DUR };
const SCENES = [
  { t: T.intro, name: 'v2.10' },
  { t: T.mcp, name: 'MCP ON MAINCLOUD' },
  { t: T.plus, name: 'ALSO IN 2.10' },
  { t: T.outro, name: 'SPACETIMEDB' },
];

// ======================================================================
// 2 — MCP ON MAINCLOUD: chat with an agent that reads and writes a live Maincloud database
// ======================================================================
// Chat timeline (scene-local u). kind: user bubble, tool call, agent reply.
const CHAT = [
  { u: 1.0, kind: 'user', s: 'How many players are online in my-game?' },
  { u: 2.0, kind: 'tool', s: 'sql', d: 'SELECT * FROM player WHERE online = true' },
  { u: 2.8, kind: 'agent', s: '4 players are online right now.' },
  { u: 3.9, kind: 'user', s: 'Post a welcome message for them.' },
  { u: 4.9, kind: 'tool', s: 'call', d: 'send_message("Welcome back!")' },
  { u: 5.7, kind: 'agent', s: 'Done. It’s live for everyone in the game.' },
];
const TOOL_RUN = 0.45;
const PLAYERS = [['alice', true], ['bob', true], ['carol', false], ['dave', true], ['erin', true]];

function sMcp(ctx, t) {
  const u = t - T.mcp;
  const len = T.plus - T.mcp;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'MCP · NOW ON MAINCLOUD', [{ s: 'Connect your AI agent to your' }, { s: 'Maincloud databases.', fill: 'brand' }]);

  // chat
  const cp = { x: 140, y: 270, w: 1000, h: 660 };
  agentChat(ctx, u, t, { ...cp, title: 'your AI agent', chat: CHAT, ys: [370, 455, 525, 625, 710, 780], toolRun: TOOL_RUN });

  // live database on Maincloud
  const dp = { x: 1190, y: 270, w: 590, h: 660 };
  ctx.save();
  enter(ctx, u, 0.45, 40);
  glass(ctx, dp.x, dp.y, dp.w, dp.h, 20);
  text(ctx, 'MAINCLOUD', dp.x + 32, dp.y + 50, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  text(ctx, 'my-game', dp.x + 32, dp.y + 98, { size: 34, weight: 800, fam: SANS });
  ctx.fillStyle = C.green; ctx.globalAlpha *= 0.6 + 0.4 * Math.sin(t * 5) ** 2;
  ctx.beginPath(); ctx.arc(dp.x + dp.w - 88, dp.y + 87, 6, 0, TAU); ctx.fill();
  ctx.globalAlpha /= 0.6 + 0.4 * Math.sin(t * 5) ** 2;
  text(ctx, 'live', dp.x + dp.w - 72, dp.y + 93, { size: 16, fill: hexA(C.white, 0.6) });
  ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(dp.x, dp.y + 128, dp.w, 1);
  // player table
  const q = CHAT[1], qa = u - q.u;
  text(ctx, 'player', dp.x + 32, dp.y + 172, { size: 18, weight: 700 });
  text(ctx, 'name', dp.x + 48, dp.y + 208, { size: 14, weight: 600, tracking: 2, fill: hexA(C.white, 0.4) });
  text(ctx, 'online', dp.x + 360, dp.y + 208, { size: 14, weight: 600, tracking: 2, fill: hexA(C.white, 0.4) });
  PLAYERS.forEach(([n, on], i) => {
    const ry = dp.y + 226 + i * 44;
    const hit = on ? E.outCubic(P(qa, 0.08 * i, 0.08 * i + 0.25)) * (1 - 0.6 * P(qa, 2.2, 3.0)) : 0;
    rr(ctx, dp.x + 24, ry, dp.w - 48, 38, 8);
    ctx.fillStyle = hexA(C.white, 0.025 + 0.05 * hit); ctx.fill();
    if (hit > 0) { ctx.strokeStyle = hexA(C.green, 0.6 * hit); ctx.lineWidth = 1.5; ctx.stroke(); }
    text(ctx, n, dp.x + 48, ry + 26, { size: 18, fill: hexA(C.white, 0.85) });
    text(ctx, on ? 'true' : 'false', dp.x + 360, ry + 26, { size: 18, fill: on ? hexA(C.white, 0.85) : hexA(C.white, 0.4) });
  });
  // message table: a row appears when the agent calls the reducer
  const c = CHAT[4], ca = u - c.u - TOOL_RUN;
  text(ctx, 'message', dp.x + 32, dp.y + 490, { size: 18, weight: 700 });
  const MSGS = [['bob', 'gg everyone']];
  if (ca > 0) MSGS.push(['agent', 'Welcome back!']);
  MSGS.forEach(([who, s], i) => {
    const ry = dp.y + 510 + i * 48;
    const na = i === 1 ? E.outExpo(clamp(ca / 0.4)) : 1;
    const flash = i === 1 ? pulse(ca, 2.5) : 0;
    ctx.save();
    ctx.globalAlpha *= na;
    ctx.translate((1 - na) * 30, 0);
    rr(ctx, dp.x + 24, ry, dp.w - 48, 40, 8);
    ctx.fillStyle = hexA(C.white, 0.025 + 0.06 * flash); ctx.fill();
    if (i === 1) { ctx.strokeStyle = hexA(C.green, 0.25 + 0.6 * flash); ctx.lineWidth = 1.5; ctx.stroke(); }
    text(ctx, who, dp.x + 48, ry + 27, { size: 18, fill: hexA(C.white, 0.5) });
    text(ctx, s, dp.x + 160, ry + 27, { size: 18, fill: hexA(C.white, 0.9) });
    ctx.restore();
  });
  ctx.restore();

  // link between the agent and the panel: data pulses while a tool runs
  linkPulses(ctx, u, { x0: cp.x + cp.w, x1: dp.x, y: 600, calls: [CHAT[1].u, CHAT[4].u] });

  if (u > 6.4) text(ctx, 'spacetime mcp --server maincloud', CX, 990, { size: 20, align: 'center', fill: hexA(C.white, 0.55), alpha: E.outCubic(P(u, 6.4, 6.8)) });
  ctx.restore();
}


// ======================================================================
// PLUS cards, then the outro
// ======================================================================
const PLUS = [
  { k: 'C# MODULES · .NET 10', title: ['Faster reducer', 'dispatch'], d: ['Direct calls: 6–12% faster inserts,', 'filters and scans in benchmarks.'] },
];

const intro = R.makeIntro({ pre: 'v2.', from: '9', to: '10', sub: 'What’s new since 2.9', T });
const plus = R.makeCards({ start: T.plus, end: T.outro, kick: 'PERFORMANCE', parts: [{ s: 'Also in' }, { s: '2.10.', fill: 'brand' }], list: PLUS, toOutro: true });
const outro = R.makeOutro({ version: '2.10', T });
const { frame, setSlow } = R.makeReel({ DUR, T, SCENES, label: '/  RELEASE v2.10', draw: [intro, sMcp, plus, outro], seed: 10 });

module.exports = { frame, setSlow, DUR, T, SCENES, CHAT, TOOL_RUN, PLUS };
