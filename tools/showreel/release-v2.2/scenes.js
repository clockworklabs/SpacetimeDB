// SpacetimeDB v2.2 release video (v2.1.0 → v2.2.0), 37 s @ 120 BPM. See DECISIONS.md for what's shown and why.
const L = require('../lib');
const { C, SANS, MONO, clamp, lerp, P, E, pulse, hexA, rr, glass, measure, text } = L;
const U = require('../ui');
const { CX, TAU, check } = U;
const R = require('../release');
const { planMarker } = R;
const { typedCommand } = R;
const { slide, panel, enter, caption, pill, code, SYN, header } = R;

const DUR = 37;
const T = { intro: 0, drop: 5.5, cli: 14.5, apps: 23.5, outro: 31, end: DUR };
const SCENES = [
  { t: T.intro, name: 'v2.2' },
  { t: T.drop, name: 'REMOVE TABLES' },
  { t: T.cli, name: 'SAFER CLI' },
  { t: T.apps, name: 'ALSO IN 2.2' },
  { t: T.outro, name: 'SPACETIMEDB' },
];


// ======================================================================
// REMOVE TABLES: clear() it, delete it from the code, republish
// ======================================================================
const DR = { steps: [0.5, 1.0, 1.5], out: 2.4, gone: 2.9, note: 4.2 };
const STEPS = [['1', [['ctx.db.', null], ['legacy_items', null], ['().', null], ['clear', SYN.fn], ['();', null]]], ['2', [['delete the table from your module', null]]], ['3', [['spacetime publish', null]]]];
const TABLES = ['players', 'scores', 'legacy_items'];

function dLane(ctx, x, y, w, h, u, now) {
  glass(ctx, x, y, w, h, 20, now ? { stroke0: hexA(C.green, 0.45) } : {});
  text(ctx, now ? 'v2.2' : 'v2.1', x + 32, y + 50, { size: 15, weight: 700, tracking: 4, fill: now ? C.green : hexA(C.white, 0.5) });
  text(ctx, now ? 'Dropped, rest untouched' : 'Manual migration', x + 102, y + 50, { size: 22, weight: 700, fam: SANS, fill: hexA(C.white, now ? 0.95 : 0.7) });
  text(ctx, 'my-game', x + 32, y + 104, { size: 18, weight: 700 });
  TABLES.forEach((n, i) => {
    const legacy = i === 2;
    const ga = legacy && now ? 1 - E.inCubic(P(u, DR.gone, DR.gone + 0.5)) : 1;
    if (ga <= 0) return;
    ctx.save(); ctx.globalAlpha *= ga; ctx.translate(legacy && now ? (1 - ga) * 40 : 0, 0);
    const ry = y + 124 + i * 50;
    rr(ctx, x + 24, ry, w - 48, 42, 8); ctx.fillStyle = hexA(C.white, 0.04); ctx.fill();
    text(ctx, n, x + 48, ry + 28, { size: 19, fill: hexA(C.white, 0.85) });
    text(ctx, legacy ? '0 rows' : i === 0 ? '1,204 rows' : '8,730 rows', x + w - 48, ry + 28, { size: 17, align: 'right', fill: hexA(C.white, 0.45) });
    ctx.restore();
  });
  const oa = E.outCubic(P(u, DR.out, DR.out + 0.3));
  const oy = y + 310;
  if (oa > 0) {
    if (now) { planMarker(ctx, x + 34, oy, C.green, oa); text(ctx, '  Removed table: legacy_items', x + 32, oy, { size: 18, fill: C.green, alpha: oa }); }
    else {
      text(ctx, 'Removing the table legacy_items', x + 32, oy, { size: 18, fill: C.red, alpha: oa });
      text(ctx, 'requires a manual migration', x + 32, oy + 30, { size: 18, fill: C.red, alpha: oa });
    }
  }
  const na = E.outCubic(P(u, DR.note, DR.note + 0.3));
  if (na > 0) text(ctx, now ? 'Only empty tables are dropped.' : 'Usual way out: --delete-data wipes every table.', x + 32, y + h - 32, { size: 19, weight: 500, fam: SANS, fill: hexA(C.white, 0.65), alpha: na });
}

