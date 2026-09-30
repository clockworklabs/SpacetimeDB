// SpacetimeDB v2.9 release video (v2.8.0 → v2.9.0: 2.8.1, 2.8.2, 2.8.3, 2.9.0), 44.5 s @ 120 BPM.
// See DECISIONS.md for what's shown and why.
const L = require('../lib');
const { W, H, C, SANS, MONO, clamp, lerp, P, E, pulse, hexA, rr, glass, measure, text, drawMark, A } = L;
const U = require('../ui');
const { CX, TAU, riseParts, kicker, check, spinner } = U;
const R = require('../release');
const { typedCommand } = R;
const { agentChat, linkPulses } = R;
const { slide, panel, enter, status, caption, pill, bubble, header } = R;

const DUR = 44.5;
const T = { intro: 0, mcp: 5.5, plugins: 15, unity: 23, mods: 31.5, outro: 38.5, end: DUR };
const SCENES = [
  { t: T.intro, name: 'v2.9' },
  { t: T.mcp, name: 'SPACETIME MCP' },
  { t: T.plugins, name: 'AGENT PLUGINS' },
  { t: T.unity, name: 'UNITY' },
  { t: T.mods, name: 'ALSO IN 2.9' },
  { t: T.outro, name: 'SPACETIMEDB' },
];

// ======================================================================
// 2 — SPACETIME MCP: a coding agent inspects a local database through MCP tools
// ======================================================================
// Chat timeline (scene-local u). kind: user bubble, tool call, agent reply.
const CHAT = [
  { u: 1.0, kind: 'user', s: 'Which databases do I have?' },
  { u: 2.0, kind: 'tool', s: 'list_databases', d: '{}' },
  { u: 2.8, kind: 'agent', s: 'Two: chat-app and my-game.' },
  { u: 3.9, kind: 'user', s: 'My game client sees no players. Why?' },
  { u: 4.9, kind: 'tool', s: 'get_schema', d: 'database: "my-game"' },
  { u: 5.7, kind: 'agent', s: 'player is a private table. Make it public?' },
];
const TOOL_RUN = 0.45;
const TOOLS = ['list_databases', 'get_schema', 'sql', 'call'];

