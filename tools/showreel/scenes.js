// SpacetimeDB showreel v2 — ~90s @ 120 BPM (beat = 0.5s), paced for reading.
const L = require('./lib');
const { W, H, DUR, C, SANS, MONO, clamp, lerp, P, E, spring, pulse, rng, hash, hexA, rr, glass, brandGrad, font, riseText, measure, text, scramble, drawMark, A, createCanvas } = L;

const U = require('./ui');
const { CX, CY, TAU, riseParts, kicker, check, spinner, trafficLights, chip, termRows, starPoint } = U;

// Scene start times
const T = { hook: 0, stack: 5, code: 15, rt: 27, cloud: 36, feat: 43, speed: 49.5, langs: 57, ai: 63, scale: 72.5, outro: 84.5, end: DUR };

const SCENES = [
  { t: T.hook, name: 'THE IDEA' },
  { t: T.stack, name: 'ONE SYSTEM' },
  { t: T.code, name: 'MODULES' },
  { t: T.rt, name: 'REAL-TIME SYNC' },
  { t: T.cloud, name: 'MAINCLOUD' },
  { t: T.feat, name: 'BUILT IN' },
  { t: T.speed, name: 'PERFORMANCE' },
  { t: T.langs, name: 'EVERY LANGUAGE' },
  { t: T.ai, name: 'AI-NATIVE' },
  { t: T.scale, name: 'SCALE' },
  { t: T.outro, name: 'SPACETIMEDB' },
];

// key absolute moments
const K = {
  impact1: T.stack + 6.0,
  absorb: T.code + 5.0,
  zoom: T.rt + 9,
  bigbar: T.speed + 4.0,
  ceiling: T.scale + 2.2,
  stline: T.scale + 3.0,
  root: T.scale + 6.6,
  deploy: T.cloud + 2.1,
  outro: T.outro,
};

const FLASHES = [
  { t: K.impact1, a: 0.9, k: 6 },
  { t: K.absorb, a: 0.22, k: 9 },
  { t: K.zoom, a: 1.0, k: 4.5 },
  { t: K.bigbar, a: 0.18, k: 9 },
  { t: K.stline, a: 0.12, k: 9 },
  { t: K.deploy, a: 0.12, k: 9 },
  { t: K.outro, a: 0.95, k: 4.5 },
];
const SHAKES = [
  { t: 1.0, a: 5 }, { t: 2.0, a: 6 }, { t: 3.0, a: 14 },
  { t: K.impact1, a: 26 }, { t: K.absorb, a: 8 }, { t: K.zoom, a: 16 }, { t: K.bigbar, a: 16 },
  { t: K.ceiling, a: 7 }, { t: K.stline, a: 12 }, { t: K.root, a: 8 }, { t: K.deploy, a: 8 },
  { t: T.langs + 0.1, a: 5 }, { t: T.langs + 0.6, a: 5 }, { t: T.langs + 1.1, a: 5 }, { t: T.langs + 1.6, a: 7 },
  { t: K.outro, a: 22 },
];
const SHOCKS = [
  { t: K.impact1, x: 960, y: 440, col: C.green, r: 1500, w: 60 },
  { t: K.impact1 + 0.06, x: 960, y: 440, col: C.pink, r: 1100, w: 24 },
  { t: K.absorb, x: 1480, y: 450, col: C.purple, r: 520, w: 20 },
  { t: K.root, x: 1340, y: 600, col: C.purple, r: 600, w: 22 },
  { t: K.outro, x: 960, y: 420, col: C.green, r: 1600, w: 70 },
  { t: K.outro + 0.06, x: 960, y: 420, col: C.purple, r: 1200, w: 26 },
];
const BURSTS = [
  { t: K.impact1, x: 960, y: 440, n: 140, seed: 11 },
  { t: K.absorb, x: 1480, y: 450, n: 50, seed: 23, v: 0.5 },
  { t: K.bigbar + 0.45, x: 1800, y: 400, n: 60, seed: 41, v: 0.6, dir: 0 },
  { t: K.stline + 0.4, x: 1620, y: 395, n: 50, seed: 43, v: 0.5, dir: 0 },
  { t: K.outro, x: 960, y: 420, n: 160, seed: 77 },
];

// ======================================================================
// Background
// ======================================================================
function warpSpeed(t) {
  let s = 0.08;
  s += 1.3 * (1 - E.outCubic(P(t, 0, 3.5))) * P(t, 0, 0.3);
  s += 2.2 * pulse(t - K.impact1, 2.5);
  s += 3.5 * E.inExpo(P(t, K.zoom - 0.6, K.zoom)) * (t < K.zoom ? 1 : 0) + 3.5 * pulse(t - K.zoom, 3);
  s += 0.25 * P(t, T.speed, T.speed + 0.5) * (1 - P(t, T.speed + 1.6, T.speed + 2.1));
  s += 2.0 * pulse(t - K.outro, 2.5);
  s += 2.6 * E.inCubic(P(t, K.outro + 1.2, K.outro + 5.0));
  return s;
}
let GRID = null, drawStars = null;
function initStatic() {
  GRID = U.makeGrid();
  drawStars = U.makeStarfield(warpSpeed, DUR);
}

function drawBackground(ctx, t) {
  ctx.fillStyle = C.bg;
  ctx.fillRect(0, 0, W, H);
  const gx = -((t * 14) % 48), gy = -((t * 6) % 48);
  const ga = 1 - 0.8 * (1 - P(t, T.stack - 0.6, T.stack)) - 0.8 * P(t, K.outro - 0.3, K.outro + 0.1);
  ctx.save();
  ctx.globalAlpha = clamp(ga);
  ctx.drawImage(GRID, gx - 48, gy - 48);
  ctx.restore();
  drawStars(ctx, t);
}

// ======================================================================
// Playback speed + camera shake
// ======================================================================
// Playback slow-down factor (render.js --slow). Impacts (shake, flashes) stay snappy in real
// time; everything else plays at scene speed.
let SLOW = 1;
function setSlow(n) { SLOW = n; }

// Shake decays and oscillates in real time so a slowed-down render doesn't turn it into a wobble.
function shakeOffset(t, rt = t) {
  let a = 0;
  for (const s of SHAKES) if (t >= s.t) a += s.a * pulse((t - s.t) * SLOW, 9);
  return [a * (Math.sin(rt * 91) * 0.6 + Math.sin(rt * 143 + 1.3) * 0.4), a * (Math.cos(rt * 107) * 0.6 + Math.sin(rt * 67 + 0.4) * 0.4)];
}

// ======================================================================
// Core
// ======================================================================
function coreState(t) {
  let x = 960, y = 440, size = 360;
  const m1 = E.inOutExpo(P(t, T.code - 0.1, T.code + 0.6));
  x = lerp(x, 1480, m1); y = lerp(y, 450, m1); size = lerp(size, 280, m1);
  const m2 = E.inOutExpo(P(t, T.rt - 0.15, T.rt + 0.6));
  x = lerp(x, 960, m2); y = lerp(y, 560, m2); size = lerp(size, 210, m2);
  const app = spring(t - K.impact1, 1.6, 6.5);
  const beat = t >= T.code ? pulse((t - T.code) % 0.5, 9) : 0;
  return { x, y, size: size * (0.2 + 0.8 * app) * (1 + 0.025 * beat), sweep: E.outExpo(P(t, K.impact1, K.impact1 + 0.5)), ring: E.outExpo(P(t, K.impact1 + 0.02, K.impact1 + 0.6)), beat };
}
function drawCore(ctx, t, extra = 0) {
  if (t < K.impact1) return;
  const s = coreState(t);
  const hot = pulse(t - K.impact1, 3) + pulse(t - K.absorb, 5) * 0.8 + extra;
  ctx.save();
  ctx.translate(s.x, s.y);
  ctx.rotate(t * 0.15);
  const rr0 = s.size * 0.62;
  ctx.strokeStyle = hexA(C.white, 0.22 * s.ring);
  ctx.lineWidth = 1.2;
  for (let i = 0; i < 72; i++) {
    if (i / 72 > s.ring) break;
    const a = (i / 72) * TAU, l = i % 6 === 0 ? 12 : 5;
    ctx.beginPath(); ctx.moveTo(Math.cos(a) * rr0, Math.sin(a) * rr0); ctx.lineTo(Math.cos(a) * (rr0 + l), Math.sin(a) * (rr0 + l)); ctx.stroke();
  }
  ctx.restore();
  drawMark(ctx, s.x, s.y, s.size, { sweep: s.sweep, ring: s.ring, glow: hexA(C.green, 0.9), glowBlur: 30 + 40 * hot + 25 * s.beat, rot: Math.sin(t * 0.7) * 0.05 });
}

// ======================================================================
// 1 — HOOK
// ======================================================================
function sHook(ctx, t) {
  const u = t - T.hook;
  if (u > 5.2) return;
  const lp = E.outExpo(P(u, 0.0, 0.7)), lf = 1 - P(u, 0.5, 1.1);
  if (lf > 0) {
    ctx.save();
    ctx.globalAlpha = lf;
    ctx.shadowColor = C.white; ctx.shadowBlur = 40;
    const w = lp * 1800;
    const g = ctx.createLinearGradient(CX - w / 2, 0, CX + w / 2, 0);
    g.addColorStop(0, hexA(C.white, 0)); g.addColorStop(0.5, C.white); g.addColorStop(1, hexA(C.white, 0));
    ctx.fillStyle = g;
    ctx.fillRect(CX - w / 2, CY - 1.5, w, 3);
    ctx.restore();
  }
  const ex = E.inExpo(P(u, 4.6, 4.95));
  const zoom = 1 + 0.07 * E.inOutSine(P(u, 0, 5)) + 0.05 * pulse(u - 3.0, 8) + 0.02 * pulse(u - 2.0, 9) + 0.02 * pulse(u - 1.0, 9);
  ctx.save();
  ctx.translate(CX, CY - ex * 1400);
  ctx.scale(zoom, zoom);
  ctx.translate(-CX, -CY);
  kicker(ctx, 'WHAT IF', CX, CY - 175, u - 0.3, { size: 28, tracking: 16, align: 'center', dur: 0.6 });
  riseText(ctx, 'Your database', CX, CY + 5, { t: u - 1.0, size: 150, weight: 900, align: 'center', stagger: 0.03, dur: 0.55, fill: C.white, tracking: -2 });
  const l2 = { t: u - 2.0, size: 150, weight: 900, align: 'center', stagger: 0.03, dur: 0.55, tracking: -2 };
  const gl = u > 3.0 ? pulse(u - 3.0, 5) : 0;
  if (gl > 0.02) {
    ctx.save();
    ctx.globalCompositeOperation = 'lighter';
    ctx.globalAlpha = 0.8 * gl;
    riseText(ctx, 'was the server?', CX - 22 * gl, CY + 165, { ...l2, fill: C.pink });
    riseText(ctx, 'was the server?', CX + 22 * gl, CY + 165, { ...l2, fill: C.cyan });
    ctx.restore();
  }
  const slices = 7;
  for (let i = 0; i < slices; i++) {
    const y0 = CY + 165 - 150 + (i * 190) / slices;
    const off = gl > 0.02 ? (hash(Math.floor(u * 24) * 13 + i) - 0.5) * 90 * gl : 0;
    ctx.save();
    ctx.beginPath(); ctx.rect(0, y0, W, 190 / slices + 0.5); ctx.clip();
    riseText(ctx, 'was the server?', CX + off, CY + 165, { ...l2, fill: (sx, w) => brandGrad(ctx, sx, 0, sx + w, 0, 0.1 * Math.sin(u * 3)), glow: hexA(C.purple, 0.5), glowBlur: 40 });
    ctx.restore();
  }
  ctx.restore();
}

// ======================================================================
// 2 — THE USUAL STACK → ONE SYSTEM
// ======================================================================
const STACK = [
  { id: 'client', label: 'CLIENT', sub: 'browser · mobile · game', x: 300, y: 565 },
  { id: 'lb', label: 'LOAD BALANCER', sub: 'ingress · TLS', x: 640, y: 385 },
  { id: 'ws', label: 'WEBSOCKET GATEWAY', sub: 'fan-out · presence', x: 640, y: 745 },
  { id: 'api', label: 'API SERVER', sub: 'REST · GraphQL', x: 980, y: 305 },
  { id: 'queue', label: 'MESSAGE QUEUE', sub: 'pub/sub', x: 980, y: 565 },
  { id: 'auth', label: 'AUTH SERVICE', sub: 'sessions · tokens', x: 980, y: 825 },
  { id: 'cache', label: 'CACHE', sub: 'in-memory kv', x: 1320, y: 305 },
  { id: 'db', label: 'DATABASE', sub: 'SQL', x: 1320, y: 565 },
  { id: 'workers', label: 'WORKERS', sub: 'cron · jobs', x: 1320, y: 825 },
];
const SI = Object.fromEntries(STACK.map((n, i) => [n.id, i]));
const EDGES = [['client', 'lb'], ['client', 'ws'], ['lb', 'api'], ['ws', 'queue'], ['ws', 'auth'], ['api', 'cache'], ['api', 'db'], ['api', 'queue'], ['api', 'auth'], ['queue', 'db'], ['queue', 'workers'], ['workers', 'db'], ['cache', 'db'], ['auth', 'db']];
const BADGES = [{ n: 'api', s: 'TIMEOUT', u: 3.4 }, { n: 'cache', s: 'CACHE MISS', u: 3.7 }, { n: 'lb', s: '504', u: 3.95 }, { n: 'queue', s: 'RETRY 3/5', u: 4.2 }, { n: 'ws', s: 'DESYNC', u: 4.4 }];
const boxU = i => 0.5 + i * 0.3;