function sDrop(ctx, t) {
  const u = t - T.drop, len = T.cli - T.drop;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'MIGRATIONS', [{ s: 'Remove tables without' }, { s: 'wiping the database.', fill: 'brand' }]);
  // the three steps
  let x = 140;
  STEPS.forEach(([n, parts], i) => {
    const a = E.outExpo(clamp((u - DR.steps[i]) / 0.35));
    const w = parts.reduce((s, [p]) => s + measure(ctx, p, 500, 20, MONO), 0) + 96;
    if (a > 0) {
      ctx.save(); ctx.globalAlpha *= a; ctx.translate(0, (1 - a) * 16);
      rr(ctx, x, 262, w, 60, 14); ctx.fillStyle = '#121A1F'; ctx.fill();
      ctx.strokeStyle = hexA(C.white, 0.14); ctx.lineWidth = 1.5; ctx.stroke();
      ctx.fillStyle = hexA(C.green, 0.9); ctx.beginPath(); ctx.arc(x + 30, 292, 14, 0, TAU); ctx.fill();
      text(ctx, n, x + 30, 298, { size: 16, weight: 800, fam: SANS, align: 'center', fill: '#0B1114' });
      code(ctx, parts, x + 58, 299, 20);
      ctx.restore();
    }
    x += w + 24;
  });
  ctx.save(); ctx.globalAlpha *= 0.85; enter(ctx, u, 0.5, 40); dLane(ctx, 140, 360, 800, 500, u, false); ctx.restore();
  ctx.save(); enter(ctx, u, 0.65, 40); dLane(ctx, 980, 360, 800, 500, u, true); ctx.restore();
  caption(ctx, 'Empty a table with clear(), remove it from your code, and republish. No full wipe.', u, 5.0, 950);
  ctx.restore();
}

// ======================================================================
// SAFER CLI: names in list, a confirmation before delete, precise --yes
// ======================================================================
const CL = { list: [0.5, 1.0], rows: 1.3, del: [2.1, 2.8], ask: 3.1, enter: 4.0, abort: 4.2, ci: [4.9, 5.5], ciNote: 5.8 };
const ASK = 'Are you sure you want to delete database my-game-prod (c200e1…4b7a)? This action cannot be undone. [y/N]';