function sMcp(ctx, t) {
  const u = t - T.mcp;
  const len = T.plugins - T.mcp;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'NEW · SPACETIME MCP', [{ s: 'Connect your coding agent' }, { s: 'to your database.', fill: 'brand' }]);

  // chat
  const cp = { x: 140, y: 270, w: 1000, h: 660 };
  agentChat(ctx, u, t, { ...cp, title: 'your coding agent', chat: CHAT, ys: [370, 455, 525, 625, 710, 780], toolRun: TOOL_RUN });

  // the MCP bridge on the local server: its tools light up as the agent uses them
  const dp = { x: 1190, y: 270, w: 590, h: 660 };
  ctx.save();
  enter(ctx, u, 0.45, 40);
  glass(ctx, dp.x, dp.y, dp.w, dp.h, 20);
  text(ctx, 'LOCAL SERVER', dp.x + 32, dp.y + 50, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  text(ctx, 'spacetime mcp', dp.x + 32, dp.y + 98, { size: 32, weight: 700 });
  const blink = 0.6 + 0.4 * Math.sin(t * 5) ** 2;
  ctx.save();
  ctx.fillStyle = C.green; ctx.globalAlpha *= blink;
  ctx.beginPath(); ctx.arc(dp.x + dp.w - 104, dp.y + 87, 6, 0, TAU); ctx.fill();
  ctx.restore();
  text(ctx, 'ready', dp.x + dp.w - 88, dp.y + 93, { size: 16, fill: hexA(C.white, 0.6) });
  ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(dp.x, dp.y + 128, dp.w, 1);
  // tools
  text(ctx, 'TOOLS', dp.x + 32, dp.y + 172, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  TOOLS.forEach((n, i) => {
    const cx = dp.x + 32 + (i % 2) * 270, cy = dp.y + 200 + Math.floor(i / 2) * 58;
    const m = CHAT.find(c => c.kind === 'tool' && c.s === n);
    const hit = m ? E.outCubic(P(u - m.u, 0, 0.2)) * (1 - 0.6 * P(u - m.u, 1.6, 2.4)) : 0;
    rr(ctx, cx, cy, 256, 44, 10);
    ctx.fillStyle = hexA(C.white, 0.03 + 0.05 * hit); ctx.fill();
    ctx.strokeStyle = hit > 0 ? hexA(C.green, 0.2 + 0.6 * hit) : hexA(C.white, 0.12); ctx.lineWidth = 1.5; ctx.stroke();
    text(ctx, n, cx + 20, cy + 29, { size: 18, weight: 600, fill: hexA(C.white, 0.6 + 0.4 * hit) });
  });
  ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(dp.x, dp.y + 340, dp.w, 1);
  // databases: listed by list_databases, then my-game's tables are read by get_schema
  text(ctx, 'DATABASES', dp.x + 32, dp.y + 384, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  const la = u - CHAT[1].u - TOOL_RUN, sa = u - CHAT[4].u - TOOL_RUN;
  ['chat-app', 'my-game'].forEach((n, i) => {
    const a = E.outExpo(clamp((la - i * 0.1) / 0.35));
    if (a <= 0) return;
    const ry = dp.y + 404 + i * 52;
    ctx.save();
    ctx.globalAlpha *= a;
    ctx.translate((1 - a) * 30, 0);
    rr(ctx, dp.x + 24, ry, dp.w - 48, 42, 8);
    ctx.fillStyle = hexA(C.white, 0.04); ctx.fill();
    drawMark(ctx, dp.x + 50, ry + 21, 24, { color: hexA(C.white, 0.8) });
    text(ctx, n, dp.x + 74, ry + 28, { size: 19, weight: 600 });
    ctx.restore();
  });
  // my-game's tables
  [['player', 'private'], ['message', 'public']].forEach(([n, vis], i) => {
    const a = E.outExpo(clamp((sa - i * 0.1) / 0.35));
    if (a <= 0) return;
    const ry = dp.y + 516 + i * 50;
    const flag = vis === 'private' ? pulse(sa - 0.9, 2) + 0.5 * P(sa, 0.9, 1.2) : 0;
    ctx.save();
    ctx.globalAlpha *= a;
    ctx.translate((1 - a) * 30, 0);
    ctx.strokeStyle = hexA(C.white, 0.2); ctx.lineWidth = 1.5;
    ctx.beginPath(); ctx.moveTo(dp.x + 50, ry - 10 - (i ? 50 : 0)); ctx.lineTo(dp.x + 50, ry + 20); ctx.lineTo(dp.x + 70, ry + 20); ctx.stroke();
    text(ctx, n, dp.x + 84, ry + 27, { size: 19, fill: hexA(C.white, 0.85) });
    const tw = measure(ctx, vis, 600, 15, MONO) + 28;
    rr(ctx, dp.x + dp.w - 40 - tw, ry + 5, tw, 30, 15);
    ctx.fillStyle = '#0d1317'; ctx.fill();
    ctx.strokeStyle = flag > 0 ? hexA(C.green, clamp(0.3 + 0.6 * flag)) : hexA(C.white, 0.2); ctx.stroke();
    text(ctx, vis, dp.x + dp.w - 40 - tw / 2, ry + 25, { size: 15, weight: 600, align: 'center', fill: hexA(C.white, 0.75) });
    ctx.restore();
  });
  ctx.restore();

  // link between the agent and the panel: data pulses while a tool runs
  linkPulses(ctx, u, { x0: cp.x + cp.w, x1: dp.x, y: 600, calls: [CHAT[1].u, CHAT[4].u] });

  if (u > 6.4) text(ctx, 'List databases, read schemas, run SQL, call reducers. Uses your CLI login.', CX, 990, { size: 22, weight: 500, fam: SANS, align: 'center', fill: hexA(C.white, 0.6), alpha: E.outCubic(P(u, 6.4, 6.8)) });
  ctx.restore();
}

// ======================================================================
// 3 — PLUGINS for Claude Code and Codex: skills + MCP server in one install
// ======================================================================
const INSTALL = [
  { g: 'CLAUDE CODE', lines: ['claude plugin marketplace add clockworklabs/SpacetimeDB', 'claude plugin install spacetimedb@spacetimedb-plugins'] },
  { g: 'CODEX', lines: ['codex plugin marketplace add clockworklabs/SpacetimeDB --sparse .agents --sparse codex-plugin', 'codex plugin add spacetimedb@spacetimedb-plugins'] },
];
// [scene-local start, end] of typing for each command line, in order
const TYPE = [[0.6, 1.1], [1.2, 1.7], [1.9, 2.6], [2.7, 3.2]];
const DONE = 0.3; // check appears this long after an install line finishes typing
// The skills both plugins ship (skills/ at v2.9.0), plus the MCP server
const SKILLS = ['concepts', 'cli', 'mcp', 'rust-server', 'csharp-server', 'typescript-server', 'cpp-server', 'typescript-client', 'csharp-client', 'unity', 'unreal'];
const GET = 3.6;

function sPlugins(ctx, t) {
  const u = t - T.plugins;
  const len = T.unity - T.plugins;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'NEW · AGENT PLUGINS', [{ s: 'SpacetimeDB plugins for' }, { s: 'Claude Code and Codex.', fill: 'brand' }], { align: 'center' });

  // terminal
  const tm = { x: 140, y: 270, w: 1640, h: 350 };
  ctx.save();
  enter(ctx, u, 0.3, 40);
  panel(ctx, tm.x, tm.y, tm.w, tm.h, 'terminal');
  let li = 0;
  INSTALL.forEach((grp, gi) => {
    const gy = tm.y + 110 + gi * 130;
    text(ctx, grp.g, tm.x + 40, gy, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.45) });
    grp.lines.forEach((s, k) => {
      const [c0, c1] = TYPE[li++];
      const y = gy + 40 + k * 40;
      if (u < c0 - 0.05) return;
      typedCommand(ctx, s, tm.x + 40, y, u, c0, c1);
      if (k === 1 && u > c1 + DONE) {
        const cx = tm.x + 66 + measure(ctx, s, 500, 20, MONO) + 24;
        check(ctx, cx, y - 6, 14, C.green, E.outCubic(clamp((u - c1 - DONE) / 0.2)));
        text(ctx, 'installed', cx + 26, y, { size: 18, fill: C.green, alpha: E.outCubic(P(u, c1 + DONE + 0.1, c1 + DONE + 0.4)) });
      }
    });
  });
  ctx.restore();

  // what the plugin brings: every skill + the MCP server
  const ga = u - GET;
  if (ga > 0) {
    text(ctx, 'BOTH PLUGINS INSTALL', CX, 690, { size: 14, weight: 700, tracking: 4, align: 'center', fill: hexA(C.white, 0.5), alpha: E.outCubic(clamp(ga / 0.3)) });
    // two centered rows of skill chips
    const rows = [SKILLS.slice(0, 6), SKILLS.slice(6)];
    const gap = 14;
    let idx = 0;
    rows.forEach((row, r) => {
      const ws = row.map(s => measure(ctx, s, 600, 19, MONO) + 44);
      let x = CX - (ws.reduce((a, b) => a + b, 0) + gap * (row.length - 1)) / 2;
      row.forEach((s, i) => {
        const a = E.outExpo(clamp((ga - 0.1 - idx++ * 0.05) / 0.35));
        if (a > 0) {
          ctx.save();
          ctx.globalAlpha *= a;
          ctx.translate(0, (1 - a) * 16);
          pill(ctx, x, 745 + r * 62, s, { size: 19, h: 46, pad: 44, bg: hexA(C.white, 0.04), stroke: hexA(C.white, 0.2) });
          ctx.restore();
        }
        x += ws[i] + gap;
      });
    });
    // the MCP server, highlighted
    const ma = E.outBack(clamp((ga - 0.9) / 0.4));
    if (ma > 0) {
      const lbl = 'MCP server · spacetime mcp';
      const lw = measure(ctx, lbl, 700, 20, MONO) + 80;
      ctx.save();
      ctx.translate(CX, 890); ctx.scale(ma, ma);
      rr(ctx, -lw / 2, -26, lw, 52, 26); ctx.fillStyle = '#0d1317'; ctx.fill();
      ctx.strokeStyle = hexA(C.green, 0.8); ctx.lineWidth = 1.5; ctx.stroke();
      text(ctx, '+', -lw / 2 + 28, 8, { size: 24, weight: 700, fill: C.green });
      text(ctx, lbl, -lw / 2 + 54, 7, { size: 20, weight: 700 });
      ctx.restore();
    }
  }
  ctx.restore();
}

