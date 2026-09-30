// Renders the library gallery: one still per kit component (with sample parameters) and one per scene example
// from the existing reels, into kit/gallery/<name>.png (960×540, gitignored). Read them with kit/CATALOG.md.
//   node --expose-gc kit/gallery.js            → everything
//   node --expose-gc kit/gallery.js chat cards → only those entries
const fs = require('fs');
const path = require('path');
const L = require('../lib');
const { W, H, C, hexA, loadAssets, createCanvas } = L;
const K = require('./index');

const OUT = path.join(__dirname, 'gallery');
const only = process.argv.slice(2);
const gc = global.gc || (() => {});

// Component demos: draw(ctx) on a plain background. Times are chosen so animations have settled.
const T0 = { intro: 0, outro: 30, a: 10, b: 20 };
const DEMOS = {
  'header': ctx => K.header(ctx, 5, 'NEW · FEATURE NAME', [{ s: 'Say what shipped' }, { s: 'in plain words.', fill: 'brand' }]),
  'layout-panel-code-pill-caption': ctx => {
    K.header(ctx, 5, 'LAYOUT', [{ s: 'panel, code,' }, { s: 'pill, caption.', fill: 'brand' }]);
    K.panel(ctx, 140, 270, 900, 330, 'src/lib.rs');
    K.code(ctx, [['#[spacetimedb::reducer]', K.SYN.attr]], 176, 380, 22);
    K.code(ctx, [['pub fn ', K.SYN.kw], ['add', K.SYN.fn], ['(ctx: &', null], ['ReducerContext', K.SYN.type], [', name: ', null], ['String', K.SYN.type], [') { … }', null]], 176, 420, 22);
    K.pill(ctx, 1100, 330, 'a pill', { size: 19, h: 46 });
    K.pill(ctx, 1100, 400, 'highlighted pill', { size: 19, h: 46, stroke: hexA(C.green, 0.8) });
    K.caption(ctx, 'caption(): one plain sentence under the scene.', 5, 0, 800);
  },
  'chat': (ctx, t) => {
    const chat = [
      { u: 1.0, kind: 'user', s: 'Which databases do I have?' },
      { u: 2.0, kind: 'tool', s: 'list_databases', d: '{}' },
      { u: 2.8, kind: 'agent', s: 'Two: chat-app and my-game.' },
    ];
    K.agentChat(ctx, 5, t, { x: 140, y: 270, w: 1000, h: 400, title: 'agentChat()', chat, ys: [370, 455, 525] });
    L.glass(ctx, 1190, 270, 590, 400, 20);
    K.linkPulses(ctx, 2.2, { x0: 1140, x1: 1190, y: 470, calls: [2.0] });
  },
  'terminal': (ctx, t) => {
    K.panel(ctx, 140, 270, 1200, 360, 'typedCommand() · resultLine() · planMarker()');
    K.typedCommand(ctx, 'spacetime publish my-game', 172, 380, 5, 0, 1);
    K.resultLine(ctx, 'published', 176, 420, 1);
    K.typedCommand(ctx, 'spacetime delete my-game', 172, 480, 5, 0, 1);
    K.resultLine(ctx, '403 Forbidden', 176, 520, 1, { ok: false, weight: 600 });
    K.planMarker(ctx, 176, 580, C.green);
    L.text(ctx, '  Removed table: legacy_items', 172, 580, { size: 18, fill: C.green });
  },
  'compare': ctx => {
    K.header(ctx, 5, 'BEFORE / AFTER', [{ s: 'laneFrame() and' }, { s: 'beforeAfter().', fill: 'brand' }]);
    K.beforeAfter(ctx, 5, (c, x, y, w, h, now) => K.laneFrame(c, x, y, w, h, { now, version: now ? 'v2.9' : 'v2.8', title: now ? 'The new behavior' : 'The old limitation' }),
      { old: [140, 300, 800, 400], now: [980, 300, 800, 400] });
  },
  'widgets': (ctx, t) => {
    K.commandChip(ctx, 'spacetime dev --template solid-ts', 960, 300, 5, null, null);
    K.statusPill(ctx, 900, 500, 'online', t);
    K.statusPill(ctx, 1300, 500, 'lost', t);
    K.statusPill(ctx, 1700, 500, 'retry', t);
  },
  'cards': (ctx, t) => K.makeCards({ start: 0, end: 20, kick: 'MORE FEATURES', parts: [{ s: 'Also in' }, { s: '2.X.', fill: 'brand' }], list: [
    { k: 'CLI', title: ['A feature', 'in two lines'], d: ['One sentence of what users', 'can now do.'] },
    { k: 'C# MODULES', title: ['Another', 'feature'], d: ['Cards hold 1 to 5 items;', 'text shrinks to fit.'] },
    { k: 'TYPESCRIPT SDK', title: ['A third', 'one'], d: ['Each card: kicker, title,', 'two-line description.'] },
  ] })(ctx, t),
  'intro': (ctx, t) => K.makeIntro({ pre: 'v2.', from: '8', to: '9', sub: 'What’s new since 2.8', T: T0 })(ctx, t),
  'outro': (ctx, t) => K.makeOutro({ version: '2.9', T: T0 })(ctx, t),
};
const DEMO_T = { intro: 4.2, outro: 34, cards: 5, chat: 5, terminal: 5, widgets: 5 };