function sCli(ctx, t) {
  const u = t - T.cli, len = T.apps - T.cli;
  if (u < -0.1 || u > len) return;
  ctx.save();
  slide(ctx, u, len);
  header(ctx, u, 'CLI', [{ s: 'The CLI asks before' }, { s: 'deleting a database.', fill: 'brand' }]);
  const Tm = { x: 140, y: 270, w: 1640, h: 610 };
  ctx.save();
  enter(ctx, u, 0.25, 40);
  panel(ctx, Tm.x, Tm.y, Tm.w, Tm.h, 'terminal');
  const cmd = (s, c0, c1, y) => {
    if (u < c0 - 0.05) return;
    typedCommand(ctx, s, Tm.x + 36, y, u, c0, c1);
  };
  cmd('spacetime list', CL.list[0], CL.list[1], Tm.y + 112);
  if (u > CL.rows) {
    const a = E.outCubic(P(u, CL.rows, CL.rows + 0.3));
    text(ctx, 'Database Name(s)', Tm.x + 62, Tm.y + 152, { size: 18, weight: 700, fill: hexA(C.white, 0.6), alpha: a });
    text(ctx, 'Identity', Tm.x + 420, Tm.y + 152, { size: 18, weight: 700, fill: hexA(C.white, 0.6), alpha: a });
    [['my-game-prod', 'c200e1…4b7a'], ['my-game-dev', 'c2009f…12cd']].forEach(([n, id], i) => {
      const ra = E.outCubic(P(u, CL.rows + 0.1 + i * 0.1, CL.rows + 0.4 + i * 0.1));
      text(ctx, n, Tm.x + 62, Tm.y + 186 + i * 32, { size: 18, weight: 600, alpha: ra });
      text(ctx, id, Tm.x + 420, Tm.y + 186 + i * 32, { size: 18, fill: hexA(C.white, 0.6), alpha: ra });
    });
    const na = E.outCubic(P(u, CL.rows + 0.5, CL.rows + 0.8));
    text(ctx, '← names, not just hex identities', Tm.x + 760, Tm.y + 186, { size: 18, weight: 500, fam: SANS, fill: C.green, alpha: na });
  }
  cmd('spacetime delete my-game-prod', CL.del[0], CL.del[1], Tm.y + 290);
  if (u > CL.ask) {
    const a = E.outCubic(P(u, CL.ask, CL.ask + 0.3));
    R.fitText(ctx, ASK, Tm.x + 62, Tm.y + 330, Tm.w - 110, { size: 18, fam: MONO, weight: 500, fill: C.white, alpha: a });
    if (u < CL.enter && Math.floor(u * 4) % 2 === 0) { ctx.fillStyle = C.white; ctx.fillRect(Tm.x + 62, Tm.y + 346, 11, 22); }
  }
  if (u > CL.abort) {
    const a = E.outCubic(P(u, CL.abort, CL.abort + 0.25));
    text(ctx, 'Aborting', Tm.x + 62, Tm.y + 366, { size: 18, weight: 700, fill: hexA(C.white, 0.85), alpha: a });
    text(ctx, '← Enter means no', Tm.x + 200, Tm.y + 366, { size: 18, weight: 500, fam: SANS, fill: C.green, alpha: a });
  }
  // CI
  ctx.fillStyle = hexA(C.white, 0.07); ctx.fillRect(Tm.x, Tm.y + 420, Tm.w, 1);
  if (u > CL.ci[0] - 0.05) text(ctx, 'IN CI', Tm.x + 36, Tm.y + 470, { size: 14, weight: 700, tracking: 4, fill: hexA(C.white, 0.45) });
  if (u > CL.ci[0] - 0.05) {
    const s = 'spacetime publish --yes=migrate,break-clients my-game';
    typedCommand(ctx, s, Tm.x + 36, Tm.y + 520, u, CL.ci[0], CL.ci[1]);
  }
  if (u > CL.ciNote) text(ctx, 'Skip only the prompts you name.', Tm.x + 62, Tm.y + 562, { size: 19, weight: 500, fam: SANS, fill: C.green, alpha: E.outCubic(P(u, CL.ciNote, CL.ciNote + 0.3)) });
  ctx.restore();
  ctx.restore();
}

// ======================================================================
// CARDS, then the outro
// ======================================================================
const APPS = [
  { k: 'TYPESCRIPT SDK', title: ['Fewer frames', 'on the wire'], d: ['Calls made in the same tick', 'travel in one message.'] },
  { k: 'REACT', title: ['useProcedure', 'and pausable tables'], d: ['A typed procedure hook, and', 'useTable({ enabled }) to pause.'] },
  { k: 'TEMPLATES', title: ['Astro', 'starter'], d: ['An Astro site with a live', 'React island: astro-ts.'] },
  { k: 'WINDOWS', title: ['Signed', 'binaries'], d: ['Windows downloads are signed', 'by Clockwork Laboratories.'] },
];

const intro = R.makeIntro({ pre: 'v2.', from: '1', to: '2', sub: 'What’s new since 2.1', T });
const apps = R.makeCards({ start: T.apps, end: T.outro, kick: 'APPS AND TOOLING', parts: [{ s: 'Also in' }, { s: '2.2.', fill: 'brand' }], list: APPS, toOutro: true });
const outro = R.makeOutro({ version: '2.2', T });
const { frame, setSlow } = R.makeReel({ DUR, T, SCENES, label: '/  RELEASE v2.2', draw: [intro, sDrop, sCli, apps, outro], seed: 2 });

module.exports = { frame, setSlow, DUR, T, SCENES, DR, CL };