// ======================================================================
// 4 — UNITY: the SDK works with Domain Reload turned off
// ======================================================================
// Simulated seconds on the lanes (illustrative, no real durations shown).
const LANE_SIM = [0, 4.2];
const LANE_PLAY = [2.0, 5.6]; // scene-local u over which the playhead sweeps
const TOGGLE = 1.2; // scene-local u when "Reload Domain" switches off
// [start, end] of each segment: 'reload' (waiting) or 'play' (a Play Mode session)
const LANES = [
  { label: 'Domain Reload on', segs: [['reload', 0.15, 2.6], ['play', 2.7, 4.2]] },
  { label: 'Domain Reload off', segs: [['play', 0.25, 1.2], ['play', 1.5, 2.45], ['play', 2.75, 3.7]] },
];
const laneSimAt = u => lerp(LANE_SIM[0], LANE_SIM[1], P(u, LANE_PLAY[0], LANE_PLAY[1]));
const laneUAt = s => lerp(LANE_PLAY[0], LANE_PLAY[1], (s - LANE_SIM[0]) / (LANE_SIM[1] - LANE_SIM[0]));

function playIcon(ctx, x, y, s, col) {
  ctx.fillStyle = col;
  ctx.beginPath(); ctx.moveTo(x - s * 0.4, y - s * 0.5); ctx.lineTo(x + s * 0.5, y); ctx.lineTo(x - s * 0.4, y + s * 0.5); ctx.closePath(); ctx.fill();
}