function stackPos(u, i) {
  const n = STACK[i];
  const c = E.inExpo(P(u, 5.4, 6.0));
  const jit = P(u, 3.4, 5.4) * 7 * (1 - c);
  const b = Math.floor(u * 20);
  const x = n.x + (hash(b * 17 + i) - 0.5) * jit, y = n.y + (hash(b * 29 + i + 5) - 0.5) * jit;
  const dx = x - 960, dy = y - 440;
  const ang = E.inCubic(c) * 1.6, sc = 1 - c;
  const rx = dx * Math.cos(ang) - dy * Math.sin(ang), ry = dx * Math.sin(ang) + dy * Math.cos(ang);
  return { x: 960 + rx * sc, y: 440 + ry * sc, s: lerp(1, 0.05, E.inCubic(c)), c };
}

function sStack(ctx, t) {
  const u = t - T.stack;
  if (u < -0.1 || u > 10.2) return;
  const enter = E.outExpo(P(u, -0.05, 0.35));
  ctx.save();
  ctx.translate(0, (1 - enter) * 900);
  const zoom = 1.06 - 0.06 * E.outCubic(P(u, 0, 5.4));
  ctx.translate(CX, CY); ctx.scale(zoom, zoom); ctx.translate(-CX, -CY);
  const c = E.inExpo(P(u, 5.4, 6.0));

  if (u < 6.05) {
    const tf = 1 - P(u, 5.2, 5.5);
    ctx.save(); ctx.globalAlpha = tf;
    kicker(ctx, '// THE USUAL STACK', 110, 150, u - 0.1);
    riseText(ctx, 'Your backend, today.', 108, 222, { t: u - 0.2, size: 64, weight: 800, stagger: 0.02, dur: 0.45, tracking: -1 });
    ctx.restore();

    const pos = STACK.map((_, i) => stackPos(u, i));
    ctx.save();
    EDGES.forEach(([a, b], k) => {
      const ia = SI[a], ib = SI[b];
      const u0 = Math.max(boxU(ia), boxU(ib)) + 0.1;
      const p = E.outCubic(P(u, u0, u0 + 0.35));
      if (p <= 0) return;
      const A0 = pos[ia], B0 = pos[ib];
      const horiz = Math.abs(A0.x - B0.x) > 40;
      const cp = horiz ? [(A0.x + B0.x) / 2, A0.y, (A0.x + B0.x) / 2, B0.y] : [A0.x + 90 * A0.s, A0.y, B0.x + 90 * B0.s, B0.y];
      const bez = q => { const m = 1 - q; return [m * m * m * A0.x + 3 * m * m * q * cp[0] + 3 * m * q * q * cp[2] + q * q * q * B0.x, m * m * m * A0.y + 3 * m * m * q * cp[1] + 3 * m * q * q * cp[3] + q * q * q * B0.y]; };
      ctx.strokeStyle = hexA(C.white, 0.3 + c * 0.3);
      ctx.lineWidth = 1.6;
      ctx.setLineDash([6, 7]);
      ctx.lineDashOffset = -u * 40;
      ctx.beginPath();
      const N = 28;
      for (let j = 0; j <= N * p; j++) { const q = bez(j / N); j === 0 ? ctx.moveTo(q[0], q[1]) : ctx.lineTo(q[0], q[1]); }
      ctx.stroke();
      ctx.setLineDash([]);
      if (p >= 1) {
        for (let m = 0; m < 2; m++) {
          const q = ((u - u0) * (0.7 + hash(k) * 0.6) + m * 0.5 + hash(k + 9)) % 1;
          const pt = bez(q);
          ctx.fillStyle = hexA(C.white, m ? 0.55 : 0.9);
          ctx.shadowColor = ctx.fillStyle; ctx.shadowBlur = 12;
          ctx.beginPath(); ctx.arc(pt[0], pt[1], 3.2, 0, TAU); ctx.fill();
          ctx.shadowBlur = 0;
        }
      }
    });
    ctx.restore();

    STACK.forEach((n, i) => {
      const lt = u - boxU(i);
      if (lt < 0) return;
      const ps = pos[i];
      const sc = (0.55 + 0.45 * spring(lt, 1.8, 7)) * ps.s;
      const bw = 262, bh = 92;
      ctx.save();
      ctx.translate(ps.x, ps.y);
      ctx.scale(sc, sc);
      ctx.globalAlpha = clamp(lt * 8);
      glass(ctx, -bw / 2, -bh / 2, bw, bh, 14, { shadow: false });
      const fl = pulse(lt, 7);
      if (fl > 0.02) { rr(ctx, -bw / 2, -bh / 2, bw, bh, 14); ctx.strokeStyle = hexA(C.white, fl * 0.8); ctx.lineWidth = 2.5; ctx.stroke(); }
      const err = BADGES.find(b => b.n === n.id && u > b.u);
      const dotc = err ? C.red : hexA(C.white, 0.45);
      ctx.fillStyle = dotc; ctx.shadowColor = dotc; ctx.shadowBlur = 10;
      ctx.beginPath(); ctx.arc(-bw / 2 + 24, -10, 5, 0, TAU); ctx.fill(); ctx.shadowBlur = 0;
      text(ctx, n.label, -bw / 2 + 40, -3, { size: 17, weight: 700, tracking: 2 });
      text(ctx, n.sub, -bw / 2 + 40, 24, { size: 15, weight: 400, fill: hexA(C.white, 0.55) });
      ctx.restore();
    });
    for (const b of BADGES) {
      const lt = u - b.u;
      if (lt < 0) continue;
      const ps = pos[SI[b.n]];
      const s = E.outBack(clamp(lt / 0.25), 3) * ps.s;
      ctx.save();
      ctx.translate(ps.x + 95 * ps.s, ps.y - 58 * ps.s);
      ctx.scale(s, s);
      const w = measure(ctx, b.s, 700, 14, MONO, 1.5) + 24;
      rr(ctx, -w / 2, -14, w, 28, 14);
      ctx.fillStyle = C.red; ctx.shadowColor = C.red; ctx.shadowBlur = 20; ctx.fill(); ctx.shadowBlur = 0;
      text(ctx, b.s, 0, 5, { size: 14, weight: 700, tracking: 1.5, fill: '#1a0006', align: 'center' });
      ctx.restore();
    }
    const sa = E.outExpo(P(u, 0.5, 0.9)) * (1 - P(u, 5.2, 5.5));
    if (sa > 0) {
      ctx.save();
      ctx.globalAlpha = sa;
      ctx.translate((1 - sa) * 60, 0);
      const svc = STACK.filter((_, i) => u > boxU(i)).length;
      const hops = EDGES.filter(([a, b]) => u > Math.max(boxU(SI[a]), boxU(SI[b])) + 0.1).length;
      const glue = Math.floor(Math.pow(P(u, 0.5, 4.6), 2) * 48213);
      const rows = [['SERVICES', String(svc), C.white], ['NETWORK HOPS', String(hops), C.white], ['LINES OF GLUE', u > 4.6 ? '∞' : glue.toLocaleString('en-US'), C.red]];
      rows.forEach(([k, v, col], i) => {
        const y = 330 + i * 170;
        ctx.fillStyle = hexA(C.white, 0.15); ctx.fillRect(1560, y - 70, 280, 1);
        text(ctx, k, 1560, y - 38, { size: 15, weight: 600, tracking: 4, fill: hexA(C.white, 0.55) });
        text(ctx, v, 1558, y + 36, { size: 64, weight: 800, fam: SANS, fill: col });
      });
      ctx.restore();
    }
  }

  if (u >= 6.0) {
    drawCore(ctx, t);
    const out = u > 9.4 ? u - 9.4 : undefined;
    riseParts(ctx, [{ s: 'One' }, { s: 'system.', fill: 'brand' }], CX, 790, { t: u - 6.15, size: 136, weight: 800, align: 'center', stagger: 0.035, dur: 0.55, letter: -3, outT: out, outDir: -1 });
    const chips = ['DATABASE', 'SERVER LOGIC', 'REAL-TIME SYNC', 'AUTH', 'HOSTING'];
    const cw = chips.map(s => measure(ctx, s, 600, 17, MONO, 3) + 44);
    const tot = cw.reduce((a, b) => a + b, 0) + 16 * (chips.length - 1);
    let x = CX - tot / 2;
    chips.forEach((s, i) => {
      const lt = u - (6.8 + i * 0.12);
      const o = out != null ? E.inExpo(clamp((out - i * 0.02) / 0.3)) : 0;
      if (lt > 0 && o < 1) {
        const a = E.outExpo(clamp(lt / 0.45));
        ctx.save();
        ctx.globalAlpha = a * (1 - o);
        ctx.translate(0, (1 - a) * 30 + o * 40);
        rr(ctx, x, 845, cw[i], 44, 22);
        ctx.fillStyle = 'rgba(255,255,255,0.04)'; ctx.fill();
        ctx.strokeStyle = hexA(C.white, 0.25); ctx.lineWidth = 1.5; ctx.stroke();
        text(ctx, s, x + cw[i] / 2, 873, { size: 17, weight: 600, tracking: 3, align: 'center' });
        ctx.restore();
      }
      x += cw[i] + 16;
    });
    text(ctx, 'One system replaces your entire stack.', CX, 955, { size: 24, weight: 500, fam: SANS, align: 'center', fill: hexA(C.white, 0.75), alpha: E.outCubic(P(u, 7.6, 8.1)) * (1 - P(u, 9.4, 9.7)) });
  }
  ctx.restore();
}