// Scene examples in existing reels: [reel, time]. The catalog lists the function to copy for each.
const EXAMPLES = {
  'ex-mcp-local': ['release-v2.9', 13.5], 'ex-plugins': ['release-v2.9', 21], 'ex-unity-playmode': ['release-v2.9', 29.5],
  'ex-mcp-maincloud': ['release-v2.10', 12.5], 'ex-mcp-endpoint': ['release-v2.7', 13], 'ex-lock': ['release-v2.7', 21],
  'ex-unique-migration': ['release-v2.7', 30], 'ex-feature-cards-5': ['release-v2.7', 38], 'ex-submodules': ['release-v2.8', 11],
  'ex-tab-reconnect': ['release-v2.8', 21], 'ex-react-reconnect': ['release-v2.6', 12], 'ex-cpp-query': ['release-v2.6', 21],
  'ex-procedures-flag': ['release-v2.5', 12], 'ex-solid-sync': ['release-v2.5', 20], 'ex-view-pk': ['release-v2.5', 29.5],
  'ex-http-routes': ['release-v2.4', 12], 'ex-templates-3': ['release-v2.4', 21], 'ex-godot': ['release-v2.3', 12],
  'ex-pipelining': ['release-v2.3', 20], 'ex-remove-tables': ['release-v2.2', 12], 'ex-safer-cli': ['release-v2.2', 21],
  'ex-rust-browser': ['release-v2.1', 12], 'ex-http-timeouts': ['release-v2.1', 21], 'ex-unreal-events': ['release-v2.1', 30],
  'ex-event-tables': ['release-v2.0', 13], 'ex-await-reducers': ['release-v2.0', 22], 'ex-spacetime-json': ['release-v2.0', 31],
  'ex-logo-grid': ['release-v2.0', 39.5],
};

(async () => {
  await loadAssets();
  fs.mkdirSync(OUT, { recursive: true });
  const canvas = createCanvas(W, H), ctx = canvas.getContext('2d');
  const small = createCanvas(W / 2, H / 2), sctx = small.getContext('2d');
  const save = name => {
    sctx.clearRect(0, 0, W / 2, H / 2);
    sctx.drawImage(canvas, 0, 0, W / 2, H / 2);
    fs.writeFileSync(path.join(OUT, name + '.png'), small.toBuffer('image/png'));
    gc();
  };
  for (const [name, draw] of Object.entries(DEMOS)) {
    if (only.length && !only.includes(name)) continue;
    ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.globalAlpha = 1;
    ctx.fillStyle = C.bg; ctx.fillRect(0, 0, W, H);
    draw(ctx, DEMO_T[name] ?? 5);
    save(name);
  }
  for (const [name, [reel, t]] of Object.entries(EXAMPLES)) {
    if (only.length && !only.includes(name)) continue;
    const SC = require(path.join(__dirname, '..', reel, 'scenes'));
    SC.frame(ctx, t, t);
    save(name);
  }
  console.log('wrote', OUT);
})();