function sUnity(ctx, t) {
  const u = t - T.unity;
  const len = T.mods - T.unity;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'UNITY SDK', [{ s: 'Unity SDK supports' }, { s: 'disabled Domain Reload.', fill: 'brand' }]);

  // settings panel: Reload Domain switches off
  const S = { x: 140, y: 270, w: 620, h: 300 };
  ctx.save();
  enter(ctx, u, 0.3, 40);
  panel(ctx, S.x, S.y, S.w, S.h, 'Project Settings · Editor');
  ctx.drawImage(A.logos.unity, S.x + S.w - 76, S.y + 76, 44, 44);
  text(ctx, 'Enter Play Mode Options', S.x + 36, S.y + 110, { size: 24, weight: 700, fam: SANS });
  text(ctx, 'Reload Domain', S.x + 36, S.y + 180, { size: 22, weight: 500, fam: SANS, fill: hexA(C.white, 0.8) });
  const off = E.inOutCubic(P(u, TOGGLE, TOGGLE + 0.3));
  const tx = S.x + S.w - 120, ty = S.y + 172;
  rr(ctx, tx, ty - 18, 80, 36, 18);
  ctx.fillStyle = off < 0.5 ? hexA(C.white, 0.3) : hexA(C.white, 0.08); ctx.fill();
  ctx.fillStyle = C.white; ctx.beginPath(); ctx.arc(lerp(tx + 62, tx + 18, off), ty, 13, 0, TAU); ctx.fill();
  text(ctx, off < 0.5 ? 'on' : 'off', tx - 16, ty + 7, { size: 18, weight: 600, align: 'right', fill: hexA(C.white, 0.6) });
  const na = E.outCubic(P(u, TOGGLE + 0.4, TOGGLE + 0.8));
  if (na > 0) {
    check(ctx, S.x + 40, S.y + 244, 14, C.green, na);
    text(ctx, 'Now supported by the SpacetimeDB SDK', S.x + 66, S.y + 250, { size: 20, weight: 500, fam: SANS, fill: hexA(C.white, 0.85), alpha: na });
  }
  ctx.restore();

  // why it matters
  const G = { x: 140, y: 600, w: 620, h: 250 };
  ctx.save();
  enter(ctx, u, 0.6, 40);
  glass(ctx, G.x, G.y, G.w, G.h, 20);
  text(ctx, 'WHY IT MATTERS', G.x + 36, G.y + 56, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  ['Skipping the domain reload makes', 'entering Play Mode faster. The SDK', 'now resets itself on every Play.'].forEach((l, i) =>
    text(ctx, l, G.x + 36, G.y + 116 + i * 38, { size: 23, weight: 500, fam: SANS, fill: hexA(C.white, 0.8) }));
  ctx.restore();

  // lanes: with Domain Reload off, Play Mode starts right away, and the SDK starts clean each time
  const Ln = { x: 800, y: 270, w: 980, h: 580 };
  ctx.save();
  enter(ctx, u, 0.45, 40);
  glass(ctx, Ln.x, Ln.y, Ln.w, Ln.h, 20);
  text(ctx, 'PRESSING PLAY', Ln.x + 36, Ln.y + 56, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.5) });
  const tx0 = Ln.x + 60, tx1 = Ln.x + Ln.w - 50;
  const X = s => lerp(tx0, tx1, (s - LANE_SIM[0]) / (LANE_SIM[1] - LANE_SIM[0]));
  const sim = laneSimAt(u);
  LANES.forEach((ln, li) => {
    const now = li === 1;
    const ly = Ln.y + 170 + li * 250;
    const la = now ? 0.4 + 0.6 * off : 1 - 0.35 * off;
    ctx.save();
    ctx.globalAlpha *= la;
    text(ctx, ln.label, Ln.x + 36, ly - 30, { size: 22, weight: 700, fam: SANS, fill: hexA(C.white, now ? 0.95 : 0.7) });
    ctx.fillStyle = hexA(C.white, 0.1); ctx.fillRect(tx0, ly + 40, tx1 - tx0, 1.5);
    ln.segs.forEach(([kind, s0, s1], si) => {
      if (sim < s0) return;
      const e = Math.min(sim, s1);
      // press Play at the start of each run of segments
      if (kind === 'play' && (si === 0 || ln.segs[si - 1][0] === 'play')) {
        const pa = E.outBack(clamp((u - laneUAt(s0 - 0.15)) / 0.2));
        ctx.save(); ctx.translate(X(s0) - 18, ly + 40); ctx.scale(pa, pa); playIcon(ctx, 0, 0, 16, C.white); ctx.restore();
      }
      if (kind === 'reload') {
        ctx.save(); ctx.translate(X(s0) - 18, ly + 40); playIcon(ctx, 0, 0, 16, C.white); ctx.restore();
        rr(ctx, X(s0), ly + 22, X(e) - X(s0), 36, 8);
        ctx.fillStyle = hexA(C.white, 0.1); ctx.fill();
        ctx.strokeStyle = hexA(C.white, 0.3); ctx.lineWidth = 1.5; ctx.stroke();
        text(ctx, 'Reloading domain', X(s0) + 18, ly + 46, { size: 17, weight: 600, fill: hexA(C.white, 0.6) });
        if (sim < s1) spinner(ctx, X(e) - 22, ly + 40, 8, t, C.white);
      } else {
        rr(ctx, X(s0), ly + 22, X(e) - X(s0), 36, 8);
        ctx.fillStyle = hexA(C.green, 0.14); ctx.fill();
        ctx.strokeStyle = hexA(C.green, 0.6); ctx.lineWidth = 1.5; ctx.stroke();
        if (X(e) - X(s0) > 90) text(ctx, 'Playing', X(s0) + 16, ly + 46, { size: 17, weight: 600 });
        // each session starts with a clean SDK
        if (now) {
          const ca = E.outCubic(clamp((u - laneUAt(s0) - 0.1) / 0.25));
          check(ctx, X(s0) + 6, ly - 2, 13, C.green, ca);
          text(ctx, 'clean start', X(s0) + 28, ly + 2, { size: 16, weight: 600, fill: C.green, alpha: ca });
        }
      }
    });
    ctx.restore();
  });
  // playhead
  if (u > LANE_PLAY[0] && u < LANE_PLAY[1] + 0.5) {
    ctx.fillStyle = hexA(C.white, 0.5 * (1 - P(u, LANE_PLAY[1], LANE_PLAY[1] + 0.5)));
    ctx.fillRect(X(sim), Ln.y + 90, 1.5, Ln.h - 120);
  }
  ctx.restore();
  caption(ctx, 'Turn Domain Reload off: the SDK now starts clean every time you press Play.', u, 4.4, 960);
  ctx.restore();
}

// ======================================================================
// CARDS, then the outro
// ======================================================================
const MODS = [
  { k: 'RUST MODULES', title: ['Default values', 'for text columns'], d: ['#[default("…")] now works on', 'String columns, like C# and TS.'] },
  { k: 'C# MODULES', title: ['Faster', 'C# modules'], d: ['Reused buffers: 3–29% faster', 'in some of our benchmarks.'] },
];

const intro = R.makeIntro({ pre: 'v2.', from: '8', to: '9', sub: 'What’s new since 2.8', T });
const mods = R.makeCards({ start: T.mods, end: T.outro, kick: 'MODULES', parts: [{ s: 'Also in' }, { s: '2.9.', fill: 'brand' }], list: MODS, toOutro: true });
const outro = R.makeOutro({ version: '2.9', T });
const { frame, setSlow } = R.makeReel({ DUR, T, SCENES, label: '/  RELEASE v2.9', draw: [intro, sMcp, sPlugins, sUnity, mods, outro], seed: 9 });

module.exports = { frame, setSlow, SCENES, T, DUR, CHAT, TOOL_RUN, TYPE, DONE, SKILLS, GET, TOGGLE, LANES, LANE_SIM, LANE_PLAY };