// ======================================================================
// 3 — MODULES (server code → publish → generate → client code)
// ======================================================================
const CODE_S = [
  '#[spacetimedb::table(accessor = player, public)]',
  'pub struct Player {',
  '    #[primary_key]',
  '    id: Identity,',
  '    x: f32,',
  '    y: f32,',
  '}',
  '',
  '#[spacetimedb::reducer]',
  'pub fn move_to(ctx: &ReducerContext, x: f32, y: f32) {',
  '    let mut p = ctx.db.player().id().find(ctx.sender()).unwrap();',
  '    p.x = x; p.y = y;',
  '    ctx.db.player().id().update(p);',
  '}',
];
const CODE_C = [
  "import { DbConnection, tables } from './module_bindings';",
  '',
  'const conn = DbConnection.builder()',
  "  .withUri('wss://maincloud.spacetimedb.com')",
  "  .withDatabaseName('my-game')",
  '  .onConnect((ctx) => {',
  '    ctx.subscriptionBuilder().subscribe([tables.player]);',
  '  })',
  '  .build();',
  '',
  'conn.db.player.onUpdate((ctx, old, p) => draw(p));',
  'conn.reducers.moveTo({ x: 10, y: 20 });',
];
// Syntax highlighting keeps its own palette (not subject to the accent rules).
const SYN = { attr: '#a880ff', str: '#6fe3ff', kw: '#ff80fb', type: '#fbdc8e', fn: '#4cf490' };
function tokenize(line) {
  const re = /(#\[[^\]]*\])|('[^']*')|\b(pub|struct|fn|let|mut|import|from|const)\b|\b(Identity|f32|ReducerContext|Player|DbConnection|tables)\b|([A-Za-z_][A-Za-z0-9_]*)(?=\()|(\s+)|([A-Za-z_][A-Za-z0-9_]*)|(.)/g;
  const out = [];
  let m;
  while ((m = re.exec(line))) {
    let col = hexA(C.white, 0.88);
    if (m[1]) col = SYN.attr;
    else if (m[2]) col = SYN.str;
    else if (m[3]) col = SYN.kw;
    else if (m[4]) col = SYN.type;
    else if (m[5]) col = SYN.fn;
    else if (m[8]) col = hexA(C.white, 0.55);
    out.push({ s: m[0], col });
  }
  return out;
}
const TOK_S = CODE_S.map(tokenize), TOK_C = CODE_C.map(tokenize);

function drawCodeLines(ctx, lines, toks, ed, u, start, step, cps, alpha, dy) {
  font(ctx, 400, 21, MONO);
  const cw = ctx.measureText('M').width;
  let cursor = null;
  ctx.save();
  ctx.globalAlpha *= alpha;
  lines.forEach((line, i) => {
    const y = ed.y + 108 + i * 36 + dy;
    const ls = start + i * step, ld = Math.max(0.06, line.length / cps);
    const lt = u - ls;
    if (lt < 0) return;
    text(ctx, String(i + 1).padStart(2, ' '), ed.x + 26, y, { size: 17, weight: 400, fill: hexA(C.white, 0.25) });
    const n = Math.floor(clamp(lt / ld) * line.length);
    let k = 0, x = ed.x + 76;
    for (const tok of toks[i]) {
      if (k >= n) break;
      const s = tok.s.slice(0, n - k);
      font(ctx, 400, 21, MONO);
      ctx.fillStyle = tok.col;
      ctx.fillText(s, x, y);
      x += s.length * cw;
      k += tok.s.length;
    }
    if (lt < ld + 0.15 || i === lines.length - 1) cursor = { x: ed.x + 76 + n * cw, y };
  });
  ctx.restore();
  return cursor;
}

function sCode(ctx, t) {
  const u = t - T.code;
  if (u < -0.1 || u > 12.2) return;
  const ex = E.inExpo(P(u, 11.6, 12.0));
  const ed = { x: 100, y: 180, w: 1040, h: 610 };
  const inE = E.outExpo(P(u, -0.05, 0.55));
  const sw = E.inOutCubic(P(u, 6.85, 7.15));
  ctx.save();
  ctx.translate((1 - inE) * -1300 - ex * 1500, 0);
  kicker(ctx, 'RUST · C# · TYPESCRIPT · C++', ed.x + 4, ed.y - 34, u - 0.1, { size: 16, fill: hexA(C.white, 0.6) });
  glass(ctx, ed.x, ed.y, ed.w, ed.h, 20);
  trafficLights(ctx, ed.x, ed.y);
  const tab1 = { x: ed.x + 110, w: 150 }, tab2w = 170 * E.outExpo(P(u, 6.6, 6.95));
  rr(ctx, lerp(tab1.x, tab1.x + 160, sw), ed.y + 14, lerp(tab1.w, 170, sw), 34, 8); ctx.fillStyle = 'rgba(255,255,255,0.08)'; ctx.fill();
  text(ctx, 'src/lib.rs', tab1.x + 18, ed.y + 36, { size: 15, weight: 500, fill: hexA(C.white, lerp(0.9, 0.45, sw)) });
  if (tab2w > 1) {
    ctx.save(); ctx.beginPath(); ctx.rect(tab1.x + 160, ed.y + 10, tab2w, 44); ctx.clip();
    text(ctx, 'src/client.ts', tab1.x + 178, ed.y + 36, { size: 15, weight: 500, fill: hexA(C.white, lerp(0.45, 0.9, sw)) });
    ctx.restore();
  }
  text(ctx, u < 7 ? 'SERVER MODULE' : 'CLIENT', ed.x + ed.w - 26, ed.y + 36, { size: 13, weight: 600, tracking: 3, fill: hexA(C.white, 0.45), align: 'right' });
  ctx.fillStyle = 'rgba(255,255,255,0.07)'; ctx.fillRect(ed.x, ed.y + 60, ed.w, 1);
  ctx.save();
  ctx.beginPath(); ctx.rect(ed.x, ed.y + 62, ed.w, ed.h - 64); ctx.clip();
  const hl = E.outExpo(P(u, 3.1, 3.6)) * (1 - sw);
  if (hl > 0) {
    ctx.fillStyle = hexA(C.white, 0.05 * hl);
    ctx.fillRect(ed.x + 1, ed.y + 84 + 8 * 36 - 6, (ed.w - 2) * hl, 6 * 36);
    ctx.fillStyle = hexA(C.green, 0.9 * hl);
    ctx.fillRect(ed.x + 1, ed.y + 84 + 8 * 36 - 6, 3, 6 * 36);
  }
  let cursor = null;
  if (sw < 1) cursor = drawCodeLines(ctx, CODE_S, TOK_S, ed, u, 0.6, 0.17, 120, 1 - sw, -sw * 80);
  if (u > 7.0) cursor = drawCodeLines(ctx, CODE_C, TOK_C, ed, u, 7.1, 0.15, 140, 1, 0);
  ctx.restore();
  if (cursor && (Math.floor(u * 4) % 2 === 0 || (u > 0.6 && u < 3.0) || (u > 7.1 && u < 9.1))) { ctx.fillStyle = C.white; ctx.fillRect(cursor.x + 1, cursor.y - 19, 11, 24); }

  const tin = E.outExpo(P(u, 0.1, 0.7));
  ctx.translate((1 - tin) * -400, 0);
  const tm = { x: 100, y: 816, w: 1040, h: 118 };
  glass(ctx, tm.x, tm.y, tm.w, tm.h, 18, { top: 'rgba(8,10,15,0.95)', bottom: 'rgba(8,10,15,0.95)' });
  ctx.save();
  ctx.beginPath(); ctx.rect(tm.x, tm.y, tm.w, tm.h); ctx.clip();
  const ph = E.inOutCubic(P(u, 5.6, 5.9));
  ctx.save(); ctx.translate(0, -ph * 110);
  termRows(ctx, tm, u, [{ cmd: 'spacetime publish my-game', c0: 3.4, c1: 3.9, res: [[4.05, 'Build finished', hexA(C.white, 0.8), 0], [4.3, 'Published → my-game', hexA(C.white, 0.8), 210]] }]);
  ctx.restore();
  ctx.save(); ctx.translate(0, (1 - ph) * 110);
  termRows(ctx, tm, u, [{ cmd: 'spacetime generate --lang typescript --out-dir src/module_bindings', c0: 5.9, c1: 6.5, res: [[6.6, 'Generated TypeScript bindings → src/module_bindings', hexA(C.white, 0.8), 0]] }]);
  ctx.restore();
  ctx.restore();
  ctx.restore();

  const oA = u > 6.3 ? u - 6.3 : undefined;
  riseText(ctx, 'Your logic runs', 1480, 740, { t: u - 1.0, size: 58, weight: 800, align: 'center', stagger: 0.02, tracking: -1, outT: oA, outDir: -1 });
  riseText(ctx, 'inside the database.', 1480, 810, { t: u - 1.2, size: 58, weight: 800, align: 'center', stagger: 0.02, tracking: -1, fill: (sx, w) => brandGrad(ctx, sx, 0, sx + w, 0), outT: oA != null ? oA - 0.05 : undefined, outDir: -1 });
  text(ctx, 'Tables + reducers. Transactional. Hot-swappable.', 1480, 870, { size: 18, weight: 500, align: 'center', fill: hexA(C.white, 0.6), alpha: E.outCubic(P(u, 1.8, 2.3)) * (1 - P(u, 6.2, 6.5)) });
  const oB = u > 11.4 ? u - 11.4 : undefined;
  riseText(ctx, 'Then subscribe', 1480, 740, { t: u - 7.0, size: 58, weight: 800, align: 'center', stagger: 0.02, tracking: -1, outT: oB, outDir: -1 });
  riseText(ctx, 'from any client.', 1480, 810, { t: u - 7.2, size: 58, weight: 800, align: 'center', stagger: 0.02, tracking: -1, fill: (sx, w) => brandGrad(ctx, sx, 0, sx + w, 0), outT: oB != null ? oB - 0.05 : undefined, outDir: -1 });
  text(ctx, 'Typed bindings, generated for you.', 1480, 870, { size: 18, weight: 500, align: 'center', fill: hexA(C.white, 0.6), alpha: E.outCubic(P(u, 7.8, 8.3)) * (1 - P(u, 11.3, 11.6)) });

  drawCore(ctx, t);

  if (u > 4.45 && u < 5.05) {
    const q = E.inCubic(P(u, 4.5, 5.0));
    const p0 = [1000, 900], p1 = [1260, 980], p2 = [1480, 450];
    const bz = s => [(1 - s) * (1 - s) * p0[0] + 2 * (1 - s) * s * p1[0] + s * s * p2[0], (1 - s) * (1 - s) * p0[1] + 2 * (1 - s) * s * p1[1] + s * s * p2[1]];
    for (let g = 5; g >= 0; g--) {
      const s = clamp(q - g * 0.04);
      const [x, y] = bz(s);
      chip(ctx, x, y, 'module.wasm', lerp(1, 0.25, s) * E.outBack(clamp((u - 4.45) / 0.15)), -0.25 * s, 'brand', g === 0 ? 1 : 0.12 * (6 - g) / 6);
    }
  }
  if (u > 6.55 && u < 7.1) {
    const q = E.inOutCubic(P(u, 6.6, 7.0));
    const p0 = [1480, 450], p1 = [1000, 60], p2 = [360, 197];
    const bz = s => [(1 - s) * (1 - s) * p0[0] + 2 * (1 - s) * s * p1[0] + s * s * p2[0], (1 - s) * (1 - s) * p0[1] + 2 * (1 - s) * s * p1[1] + s * s * p2[1]];
    for (let g = 5; g >= 0; g--) {
      const s = clamp(q - g * 0.04);
      const [x, y] = bz(s);
      chip(ctx, x, y, 'module_bindings/', lerp(0.4, 1, Math.sin(s * Math.PI)) * 0.9, 0.15 * Math.sin(s * Math.PI), C.pink, (g === 0 ? 1 : 0.12 * (6 - g) / 6) * (1 - P(u, 6.95, 7.05)));
    }
  }
}

// ======================================================================
// 4 — REAL-TIME
// ======================================================================
const ORB = { cx: 960, cy: 560, tilt: -0.16, rings: [{ rx: 270, n: 8, sp: 0.28, c: '#6f7987' }, { rx: 400, n: 12, sp: -0.17, c: '#6f7987' }, { rx: 530, n: 16, sp: 0.12, c: '#6f7987' }] };
const NODES = [];
ORB.rings.forEach((r, k) => { for (let i = 0; i < r.n; i++) NODES.push({ k, i, a0: (i / r.n) * TAU + k * 0.4 }); });
const EVENTS = (() => {
  const r = rng(99), ev = [];
  let u = 1.0;
  while (u < 8.5) { ev.push({ t: T.rt + u, s: Math.floor(r() * NODES.length), x: (r() * 800).toFixed(1), y: (r() * 800).toFixed(1) }); u += u < 3.0 ? 0.5 : u < 6.0 ? 0.25 : 0.125; }
  return ev;
})();
const UP = 0.22, DOWN = 0.3;
function nodePos(t, j) {
  const nd = NODES[j], r = ORB.rings[nd.k];
  const a = nd.a0 + (t - T.rt) * r.sp;
  const ex = Math.cos(a) * r.rx, ey = Math.sin(a) * r.rx * 0.34;
  const c = Math.cos(ORB.tilt), s = Math.sin(ORB.tilt);
  const spawn = E.outExpo(P(t, T.rt + 0.2 + j * 0.015, T.rt + 0.8 + j * 0.015));
  return { x: ORB.cx + (ex * c - ey * s) * spawn, y: ORB.cy + (ex * s + ey * c) * spawn, depth: Math.sin(a), spawn };
}

function sRealtime(ctx, t) {
  const u = t - T.rt;
  if (u < -0.1 || u > 9.1) return;
  const zx = E.inExpo(P(u, 8.6, 9.0));
  ctx.save();
  ctx.translate(ORB.cx, ORB.cy);
  ctx.scale(1 + zx * 9, 1 + zx * 9);
  ctx.translate(-ORB.cx, -ORB.cy);
  const vis = u >= 0;
  if (vis) {
    ORB.rings.forEach((r, k) => {
      const p = E.outExpo(P(u, 0.05 + k * 0.08, 0.8 + k * 0.08));
      if (p <= 0) return;
      ctx.save();
      ctx.translate(ORB.cx, ORB.cy); ctx.rotate(ORB.tilt);
      ctx.strokeStyle = hexA(C.white, 0.16); ctx.lineWidth = 1.2; ctx.setLineDash([3, 7]);
      ctx.beginPath(); ctx.ellipse(0, 0, r.rx, r.rx * 0.34, 0, -Math.PI / 2, -Math.PI / 2 + p * TAU); ctx.stroke();
      ctx.restore();
    });
    ctx.save();
    ctx.lineWidth = 1;
    NODES.forEach((_, j) => { const q = nodePos(t, j); ctx.strokeStyle = hexA(C.white, 0.05 * q.spawn); ctx.beginPath(); ctx.moveTo(ORB.cx, ORB.cy); ctx.lineTo(q.x, q.y); ctx.stroke(); });
    ctx.restore();
  }
  let hot = 0;
  for (const e of EVENTS) if (t > e.t + UP) hot += pulse(t - e.t - UP, 10) * 0.35;
  drawCore(ctx, t, hot);
  for (const e of EVENTS) {
    const d = t - e.t - UP;
    if (d < 0 || d > 0.5) continue;
    ctx.save();
    ctx.strokeStyle = hexA(C.green, 0.7 * (1 - d / 0.5)); ctx.lineWidth = 2;
    ctx.beginPath(); ctx.arc(ORB.cx, ORB.cy, 110 + d * 260, 0, TAU); ctx.stroke();
    ctx.restore();
  }
  if (vis) {
    ctx.save();
    ctx.globalCompositeOperation = 'lighter';
    for (const e of EVENTS) {
      const d = t - e.t;
      if (d < 0 || d > UP + DOWN + 0.02) continue;
      if (d < UP) {
        const q = nodePos(t, e.s), s = E.inCubic(d / UP);
        for (let g = 0; g < 5; g++) {
          const ss = clamp(s - g * 0.05);
          ctx.fillStyle = hexA(C.pink, g === 0 ? 1 : 0.35 - g * 0.06);
          ctx.beginPath(); ctx.arc(lerp(q.x, ORB.cx, ss), lerp(q.y, ORB.cy, ss), 6 - g * 0.8, 0, TAU); ctx.fill();
        }
      } else {
        const s = E.outCubic(clamp((d - UP) / DOWN));
        ctx.fillStyle = C.green;
        for (let j = 0; j < NODES.length; j++) {
          const q = nodePos(t, j);
          ctx.globalAlpha = 1 - 0.7 * s;
          ctx.beginPath(); ctx.arc(lerp(ORB.cx, q.x, s), lerp(ORB.cy, q.y, s), 3.4, 0, TAU); ctx.fill();
        }
        ctx.globalAlpha = 1;
      }
    }
    ctx.restore();
    const order = NODES.map((_, j) => j).sort((a, b) => nodePos(t, a).depth - nodePos(t, b).depth);
    for (const j of order) {
      const q = nodePos(t, j);
      if (q.spawn <= 0.01) continue;
      const col = ORB.rings[NODES[j].k].c;
      let fl = 0, sent = 0;
      for (const e of EVENTS) {
        const d = t - (e.t + UP + DOWN); if (d > 0 && d < 0.6) fl = Math.max(fl, pulse(d, 8));
        if (e.s === j && t > e.t) sent = Math.max(sent, pulse(t - e.t, 6));
      }
      const sc = (0.75 + 0.3 * (q.depth + 1) / 2) * (1 + 0.35 * fl + 0.6 * sent) * q.spawn;
      const sz = 15 * sc;
      ctx.save();
      ctx.translate(q.x, q.y);
      ctx.globalAlpha = 0.55 + 0.45 * (q.depth + 1) / 2;
      rr(ctx, -sz, -sz, sz * 2, sz * 2, 5 * sc);
      ctx.fillStyle = '#0c1119'; ctx.fill();
      ctx.strokeStyle = sent > 0.05 ? C.pink : hexA(fl > 0.05 ? C.green : C.white, 0.35 + 0.65 * fl);
      ctx.lineWidth = 1.5;
      if (fl > 0.05 || sent > 0.05) { ctx.shadowColor = sent > 0.05 ? C.pink : C.green; ctx.shadowBlur = 20 * Math.max(fl, sent); }
      ctx.stroke();
      ctx.shadowBlur = 0;
      ctx.fillStyle = col; ctx.beginPath(); ctx.arc(0, 0, 3.5 * sc, 0, TAU); ctx.fill();
      ctx.restore();
    }
  }
  ctx.restore();
  if (!vis) return;
  const pin = E.outExpo(P(u, 0.3, 0.85));
  const pout = E.inExpo(P(u, 8.4, 8.7));
  ctx.save();
  ctx.translate((1 - pin) * -560 - pout * 600, 0);
  const tp = { x: 80, y: 300, w: 400, h: 440 };
  glass(ctx, tp.x, tp.y, tp.w, tp.h, 18);
  text(ctx, 'SUBSCRIPTION', tp.x + 26, tp.y + 40, { size: 13, weight: 700, tracking: 4, fill: hexA(C.white, 0.55) });
  ctx.fillStyle = C.green; ctx.beginPath(); ctx.arc(tp.x + tp.w - 34, tp.y + 36, 5 + pulse(u % 0.5, 6) * 2, 0, TAU); ctx.fill();
  text(ctx, 'LIVE', tp.x + tp.w - 46, tp.y + 41, { size: 13, weight: 700, tracking: 3, fill: C.green, align: 'right' });
  font(ctx, 600, 19, MONO);
  let qx = tp.x + 26;
  [['SELECT', SYN.kw], [' * ', C.white], ['FROM', SYN.kw], [' player', SYN.type]].forEach(([s, c]) => { ctx.fillStyle = c; ctx.fillText(s, qx, tp.y + 80); qx += ctx.measureText(s).width; });
  ctx.fillStyle = 'rgba(255,255,255,0.08)'; ctx.fillRect(tp.x, tp.y + 100, tp.w, 1);
  const cols = [tp.x + 26, tp.x + 170, tp.x + 290];
  ['id', 'x', 'y'].forEach((h, i) => text(ctx, h, cols[i], tp.y + 132, { size: 15, weight: 600, fill: hexA(C.white, 0.45) }));
  const ROWS = 7, r0 = rng(5);
  const vals = Array.from({ length: ROWS }, () => ({ x: (r0() * 800).toFixed(1), y: (r0() * 800).toFixed(1), ct: -99 }));
  for (const e of EVENTS) if (t > e.t + UP) vals[e.s % ROWS] = { x: e.x, y: e.y, ct: e.t + UP };
  for (let r = 0; r < ROWS; r++) {
    const y = tp.y + 172 + r * 38;
    const fl = pulse(t - vals[r].ct, 5);
    if (fl > 0.02) { ctx.fillStyle = hexA(C.green, 0.16 * fl); ctx.fillRect(tp.x + 12, y - 25, tp.w - 24, 34); }
    text(ctx, `0x${(0x3a1f + r * 0x51d).toString(16)}…`, cols[0], y, { size: 16, fill: hexA(C.white, 0.7) });
    text(ctx, vals[r].x, cols[1], y, { size: 16, fill: fl > 0.1 ? C.green : C.white });
    text(ctx, vals[r].y, cols[2], y, { size: 16, fill: fl > 0.1 ? C.green : C.white });
  }
  ctx.restore();

  ctx.save();
  ctx.translate((1 - pin) * 560 + pout * 600, 0);
  const lp = { x: 1440, y: 300, w: 400, h: 440 };
  glass(ctx, lp.x, lp.y, lp.w, lp.h, 18);
  text(ctx, 'TRANSACTIONS', lp.x + 26, lp.y + 40, { size: 13, weight: 700, tracking: 4, fill: hexA(C.white, 0.55) });
  ctx.fillStyle = 'rgba(255,255,255,0.08)'; ctx.fillRect(lp.x, lp.y + 60, lp.w, 1);
  ctx.save();
  ctx.beginPath(); ctx.rect(lp.x, lp.y + 62, lp.w, lp.h - 64); ctx.clip();
  const past = EVENTS.filter(e => t > e.t).slice(-9).reverse();
  past.forEach((e, i) => {
    const age = t - e.t;
    const slide = E.outExpo(clamp(age / 0.12));
    const y = lp.y + 100 + (i - 1 + slide) * 42;
    const done = t > e.t + UP;
    ctx.globalAlpha = (1 - i / 10) * clamp(age * 10);
    if (done) check(ctx, lp.x + 26, y - 6, 12, C.green); else spinner(ctx, lp.x + 32, y - 6, 7, t, C.pink);
    text(ctx, `move_to(${e.x}, ${e.y})`, lp.x + 52, y, { size: 16, fill: hexA(C.white, 0.9) });
    text(ctx, done ? '→36' : '…', lp.x + lp.w - 24, y, { size: 14, weight: 600, fill: hexA(C.white, 0.55), align: 'right' });
  });
  ctx.restore();
  ctx.restore();

  ctx.save();
  ctx.globalAlpha = 1 - pout;
  riseParts(ctx, [{ s: 'Subscribe' }, { s: 'to the rows you need.' }], CX, 170, { t: u - 0.3, size: 70, weight: 800, align: 'center', stagger: 0.02, outT: u > 3.0 ? u - 3.0 : undefined, outDir: -1 });
  riseParts(ctx, [{ s: 'Every change. Every client.' }, { s: 'Instantly.', fill: 'brand' }], CX, 170, { t: u - 3.2, size: 70, weight: 800, align: 'center', stagger: 0.014 });
  const bt = u - 4.6;
  if (bt > 0) {
    kicker(ctx, 'IN PRODUCTION', CX, 905, bt, { align: 'center', size: 15 });
    riseParts(ctx, [{ s: 'The entire backend of' }, { s: 'BitCraft Online', fill: C.white }, { s: '— a full MMORPG — is one SpacetimeDB module.' }], CX, 958, { t: bt - 0.1, size: 34, weight: 600, align: 'center', stagger: 0.008, dur: 0.4, fill: hexA(C.white, 0.72) });
  }
  ctx.restore();
}

// ======================================================================
// 5 — PERFORMANCE
// ======================================================================
const BENCH = [['SpacetimeDB', 303920], ['Node.js + SQLite', 3188], ['Bun + Postgres', 2773], ['Node.js + Supabase', 2534], ['Node.js + Postgres', 961], ['Convex', 127]];
function sSpeed(ctx, t) {
  const u = t - T.speed;
  if (u < -0.15 || u > 7.7) return;
  const ex = E.inExpo(P(u, 7.2, 7.5));
  const ent = E.outExpo(P(u, -0.1, 0.35));
  ctx.save();
  ctx.translate((1 - ent) * W - ex * W * 1.1, 0);
  const V = uu => { const cp = P(uu, 0.05, 2.0); return cp >= 1 ? 303920 : 303920 * E.outExpo(cp); };
  const v = V(u), rate = (V(u + 0.005) - V(u - 0.005)) / 0.01;
  const up = E.inOutExpo(P(u, 2.6, 3.1));
  const cy = lerp(CY + 70, 225, up), sc = lerp(1, 0.46, up);
  const settle = 1 - E.outCubic(P(u, 1.5, 2.0));
  const bump = 1 + 0.04 * pulse(u - 2.0, 8);
  ctx.save();
  ctx.translate(CX, cy);
  ctx.scale(sc * bump, sc * bump);
  const size = 290;
  font(ctx, 900, size, SANS);
  const dw = ctx.measureText('0').width, comw = ctx.measureText(',').width;
  const tw = dw * 6 + comw;
  let x = -tw / 2;
  const gFill = brandGrad(ctx, -tw / 2, 0, tw / 2, 0, 0.15 * Math.sin(u * 2));
  const mag = Math.floor(v).toString().length;
  let place = 5;
  for (const ch of '000,000') {
    if (ch === ',') { ctx.fillStyle = mag > 3 ? gFill : hexA(C.white, 0.12); ctx.fillText(',', x, 0); x += comw; continue; }
    const q = v / Math.pow(10, place);
    const d = Math.floor(q) % 10;
    const r = rate / Math.pow(10, place);
    const f = r < 9 ? E.inOutCubic(q - Math.floor(q)) * settle : 0;
    const lead = place + 1 > mag;
    ctx.save();
    ctx.beginPath(); ctx.rect(x - 5, -size * 0.78, dw + 10, size * 0.86); ctx.clip();
    ctx.fillStyle = lead ? hexA(C.white, 0.1) : gFill;
    if (!lead) { ctx.shadowColor = hexA(C.purple, 0.6); ctx.shadowBlur = 40; }
    const lh = size * 0.86;
    ctx.fillText(String(d), x, -f * lh);
    ctx.fillText(String((d + 1) % 10), x, (1 - f) * lh);
    ctx.restore();
    x += dw; place--;
  }
  ctx.restore();
  text(ctx, 'TRANSACTIONS / SECOND', CX, cy + lerp(90, 50, up), { size: lerp(26, 17, up), weight: 600, tracking: lerp(12, 7, up), align: 'center', fill: hexA(C.white, 0.75), alpha: E.outCubic(P(u, 0.2, 0.6)) });
  kicker(ctx, 'ACID · SERIALIZABLE · IN-MEMORY', CX, cy - lerp(250, 150, up), u - 0.3, { align: 'center', size: lerp(22, 15, up), alpha: 1 - up });
  if (u > 2.8) {
    const x0 = 640, maxw = 1160, rh = 76, y0 = 400;
    BENCH.forEach(([name, val], i) => {
      const y = y0 + i * rh;
      const la = E.outExpo(P(u, 2.9 + i * 0.06, 3.3 + i * 0.06));
      text(ctx, name, x0 - 30 - (1 - la) * 40, y + 10, { size: i === 0 ? 26 : 22, weight: i === 0 ? 800 : 500, fam: SANS, align: 'right', fill: i === 0 ? C.white : hexA(C.white, 0.75), alpha: la });
      ctx.fillStyle = hexA(C.white, 0.06 * la);
      ctx.fillRect(x0, y - 18, maxw, 36);
      const w = i === 0 ? maxw * E.outExpo(P(u, 4.0, 4.5)) : Math.max(3, (maxw * val) / 303920) * E.outExpo(P(u, 3.2 + i * 0.07, 3.6 + i * 0.07));
      if (w > 0.5) {
        ctx.save();
        rr(ctx, x0, y - 18, w, 36, 4);
        ctx.fillStyle = i === 0 ? C.green : hexA(C.white, 0.35);
        if (i === 0) { ctx.shadowColor = hexA(C.green, 0.8); ctx.shadowBlur = 40; }
        ctx.fill();
        ctx.restore();
      }
      const vs = val.toLocaleString('en-US');
      if (i === 0) { if (u > 4.15) text(ctx, vs, x0 + w - 16, y + 9, { size: 24, weight: 800, fam: SANS, align: 'right', fill: '#081008', alpha: P(u, 4.15, 4.3) }); }
      else text(ctx, vs, x0 + w + 14, y + 8, { size: 19, weight: 500, fill: hexA(C.white, 0.65), alpha: la * P(u, 3.3, 3.5) });
    });
    const ct = u - 4.6;
    if (ct > 0) {
      riseParts(ctx, [{ s: '~110×', fill: C.green }, { s: 'the throughput of Bun + Postgres.' }], CX, 932, { t: ct, size: 48, weight: 800, align: 'center', stagger: 0.014 });
      text(ctx, 'Vendor-run contended transfer benchmark (α = 1.5) · spacetimedb.com/blog/benchmarking', CX, 985, { size: 15, weight: 400, align: 'center', fill: hexA(C.white, 0.45), alpha: P(u, 5.0, 5.3) });
    }
  }
  ctx.restore();
}

// ======================================================================
// 6 — SCALE
// ======================================================================
const HEX = (() => {
  const out = [];
  const dirs = [[1, 0], [1, -1], [0, -1], [-1, 0], [-1, 1], [0, 1]];
  for (let ring = 1; ring <= 3; ring++) {
    let q = -ring, r = ring;
    for (let d = 0; d < 6; d++) for (let s = 0; s < ring; s++) { out.push({ q, r, ring, i: out.length }); q += dirs[d][0]; r += dirs[d][1]; }
  }
  return out;
})();
const REGIONS = ['eu', 'na', 'ap', 'sa', 'af', 'oc'];
const hexXY = (h, sp) => [sp * (Math.sqrt(3) * h.q + (Math.sqrt(3) / 2) * h.r), sp * 1.5 * h.r];
const hexT = h => 0.2 + (h.ring - 1) * 0.55 + (h.i % (6 * h.ring)) * 0.025;

function sScale(ctx, t) {
  const u = t - T.scale;
  if (u < -0.05 || u > 12.2) return;
  const ent = E.outExpo(P(u, -0.05, 0.4));
  const im = E.inQuart(P(u, 11.3, 12.0));
  ctx.save();
  ctx.translate(CX, CY); ctx.rotate(im * 0.5); ctx.scale(1 - im * 0.995, 1 - im * 0.995); ctx.translate(-CX, -CY);
  ctx.globalAlpha = 1 - P(u, 11.8, 12.0);
  ctx.translate((1 - ent) * W, 0);

  const aOut = E.inCubic(P(u, 6.0, 6.4));
  if (u < 6.45) {
    ctx.save();
    ctx.globalAlpha = 1 - aOut;
    ctx.translate(CX, CY); ctx.scale(1 - aOut * 0.08, 1 - aOut * 0.08); ctx.translate(-CX, -CY);
    kicker(ctx, 'OK, BUT…', 300, 128, u - 0.05, { size: 16, tracking: 6 });
    riseParts(ctx, [{ s: 'Does it' }, { s: 'scale?', fill: 'brand' }], 296, 205, { t: u - 0.15, size: 84, weight: 900, stagger: 0.025 });
    const ch = { x: 300, y: 290, w: 1320, h: 470 };
    const yMax = 350000;
    const yP = v => ch.y + ch.h - (v / yMax) * ch.h;
    const xP = n => ch.x + (Math.log2(n) / 7) * ch.w;
    const ax = E.outExpo(P(u, 0.5, 1.0));
    ctx.strokeStyle = hexA(C.white, 0.35); ctx.lineWidth = 1.5;
    ctx.beginPath(); ctx.moveTo(ch.x, ch.y + ch.h - ch.h * ax); ctx.lineTo(ch.x, ch.y + ch.h); ctx.lineTo(ch.x + ch.w * ax, ch.y + ch.h); ctx.stroke();
    [100000, 200000, 300000].forEach((v, i) => {
      const a = E.outCubic(P(u, 0.6 + i * 0.06, 1.0 + i * 0.06));
      ctx.fillStyle = hexA(C.white, 0.07 * a); ctx.fillRect(ch.x, yP(v), ch.w * a, 1);
      text(ctx, `${v / 1000}k`, ch.x - 16, yP(v) + 5, { size: 15, align: 'right', fill: hexA(C.white, 0.5), alpha: a });
    });
    [1, 2, 4, 8, 16, 32, 64, 128].forEach((n, i) => text(ctx, String(n), xP(n), ch.y + ch.h + 30, { size: 15, align: 'center', fill: hexA(C.white, 0.5), alpha: E.outCubic(P(u, 0.7 + i * 0.03, 1.0 + i * 0.03)) }));
    text(ctx, 'CORES IN A DISTRIBUTED CLUSTER  →', ch.x + ch.w, ch.y + ch.h + 64, { size: 13, weight: 600, tracking: 3, align: 'right', fill: hexA(C.white, 0.45), alpha: ax });
    text(ctx, 'TPS', ch.x - 16, ch.y - 16, { size: 13, weight: 600, tracking: 3, align: 'right', fill: hexA(C.white, 0.45), alpha: ax });
    const cp = E.inOutCubic(P(u, 1.0, 2.4));
    const curve = n => 100000 * (1 - Math.exp(-n / 7));
    if (cp > 0) {
      ctx.save();
      ctx.strokeStyle = C.pink; ctx.lineWidth = 3.5; ctx.shadowColor = C.pink; ctx.shadowBlur = 16;
      ctx.beginPath();
      const steps = 120;
      let hx = 0, hy = 0, hn = 1;
      for (let i = 0; i <= steps * cp; i++) { const n = Math.pow(2, (i / steps) * 7); hx = xP(n); hy = yP(curve(n)); hn = n; i === 0 ? ctx.moveTo(hx, hy) : ctx.lineTo(hx, hy); }
      ctx.stroke();
      ctx.fillStyle = C.pink; ctx.beginPath(); ctx.arc(hx, hy, 7, 0, TAU); ctx.fill();
      ctx.restore();
      text(ctx, `${Math.round(hn)} cores · ${Math.round(curve(hn) / 1000)}k TPS`, hx + 16, hy - 16, { size: 15, weight: 600, fill: C.pink, alpha: 1 - P(u, 2.4, 2.7) });
      text(ctx, 'Horizontally-scaled cluster · 1% contention · 1 ms commits', xP(3), yP(curve(3)) - 40, { size: 15, weight: 500, fill: hexA(C.pink, 0.9), alpha: E.outCubic(P(u, 1.3, 1.7)) });
    }
    const cl = E.outExpo(P(u, 2.2, 2.6));
    if (cl > 0) {
      ctx.save();
      ctx.strokeStyle = C.red; ctx.lineWidth = 2; ctx.setLineDash([10, 8]);
      ctx.beginPath(); ctx.moveTo(ch.x, yP(100000)); ctx.lineTo(ch.x + ch.w * cl, yP(100000)); ctx.stroke();
      ctx.restore();
      text(ctx, 'CEILING: (1 / 1 ms) ÷ 1% = 100,000 TPS — no matter how many cores you add', ch.x + ch.w, yP(100000) - 16, { size: 15, weight: 600, align: 'right', fill: C.red, alpha: cl });
    }
    const sl = E.outExpo(P(u, 3.0, 3.4));
    if (sl > 0) {
      const y = yP(300000);
      ctx.save();
      rr(ctx, ch.x, y - 3, ch.w * sl, 6, 3);
      ctx.fillStyle = C.green; ctx.fill();
      ctx.restore();
      text(ctx, 'SpacetimeDB · single-threaded · ~300,000 TPS', ch.x + 12, y - 20, { size: 20, weight: 700, fam: SANS, fill: C.white, alpha: P(u, 3.2, 3.5) });
    }
    const qt = u - 3.5;
    if (qt > 0) {
      riseParts(ctx, [{ s: 'Just 1% contention makes a cluster' }, { s: 'slower than a single core.' }], CX, 915, { t: qt, size: 40, weight: 800, align: 'center', stagger: 0.01 });
      text(ctx, 'spacetimedb.com/blog/how-does-spacetime-scale', CX, 965, { size: 15, align: 'center', fill: hexA(C.white, 0.45), alpha: P(u, 4.0, 4.3) });
    }
    ctx.restore();
  }

  const bu = u - 6.4;
  if (bu > 0) {
    riseParts(ctx, [{ s: 'Scale out when' }, { s: 'your workload does.', fill: 'brand' }], 120, 205, { t: bu - 0.1, size: 72, weight: 900, stagger: 0.018 });
    kicker(ctx, 'HOW BITCRAFT SCALES', 124, 128, bu, { size: 16, tracking: 6 });
    const lines = [
      ['A root database holds global data.', hexA(C.white, 0.5)],
      ['Region databases run in parallel.', hexA(C.white, 0.5)],
      ['Transactions stay local — and fast.', hexA(C.white, 0.5)],
    ];
    lines.forEach(([s, col], i) => {
      const lt = bu - (0.8 + i * 0.45);
      if (lt < 0) return;
      const a = E.outExpo(clamp(lt / 0.5));
      ctx.save();
      ctx.globalAlpha = a;
      ctx.translate((1 - a) * -30, 0);
      ctx.fillStyle = col; ctx.shadowColor = col; ctx.shadowBlur = 12;
      ctx.beginPath(); ctx.arc(134, 392 + i * 78, 7, 0, TAU); ctx.fill(); ctx.shadowBlur = 0;
      text(ctx, s, 160, 402 + i * 78, { size: 32, weight: 600, fam: SANS });
      ctx.restore();
    });
    const mx = 1340, my = 600, sp = 68;
    const camS = lerp(1, 0.86, E.inOutCubic(P(bu, 2.0, 3.2)));
    ctx.save();
    ctx.translate(mx, my); ctx.scale(camS, camS); ctx.translate(-mx, -my);
    ctx.save();
    ctx.lineWidth = 1.2;
    for (const h of HEX) {
      const ht = bu - hexT(h);
      if (ht < 0) continue;
      const [x, y] = hexXY(h, sp);
      const a = E.outExpo(clamp(ht / 0.4));
      ctx.strokeStyle = hexA(C.white, 0.07 * a);
      ctx.beginPath(); ctx.moveTo(mx, my); ctx.lineTo(mx + x * a, my + y * a); ctx.stroke();
    }
    ctx.restore();
    const mr = rng(314);
    for (let k = 0; k < 40; k++) {
      const t0 = 0.8 + k * 0.12 + mr() * 0.1, h = HEX[Math.floor(mr() * HEX.length)], dir = mr() < 0.5;
      const d = (bu - t0) / 0.45;
      if (d < 0 || d > 1) continue;
      if (bu - hexT(h) < 0.4) continue;
      const [x, y] = hexXY(h, sp);
      const s = E.inOutCubic(dir ? d : 1 - d);
      ctx.fillStyle = dir ? C.green : hexA(C.white, 0.8); ctx.shadowColor = ctx.fillStyle; ctx.shadowBlur = 10;
      ctx.beginPath(); ctx.arc(mx + x * s, my + y * s, 3.5, 0, TAU); ctx.fill(); ctx.shadowBlur = 0;
    }
    for (const h of HEX) {
      const ht = bu - hexT(h);
      if (ht < 0) continue;
      const [x, y] = hexXY(h, sp);
      const p = E.outExpo(clamp(ht / 0.45));
      const sc = spring(ht, 1.8, 7);
      const px = mx + x * p, py = my + y * p;
      const col = '#9aa3b2';
      const rate = 0.6 + hash(h.i) * 1.4, beat = pulse(((bu * rate + hash(h.i + 3)) % 1), 7);
      ctx.save();
      ctx.translate(px, py); ctx.scale(sc, sc);
      ctx.beginPath();
      for (let k = 0; k < 6; k++) { const a = Math.PI / 6 + (k * TAU) / 6; k ? ctx.lineTo(Math.cos(a) * 30, Math.sin(a) * 30) : ctx.moveTo(Math.cos(a) * 30, Math.sin(a) * 30); }
      ctx.closePath();
      ctx.fillStyle = '#0c1119'; ctx.fill();
      ctx.strokeStyle = hexA(col, 0.45 + 0.5 * beat); ctx.lineWidth = 1.6;
      ctx.shadowColor = col; ctx.shadowBlur = 14 * beat; ctx.stroke(); ctx.shadowBlur = 0;
      ctx.strokeStyle = hexA(C.white, 0.6); ctx.lineWidth = 2.2;
      ctx.beginPath(); ctx.arc(0, -5, 8, bu * rate * 5, bu * rate * 5 + 4); ctx.stroke();
      text(ctx, `${REGIONS[h.i % 6]}-${Math.floor(h.i / 6) + 1}`, 0, 17, { size: 10, weight: 600, align: 'center', fill: hexA(C.white, 0.6) });
      ctx.restore();
    }
    const rs = spring(bu - 0.2, 1.6, 6);
    if (bu > 0.2) {
      ctx.beginPath(); ctx.arc(mx, my, 52 * rs, 0, TAU); ctx.fillStyle = '#0a0e14'; ctx.fill();
      drawMark(ctx, mx, my, 120 * rs, { glow: hexA(C.green, 0.9), glowBlur: 30, rot: bu * 0.2 });
      text(ctx, 'ROOT', mx, my + 78, { size: 13, weight: 700, tracking: 4, align: 'center', fill: hexA(C.white, 0.6), alpha: P(bu, 0.5, 0.8) });
    }
    ctx.restore();
    const shown = HEX.filter(h => bu - hexT(h) > 0).length + (bu > 0.2 ? 1 : 0);
    ctx.save();
    ctx.globalAlpha = E.outCubic(P(bu, 0.6, 1.0));
    text(ctx, 'DATABASES', 124, 660, { size: 14, weight: 600, tracking: 5, fill: hexA(C.white, 0.5) });
    text(ctx, String(shown), 120, 738, { size: 72, weight: 800, fam: SANS });
    text(ctx, 'Customers run hundreds — even thousands.', 124, 780, { size: 19, weight: 500, fam: SANS, fill: hexA(C.white, 0.65), alpha: P(bu, 2.4, 2.8) });
    ctx.restore();
    const qt = bu - 2.4;
    if (qt > 0) {
      riseText(ctx, '“You should not have to pay for horizontal', 120, 900, { t: qt, size: 32, weight: 600, stagger: 0.008, dur: 0.4, fill: C.white, tracking: 0 });
      riseText(ctx, 'scalability until you use it.”', 120, 944, { t: qt - 0.25, size: 32, weight: 600, stagger: 0.008, dur: 0.4, fill: C.white, tracking: 0 });
      text(ctx, 'THE ZERO-OVERHEAD PRINCIPLE · spacetimedb.com/blog/how-does-spacetime-scale', 122, 988, { size: 13, weight: 600, tracking: 2, fill: hexA(C.white, 0.45), alpha: P(bu, 3.0, 3.4) });
    }
  }
  ctx.restore();
  starPoint(ctx, 11.3, 12.0, u);
}

// ======================================================================
// 7 — MAINCLOUD
// ======================================================================
// usage curve: surges, idle stretch at zero, surge again
function usage(x) {
  const bump = (c, w, h) => h * Math.exp(-((x - c) * (x - c)) / (2 * w * w));
  let v = bump(0.12, 0.05, 0.35) + bump(0.28, 0.06, 0.95) + bump(0.36, 0.03, 0.5) + bump(0.8, 0.05, 0.7) + bump(0.9, 0.04, 0.45);
  v += 0.05 * Math.sin(x * 90) * (v > 0.05 ? 1 : 0);
  const idle = x > 0.46 && x < 0.7;
  return idle ? 0 : Math.max(0, v);
}
function sCloud(ctx, t) {
  const u = t - T.cloud;
  if (u < -0.1 || u > 7.2) return;
  const ent = E.outExpo(P(u, -0.1, 0.35));
  const ex = E.inExpo(P(u, 6.7, 7.0));
  ctx.save();
  ctx.translate((1 - ent) * W - ex * W * 1.1, 0);
  kicker(ctx, 'MAINCLOUD · FULLY MANAGED · SERVERLESS', 142, 118, u - 0.05, { size: 16, tracking: 6 });
  riseParts(ctx, [{ s: 'Ship it to' }, { s: 'Maincloud.', fill: 'brand' }], 140, 205, { t: u - 0.1, size: 84, weight: 900, stagger: 0.022 });

  // terminal
  const tin = E.outExpo(P(u, 0.25, 0.75));
  ctx.save();
  ctx.globalAlpha = tin;
  ctx.translate(0, (1 - tin) * 40);
  const tm = { x: 140, y: 270, w: 800, h: 250 };
  glass(ctx, tm.x, tm.y, tm.w, tm.h, 20, { top: 'rgba(10,12,18,0.96)', bottom: 'rgba(7,9,13,0.96)' });
  trafficLights(ctx, tm.x, tm.y);
  ctx.fillStyle = 'rgba(255,255,255,0.07)'; ctx.fillRect(tm.x, tm.y + 60, tm.w, 1);
  termRows(ctx, tm, u, [
    { y: 110, cmd: 'spacetime login', c0: 0.6, c1: 0.95, res: [[1.05, 'Logged in', C.green, 0]] },
    { y: 190, cmd: 'spacetime publish my-game --server maincloud', c0: 1.3, c1: 2.0, res: [[2.1, 'Live at maincloud.spacetimedb.com', C.green, 0]] },
  ]);
  ctx.restore();
  // managed badges
  const BAD = ['Managed infrastructure', 'Automatic scaling', 'Replication', 'Backups'];
  BAD.forEach((s, i) => {
    const lt = u - (2.4 + i * 0.15);
    if (lt < 0) return;
    const a = E.outExpo(clamp(lt / 0.45));
    const c = i % 2, r = Math.floor(i / 2);
    const x = 140 + c * 410, y = 560 + r * 84;
    ctx.save();
    ctx.globalAlpha = a;
    ctx.translate(x, y + (1 - a) * 24);
    glass(ctx, 0, 0, 390, 66, 14, { shadow: false });
    check(ctx, 24, 35, 16, C.green, E.outCubic(clamp((lt - 0.1) / 0.3)));
    text(ctx, s, 58, 42, { size: 23, weight: 600, fam: SANS });
    ctx.restore();
  });

  // usage chart
  const cin = E.outExpo(P(u, 0.5, 1.0));
  const cp = { x: 1010, y: 270, w: 800, h: 460 };
  ctx.save();
  ctx.globalAlpha = cin;
  ctx.translate((1 - cin) * 60, 0);
  glass(ctx, cp.x, cp.y, cp.w, cp.h, 20);
  text(ctx, 'TRAFFIC', cp.x + 30, cp.y + 44, { size: 13, weight: 700, tracking: 4, fill: C.green });
  text(ctx, 'YOU PAY FOR', cp.x + 150, cp.y + 44, { size: 13, weight: 700, tracking: 4, fill: C.pink });
  ctx.fillStyle = C.green; ctx.fillRect(cp.x + 30, cp.y + 54, 80, 2);
  ctx.fillStyle = C.pink; ctx.fillRect(cp.x + 150, cp.y + 54, 110, 2);
  const gx = cp.x + 30, gw = cp.w - 60, gy = cp.y + cp.h - 50, gh = cp.h - 150;
  ctx.fillStyle = hexA(C.white, 0.15); ctx.fillRect(gx, gy, gw, 1);
  const prog = E.inOutSine(P(u, 1.0, 4.2));
  if (prog > 0) {
    const N = 200;
    ctx.beginPath();
    ctx.moveTo(gx, gy);
    let lx = gx, ly = gy;
    for (let i = 0; i <= N * prog; i++) { const x = i / N; lx = gx + x * gw; ly = gy - usage(x) * gh; ctx.lineTo(lx, ly); }
    ctx.lineTo(lx, gy); ctx.closePath();
    ctx.fillStyle = hexA(C.green, 0.14); ctx.fill();
    ctx.save();
    ctx.strokeStyle = C.green; ctx.lineWidth = 2.5; ctx.shadowColor = C.green; ctx.shadowBlur = 12;
    ctx.beginPath();
    for (let i = 0; i <= N * prog; i++) { const x = i / N; const px = gx + x * gw, py = gy - usage(x) * gh; i ? ctx.lineTo(px, py) : ctx.moveTo(px, py); }
    ctx.stroke();
    // cost tracks usage
    ctx.strokeStyle = hexA(C.white, 0.7); ctx.lineWidth = 2; ctx.setLineDash([6, 6]);
    ctx.beginPath();
    for (let i = 0; i <= N * prog; i++) { const x = i / N; const px = gx + x * gw, py = gy - usage(x) * gh * 0.55 - 3; i ? ctx.lineTo(px, py) : ctx.moveTo(px, py); }
    ctx.stroke();
    ctx.restore();
    // idle annotation
    const ia = E.outCubic(P(u, 2.5, 2.9));
    if (ia > 0) {
      const x0 = gx + 0.47 * gw, x1 = gx + 0.69 * gw;
      ctx.save();
      ctx.globalAlpha *= ia;
      ctx.fillStyle = hexA(C.white, 0.05); ctx.fillRect(x0, gy - gh, x1 - x0, gh);
      text(ctx, 'IDLE', (x0 + x1) / 2, gy - gh * 0.55, { size: 14, weight: 700, tracking: 4, align: 'center', fill: hexA(C.white, 0.6) });
      text(ctx, 'scaled to zero', (x0 + x1) / 2, gy - gh * 0.55 + 30, { size: 17, weight: 600, fam: SANS, align: 'center', fill: C.white });
      text(ctx, '$0', (x0 + x1) / 2, gy - 18, { size: 22, weight: 800, fam: SANS, align: 'center', fill: C.white });
      ctx.restore();
    }
  }
  ctx.restore();

  const bt = u - 3.3;
  if (bt > 0) {
    riseParts(ctx, [{ s: 'Scales to zero when idle.' }, { s: 'Pay only for what you use.' }], CX, 900, { t: bt, size: 46, weight: 800, align: 'center', stagger: 0.012 });
    // CTA: solid green button + URL
    const cp = clamp((u - 3.9) / 0.45);
    if (cp > 0) {
      const bl = 'Start free', url = 'spacetimedb.com/pricing';
      const bw = measure(ctx, bl, 700, 26, SANS) + 64, bh = 58;
      const uw = measure(ctx, url, 500, 20, MONO);
      const gw = bw + 28 + uw;
      const gx = CX - gw / 2, cy = 972;
      const sc = E.outBack(cp, 2.2);
      ctx.save();
      ctx.globalAlpha *= clamp(cp * 3);
      ctx.translate(gx + bw / 2, cy); ctx.scale(sc, sc);
      rr(ctx, -bw / 2, -bh / 2, bw, bh, bh / 2);
      ctx.fillStyle = C.green; ctx.fill();
      text(ctx, bl, 0, 9, { size: 26, weight: 700, fam: SANS, align: 'center', fill: C.bg });
      ctx.restore();
      text(ctx, url, gx + bw + 28, cy + 7, { size: 20, weight: 500, fill: hexA(C.white, 0.8), alpha: E.outCubic(clamp((u - 4.1) / 0.4)) });
    }
  }
  ctx.restore();
}

// ======================================================================
// 8 — BUILT IN (features)
// ======================================================================
const FEATS = [
  { k: 'REDUCERS', title: 'ACID transactions', d: ['Atomic, serializable functions', 'that run inside the database.'], col: C.green },
  { k: 'SUBSCRIPTIONS', title: 'Live queries', d: ['Clients mirror exactly the rows', 'they subscribe to.'], col: C.pink },
  { k: 'VIEWS', title: 'Computed data', d: ['Read-only functions, exposed', 'to clients like tables.'], col: C.purple },
  { k: 'SCHEDULED REDUCERS', title: 'Timers & game loops', d: ['Run logic on an interval or at', 'a set time — in the database.'], col: C.yellow },
  { k: 'AUTH & IDENTITY', title: 'SpacetimeAuth + OIDC', d: ['Auth0, Clerk, Google, GitHub…', 'ctx.sender is authenticated.'], col: C.cyan },
  { k: 'HOT-SWAP', title: 'Zero-downtime updates', d: ['Republish your module without', 'disconnecting clients.'], col: C.green },
];
function featIllus(ctx, i, x, y, u, col) {
  ctx.save();
  ctx.translate(x, y);
  ctx.strokeStyle = col; ctx.fillStyle = col; ctx.lineWidth = 2.5; ctx.lineCap = 'round';
  if (i === 0) {
    for (let r = 0; r < 3; r++) {
      const ph = ((u * 0.9 + r * 0.33) % 1);
      rr(ctx, -50, -38 + r * 28, 100, 18, 5); ctx.globalAlpha = 0.25; ctx.fill(); ctx.globalAlpha = 1;
      rr(ctx, -50, -38 + r * 28, Math.max(1, 100 * E.outCubic(clamp(ph * 1.6))), 18, 5); ctx.fill();
    }
  } else if (i === 1) {
    ctx.beginPath(); ctx.arc(0, 0, 8, 0, TAU); ctx.fill();
    for (let k = 0; k < 3; k++) { const p = (u * 0.8 + k / 3) % 1; ctx.globalAlpha = 1 - p; ctx.beginPath(); ctx.arc(0, 0, 12 + p * 50, 0, TAU); ctx.stroke(); }
  } else if (i === 2) {
    ctx.globalAlpha = 0.7;
    ctx.beginPath(); ctx.moveTo(-50, -45); ctx.lineTo(50, -45); ctx.lineTo(10, 5); ctx.lineTo(10, 40); ctx.lineTo(-10, 40); ctx.lineTo(-10, 5); ctx.closePath(); ctx.stroke();
    ctx.globalAlpha = 1;
    for (let k = 0; k < 8; k++) { const p = (u * 0.7 + k / 8) % 1; const keep = k % 3 === 0; const yy = -70 + p * 140; if (!keep && yy > -45) continue; if (yy < -62) continue; ctx.beginPath(); ctx.arc(keep ? lerp((k - 4) * 10, 0, clamp((yy + 45) / 50)) : (k - 4) * 10, yy, 4, 0, TAU); ctx.fill(); }
  } else if (i === 3) {
    ctx.beginPath(); ctx.arc(0, 0, 46, 0, TAU); ctx.stroke();
    for (let k = 0; k < 12; k++) { const a = (k / 12) * TAU; ctx.beginPath(); ctx.moveTo(Math.cos(a) * 38, Math.sin(a) * 38); ctx.lineTo(Math.cos(a) * 44, Math.sin(a) * 44); ctx.stroke(); }
    const a = u * 2.5 - Math.PI / 2; ctx.beginPath(); ctx.moveTo(0, 0); ctx.lineTo(Math.cos(a) * 34, Math.sin(a) * 34); ctx.stroke();
    const b = u * 0.3 - Math.PI / 2; ctx.lineWidth = 4; ctx.beginPath(); ctx.moveTo(0, 0); ctx.lineTo(Math.cos(b) * 22, Math.sin(b) * 22); ctx.stroke();
  } else if (i === 4) {
    ctx.beginPath(); ctx.moveTo(0, -50); ctx.lineTo(40, -34); ctx.lineTo(36, 10); ctx.quadraticCurveTo(28, 38, 0, 52); ctx.quadraticCurveTo(-28, 38, -36, 10); ctx.lineTo(-40, -34); ctx.closePath(); ctx.stroke();
    check(ctx, -16, 2, 32, col, clamp((u % 1.6) * 2));
  } else {
    const a = u * 2;
    for (let k = 0; k < 2; k++) {
      ctx.beginPath(); ctx.arc(0, 0, 42, a + k * Math.PI, a + k * Math.PI + 2.4); ctx.stroke();
      const e = a + k * Math.PI + 2.4; const hx = Math.cos(e) * 42, hy = Math.sin(e) * 42;
      ctx.beginPath(); ctx.moveTo(hx + Math.cos(e - 2.3) * 14, hy + Math.sin(e - 2.3) * 14); ctx.lineTo(hx, hy); ctx.lineTo(hx + Math.cos(e + 2.1) * 14, hy + Math.sin(e + 2.1) * 14); ctx.stroke();
    }
    font(ctx, 700, 18, MONO); ctx.textAlign = 'center'; ctx.fillText(`v${1 + (Math.floor(u / 1.4) % 9)}`, 0, 7);
  }
  ctx.restore();
}
function sFeatures(ctx, t) {
  const u = t - T.feat;
  if (u < -0.1 || u > 6.7) return;
  const ent = E.outExpo(P(u, -0.1, 0.35));
  const ex = E.inExpo(P(u, 6.2, 6.5));
  ctx.save();
  ctx.translate((1 - ent) * W - ex * W * 1.1, 0);
  kicker(ctx, 'EVERYTHING IN ONE MODULE', CX, 115, u - 0.05, { align: 'center', size: 16, tracking: 6 });
  riseParts(ctx, [{ s: 'Everything your backend needs.' }, { s: 'Built in.', fill: 'brand' }], CX, 190, { t: u - 0.1, size: 64, weight: 900, align: 'center', stagger: 0.012 });
  const cw = 520, chh = 300, gap = 26;
  const x0 = CX - (3 * cw + 2 * gap) / 2, y0 = 250;
  FEATS.forEach((f, i) => {
    const c = i % 3, r = Math.floor(i / 3);
    const lt = u - (0.4 + i * 0.2);
    if (lt < 0) return;
    const p = E.outExpo(clamp(lt / 0.6));
    const fu = 1.9 + i * 0.7;
    const foc = u > fu ? pulse(u - fu, 1.6) : 0;
    const x = x0 + c * (cw + gap), y = y0 + r * (chh + gap);
    ctx.save();
    ctx.globalAlpha = clamp(lt * 4);
    ctx.translate(x + cw / 2, y + chh / 2 + (1 - p) * 60);
    ctx.scale(1 + 0.03 * foc, 1 + 0.03 * foc);
    glass(ctx, -cw / 2, -chh / 2, cw, chh, 20, { shadow: false });
    if (foc > 0.02) { rr(ctx, -cw / 2, -chh / 2, cw, chh, 20); ctx.strokeStyle = hexA(C.green, 0.9 * foc); ctx.lineWidth = 2; ctx.shadowColor = f.col; ctx.shadowBlur = 30 * foc; ctx.stroke(); ctx.shadowBlur = 0; }
    text(ctx, f.k, -cw / 2 + 32, -chh / 2 + 52, { size: 14, weight: 700, tracking: 3, fill: hexA(C.white, 0.5) });
    text(ctx, f.title, -cw / 2 + 32, -chh / 2 + 110, { size: 34, weight: 800, fam: SANS });
    f.d.forEach((s, k) => text(ctx, s, -cw / 2 + 32, -chh / 2 + 165 + k * 32, { size: 21, weight: 400, fam: SANS, fill: hexA(C.white, 0.7) }));
    ctx.save(); ctx.translate(cw / 2 - 78, chh / 2 - 74); ctx.scale(0.78, 0.78); featIllus(ctx, i, 0, 0, u, hexA(C.white, 0.7)); ctx.restore();
    ctx.restore();
  });
  ctx.restore();
}

// ======================================================================
// 9 — LANGUAGES
// ======================================================================
const LANGS = [
  { s: 'Rust.', c: C.white, u: 0.1 },
  { s: 'C#.', c: C.white, u: 0.6 },
  { s: 'TypeScript.', c: C.white, u: 1.1 },
  { s: 'C++.', c: C.white, u: 1.6 },
];
const CLIENTS = [
  ['react', 'React'], ['nextjs', 'Next.js'], ['vue', 'Vue'], ['svelte', 'Svelte'], ['angular', 'Angular'], ['remix', 'Remix'], ['nuxt', 'Nuxt'],
  ['nodejs', 'Node.js'], ['bun', 'Bun'], ['deno', 'Deno'], ['tanstack', 'TanStack'], ['unity', 'Unity'], ['unreal', 'Unreal'], ['rust', 'Rust'],
];
function sLangs(ctx, t) {
  const u = t - T.langs;
  if (u < -0.3 || u > 6.3) return;
  const ent = E.outExpo(P(u, -0.2, 0.2));
  ctx.save();
  ctx.translate((1 - ent) * W, 0);
  const top = E.inOutExpo(P(u, 2.0, 2.4));
  if (u < 2.3) {
    kicker(ctx, 'WRITE YOUR SERVER MODULE IN', CX, CY - 150, u, { align: 'center', size: 24, tracking: 10, alpha: 1 - top });
    LANGS.forEach((Lg, i) => {
      const next = LANGS[i + 1];
      const outT = next ? u - next.u : u - 2.05;
      if (u < Lg.u - 0.02 || outT > 0.4) return;
      const lt = u - Lg.u;
      for (let e = 3; e >= 1; e--) {
        const es = 1 + e * 0.12 * E.outExpo(clamp(lt / 0.5));
        ctx.save();
        ctx.translate(CX, CY + 60); ctx.scale(es, es);
        ctx.globalAlpha = (0.18 / e) * pulse(lt, 2.5) * (outT > 0 ? 1 - clamp(outT / 0.2) : 1);
        font(ctx, 900, 230, SANS); ctx.textAlign = 'center';
        ctx.strokeStyle = Lg.c; ctx.lineWidth = 2; ctx.strokeText(Lg.s, 0, 70);
        ctx.restore();
      }
      riseText(ctx, Lg.s, CX, CY + 130, { t: lt, size: 230, weight: 900, align: 'center', stagger: 0.014, dur: 0.3, tracking: -4, fill: Lg.c, glow: hexA(Lg.c, 0.5), glowBlur: 50, outT: outT > 0 ? outT : undefined, outDir: 1, outDur: 0.18, outStagger: 0.008 });
    });
  }
  const rowT = u - 2.1;
  if (rowT > 0) {
    kicker(ctx, 'SERVER MODULES', CX, 170, rowT, { align: 'center', size: 16, tracking: 6 });
    riseParts(ctx, LANGS.map((Lg, i) => ({ s: Lg.s.replace('.', i < 3 ? '  ·' : ''), fill: Lg.c })), CX, 250, { t: rowT, size: 64, weight: 800, align: 'center', stagger: 0.01, outT: u > 5.75 ? u - 5.75 : undefined, outDir: -1 });
  }
  const gt = u - 2.35;
  if (gt > 0) {
    kicker(ctx, 'CLIENT SDKS · TYPE-SAFE BINDINGS GENERATED FOR YOU', CX, 360, gt, { align: 'center', size: 16, tracking: 6, alpha: 1 - P(u, 5.7, 5.9) });
    const cols = 7, tw = 196, th = 180, gap = 22;
    const gx = CX - (cols * tw + (cols - 1) * gap) / 2, gy = 400;
    CLIENTS.forEach(([k, name], i) => {
      const c = i % cols, r = Math.floor(i / cols);
      const d = (c + r * 1.5) * 0.05;
      if (gt - d < 0) return;
      const p = spring(gt - d, 1.6, 6.5);
      const sweep = pulse(Math.abs(u - (3.9 + (c + r) * 0.06)), 10);
      const outp = E.inExpo(P(u, 5.72 + hash(i) * 0.12, 6.02 + hash(i) * 0.12));
      const ox = (hash(i + 3) - 0.5) * 1400 * outp, oy = (hash(i + 7) - 0.3) * 1200 * outp;
      ctx.save();
      ctx.translate(gx + c * (tw + gap) + tw / 2 + ox, gy + r * (th + gap) + th / 2 + oy);
      ctx.rotate((hash(i + 1) - 0.5) * 2 * outp);
      ctx.scale(1, clamp(p, 0, 1.3));
      ctx.globalAlpha = clamp((gt - d) * 6) * (1 - outp);
      glass(ctx, -tw / 2, -th / 2, tw, th, 16, { shadow: false, stroke0: hexA(C.white, 0.12 + 0.5 * sweep) });
      ctx.globalAlpha *= 0.92;
      ctx.drawImage(A.logos[k], -38, -th / 2 + 26, 76, 76);
      text(ctx, name, 0, th / 2 - 30, { size: 17, weight: 600, align: 'center', fill: hexA(C.white, 0.85) });
      ctx.restore();
    });
  }
  const bt = u - 3.0;
  if (bt > 0) riseParts(ctx, [{ s: 'Web, native, Unity, Unreal.' }, { s: 'Connect from anything.', fill: 'brand' }], CX, 870, { t: bt, size: 50, weight: 800, align: 'center', stagger: 0.012, outT: u > 5.72 ? u - 5.72 : undefined });
  ctx.restore();
}

// ======================================================================
// 10 — AI AGENTS
// ======================================================================
const SKILLS = [
  ['concepts', 'architecture & rules', C.green],
  ['cli', 'init · publish · sql', C.green],
  ['mcp', 'operate a live database', C.green],
  ['rust-server', 'modules in Rust', C.purple],
  ['typescript-server', 'modules in TypeScript', C.purple],
  ['csharp-server', 'modules in C#', C.purple],
  ['cpp-server', 'modules in C++', C.purple],
  ['typescript-client', 'React & web clients', C.pink],
  ['csharp-client', 'C# / .NET clients', C.pink],
];
const TLINES = [
  { u: 1.5, s: 'Read agent-setup.md' },
  { u: 1.85, s: 'spacetime CLI detected' },
  { u: 2.2, s: 'Installed plugin spacetimedb@spacetimedb-plugins' },
  { u: 2.55, s: 'MCP server connected · spacetime mcp' },
  { u: 2.9, s: '9 skills loaded' },
];
function sAgents(ctx, t) {
  const u = t - T.ai;
  if (u < -0.15 || u > 9.6) return;
  const ent = E.outExpo(P(u, -0.1, 0.4));
  const exA = E.inExpo(P(u, 9.2, 9.5));
  ctx.save();
  ctx.translate(-exA * W * 1.1, 0);
  ctx.translate(CX, CY); ctx.scale(lerp(1.12, 1, ent), lerp(1.12, 1, ent)); ctx.translate(-CX, -CY);
  ctx.globalAlpha *= ent;
  kicker(ctx, 'AGENT SKILLS + MCP', 142, 118, u, { size: 16, tracking: 6 });
  riseParts(ctx, [{ s: 'Built for' }, { s: 'AI agents.', fill: 'brand' }], 140, 200, { t: u - 0.05, size: 92, weight: 900, stagger: 0.02 });
  const tm = { x: 140, y: 250, w: 930, h: 600 };
  glass(ctx, tm.x, tm.y, tm.w, tm.h, 20, { top: 'rgba(10,12,18,0.96)', bottom: 'rgba(7,9,13,0.96)' });
  trafficLights(ctx, tm.x, tm.y);
  text(ctx, 'agent — ~/my-game', tm.x + tm.w / 2, tm.y + 36, { size: 15, align: 'center', fill: hexA(C.white, 0.45) });
  ctx.fillStyle = 'rgba(255,255,255,0.07)'; ctx.fillRect(tm.x, tm.y + 60, tm.w, 1);
  const lx = tm.x + 36;
  let y = tm.y + 118;
  const prompt1 = 'Set yourself up with spacetimedb.com/agent-setup.md';
  const n1 = Math.floor(P(u, 0.4, 1.3) * prompt1.length);
  text(ctx, '›', lx, y, { size: 24, weight: 700, fill: C.pink });
  text(ctx, prompt1.slice(0, n1), lx + 28, y, { size: 21, weight: 500 });
  if (u < 1.45 && Math.floor(u * 4) % 2 === 0) { font(ctx, 500, 21, MONO); ctx.fillStyle = C.white; ctx.fillRect(lx + 30 + ctx.measureText(prompt1.slice(0, n1)).width, y - 19, 11, 24); }
  y += 62;
  for (const Ln of TLINES) {
    const lt = u - Ln.u;
    if (lt < 0) break;
    const a = E.outExpo(clamp(lt / 0.25));
    ctx.save();
    ctx.globalAlpha *= a;
    ctx.translate((1 - a) * 20, 0);
    if (lt < 0.2) spinner(ctx, lx + 8, y - 7, 8, t, C.pink); else check(ctx, lx, y - 6, 16, C.green, E.outCubic(clamp((lt - 0.2) / 0.12)));
    text(ctx, Ln.s, lx + 30, y, { size: 20, fill: lt < 0.2 ? hexA(C.white, 0.6) : hexA(C.white, 0.9) });
    ctx.restore();
    y += 44;
  }
  y += 22;
  const prompt2 = 'Build me a multiplayer game.';
  if (u > 4.3) {
    const n2 = Math.floor(P(u, 4.3, 4.8) * prompt2.length);
    text(ctx, '›', lx, y, { size: 24, weight: 700, fill: C.pink });
    text(ctx, prompt2.slice(0, n2), lx + 28, y, { size: 21, weight: 500 });
    if (u > 4.95) {
      y += 50;
      spinner(ctx, lx + 8, y - 7, 8, t, C.white);
      text(ctx, 'Writing module + client… ' + ['', '.', '..', '...'][Math.floor(u * 6) % 4], lx + 30, y, { size: 20, fill: hexA(C.white, 0.75) });
    }
  }
  const gx = 1112, gy = 250, cw = 232, ch = 186, gap = 16;
  SKILLS.forEach(([name, desc, col], i) => {
    const st = 3.1 + i * 0.08;
    const p = E.outExpo(P(u, st, st + 0.6));
    if (p <= 0) return;
    const c = i % 3, r = Math.floor(i / 3);
    const tx = gx + c * (cw + gap), ty = gy + r * (ch + gap);
    const x = lerp(700, tx, p), yy = lerp(480, ty, p);
    ctx.save();
    ctx.translate(x + cw / 2, yy + ch / 2);
    ctx.rotate((1 - p) * (hash(i) - 0.5) * 1.2);
    ctx.scale(lerp(0.4, 1, p), lerp(0.4, 1, p));
    ctx.globalAlpha *= clamp(p * 3);
    glass(ctx, -cw / 2, -ch / 2, cw, ch, 16, {});
    ctx.fillStyle = hexA(C.white, 0.4); ctx.shadowColor = col; ctx.shadowBlur = 12;
    ctx.beginPath(); ctx.arc(-cw / 2 + 24, -ch / 2 + 28, 5, 0, TAU); ctx.fill(); ctx.shadowBlur = 0;
    text(ctx, 'SKILL.md', cw / 2 - 20, -ch / 2 + 33, { size: 12, weight: 600, tracking: 2, fill: hexA(C.white, 0.4), align: 'right' });
    text(ctx, name, -cw / 2 + 20, 18, { size: name.length > 14 ? 17 : 21, weight: 700 });
    text(ctx, desc, -cw / 2 + 20, 50, { size: 13, weight: 400, fill: hexA(C.white, 0.6) });
    ctx.restore();
  });
  const bt = u - 5.0;
  if (bt > 0 && u < 7.2) riseParts(ctx, [{ s: 'Let AI ship the whole backend —' }, { s: 'glue-free.' }], 140, 960, { t: bt, size: 48, weight: 800, stagger: 0.012, outT: u > 6.6 ? u - 6.6 : undefined, outDir: -1 });
  const at = u - 6.8;
  if (at > 0) riseParts(ctx, [{ s: 'And a place for agents to work:' }, { s: 'shared state, transactional claims, live supervision.', fill: hexA(C.white, 0.65) }], 140, 960, { t: at, size: 38, weight: 700, stagger: 0.008 });
  ctx.restore();
}
// ======================================================================
// 11 — OUTRO
// ======================================================================
let GLINT = null;
function sOutro(ctx, t) {
  const u = t - T.outro;
  if (u < 0) return;
  const mx = CX, my = 420;
  const sc = lerp(1.35, 1, spring(u, 1.2, 5.5));
  drawMark(ctx, mx, my, 330 * sc, { sweep: E.outExpo(P(u, 0, 0.65)), ring: E.outExpo(P(u, 0.06, 0.6)), glow: hexA(C.green, 0.9), glowBlur: 40 + 60 * pulse(u, 3), rot: -0.35 * (1 - E.outExpo(clamp(u / 0.9))) });
  const wt = u - 0.55;
  if (wt > 0) {
    const ww = 660, wh = (ww * 53) / 348;
    if (!GLINT) GLINT = createCanvas(ww + 40, Math.ceil(wh) + 20);
    const g = GLINT.getContext('2d');
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.globalCompositeOperation = 'source-over';
    g.clearRect(0, 0, GLINT.width, GLINT.height);
    g.drawImage(A.wordmark, 20, 10, ww, wh);
    const gp = P(u, 0.95, 1.55);
    if (gp > 0 && gp < 1) {
      g.globalCompositeOperation = 'source-atop';
      const gx = lerp(-200, ww + 240, E.inOutCubic(gp));
      const lg = g.createLinearGradient(gx - 120, 0, gx + 120, 0);
      lg.addColorStop(0, 'rgba(76,244,144,0)'); lg.addColorStop(0.5, 'rgba(76,244,144,1)'); lg.addColorStop(1, 'rgba(76,244,144,0)');
      g.fillStyle = lg;
      g.save(); g.transform(1, 0, -0.4, 1, 0, 0); g.fillRect(-100, 0, GLINT.width + 300, GLINT.height); g.restore();
    }
    const wp = E.outExpo(clamp(wt / 0.6));
    ctx.save();
    ctx.beginPath(); ctx.rect(CX - ww / 2 - 20, 600, (ww + 40) * wp, 200); ctx.clip();
    ctx.translate((1 - wp) * -40, 0);
    ctx.drawImage(GLINT, CX - ww / 2 - 20, 640);
    ctx.restore();
  }
  riseText(ctx, 'Development at the speed of light.', CX, 820, { t: u - 1.1, size: 40, weight: 500, align: 'center', stagger: 0.014, dur: 0.5, fill: hexA(C.white, 0.85), tracking: 0.5 });
  const ct = E.outExpo(P(u, 1.7, 2.2));
  if (ct > 0) {
    const s1 = 'spacetimedb.com', s2 = 'curl -sSf https://install.spacetimedb.com | sh';
    const w1 = measure(ctx, s1, 700, 22, MONO), w2 = measure(ctx, s2, 400, 18, MONO);
    const pw = w1 + w2 + 110, ph = 58;
    ctx.save();
    ctx.globalAlpha = ct;
    ctx.translate(CX, 915 + (1 - ct) * 30);
    rr(ctx, -pw / 2, -ph / 2, pw, ph, ph / 2);
    ctx.fillStyle = 'rgba(255,255,255,0.05)'; ctx.fill();
    ctx.strokeStyle = hexA(C.white, 0.2); ctx.lineWidth = 1.5; ctx.stroke();
    text(ctx, s1, -pw / 2 + 32, 8, { size: 22, weight: 700 });
    ctx.fillStyle = hexA(C.white, 0.2); ctx.fillRect(-pw / 2 + 32 + w1 + 22, -14, 1, 28);
    text(ctx, s2, -pw / 2 + 32 + w1 + 46, 7, { size: 18, weight: 400, fill: hexA(C.white, 0.65) });
    ctx.restore();
  }
}

// ======================================================================
// Frame
// ======================================================================
// t = scene time (0..DUR); rt = real on-screen time (differs only when slowed down)
function frame(ctx, t, rt = t) {
  if (!GRID) initStatic();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
  ctx.globalCompositeOperation = 'source-over';
  drawBackground(ctx, t);
  const [sx, sy] = shakeOffset(t, rt);
  ctx.save();
  ctx.translate(sx, sy);
  sHook(ctx, t);
  sStack(ctx, t);
  sCode(ctx, t);
  sRealtime(ctx, t);
  sSpeed(ctx, t);
  sScale(ctx, t);
  sCloud(ctx, t);
  sFeatures(ctx, t);
  sLangs(ctx, t);
  sAgents(ctx, t);
  sOutro(ctx, t);
  U.fxShock(ctx, t, SHOCKS);
  U.fxBursts(ctx, t, BURSTS);
  ctx.restore();
  let fl = 0;
  for (const f of FLASHES) if (t >= f.t) fl += f.a * pulse((t - f.t) * SLOW, f.k);
  fl += 0.9 * E.inExpo(P(t, K.zoom - 0.25 / SLOW, K.zoom)) * (t < K.zoom ? 1 : 0);
  fl += 0.5 * E.inExpo(P(t, K.outro - 0.2 / SLOW, K.outro)) * (t < K.outro ? 1 : 0);
  if (fl > 0.003) { ctx.fillStyle = `rgba(244,246,252,${clamp(fl)})`; ctx.fillRect(0, 0, W, H); }
  U.drawHUD(ctx, t, rt, { scenes: SCENES, dur: DUR, label: '/  SHOWREEL ’26' });
  const fo = P(t, DUR - 0.8, DUR);
  if (fo > 0) { ctx.fillStyle = `rgba(0,0,0,${E.inOutSine(fo)})`; ctx.fillRect(0, 0, W, H); }
}

module.exports = { frame, setSlow, SCENES, T, K, EVENTS, UP, DOWN };
