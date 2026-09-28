// Procedural soundtrack, 120 BPM, A minor — synced to the scene timeline.
const fs = require('fs');
const SR = 48000, LEN = require('./lib').DUR, N = Math.ceil((LEN + 0.5) * SR);
const TAU = Math.PI * 2;

function rng(seed) { let a = seed >>> 0; return () => { a |= 0; a = (a + 0x6d2b79f5) | 0; let t = Math.imul(a ^ (a >>> 15), 1 | a); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }; }
const R = rng(1234);
const noise = () => R() * 2 - 1;
const mtof = m => 440 * Math.pow(2, (m - 69) / 12);
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));

// buses (stereo)
const bus = () => [new Float32Array(N), new Float32Array(N)];
const DRUM = bus(), MUSIC = bus(), FX = bus(), SEND = bus();

class Biquad {
  constructor(type, f, q = 0.707) { this.type = type; this.x1 = this.x2 = this.y1 = this.y2 = 0; this.set(f, q); }
  set(f, q = this.q) {
    this.q = q; f = clamp(f, 10, SR * 0.45);
    const w = (TAU * f) / SR, c = Math.cos(w), s = Math.sin(w), al = s / (2 * q);
    let b0, b1, b2, a0 = 1 + al, a1 = -2 * c, a2 = 1 - al;
    if (this.type === 'lp') { b0 = (1 - c) / 2; b1 = 1 - c; b2 = (1 - c) / 2; }
    else if (this.type === 'hp') { b0 = (1 + c) / 2; b1 = -(1 + c); b2 = (1 + c) / 2; }
    else { b0 = al; b1 = 0; b2 = -al; }
    this.b0 = b0 / a0; this.b1 = b1 / a0; this.b2 = b2 / a0; this.a1 = a1 / a0; this.a2 = a2 / a0;
  }
  p(x) { const y = this.b0 * x + this.b1 * this.x1 + this.b2 * this.x2 - this.a1 * this.y1 - this.a2 * this.y2; this.x2 = this.x1; this.x1 = x; this.y2 = this.y1; this.y1 = y; return y; }
}

// render a voice: fn(τ, i) -> sample ; into bus with pan and optional send
function voice(B, t0, dur, fn, { g = 1, pan = 0, send = 0 } = {}) {
  const s0 = Math.floor(t0 * SR), n = Math.floor(dur * SR);
  const gl = g * Math.cos((pan + 1) * Math.PI / 4) * 1.414, gr = g * Math.sin((pan + 1) * Math.PI / 4) * 1.414;
  for (let i = 0; i < n; i++) {
    const k = s0 + i;
    if (k < 0 || k >= N) continue;
    const v = fn(i / SR, i);
    B[0][k] += v * gl; B[1][k] += v * gr;
    if (send) { SEND[0][k] += v * gl * send; SEND[1][k] += v * gr * send; }
  }
}

// ---------------- instruments ----------------
const KICKS = [];
function kick(t, g = 1) {
  KICKS.push(t);
  let ph = 0;
  voice(DRUM, t, 0.5, (τ) => {
    const f = 44 + 120 * Math.exp(-τ * 30);
    ph += (TAU * f) / SR;
    const body = Math.sin(ph) * Math.exp(-τ * 7);
    const click = noise() * Math.exp(-τ * 400) * 0.35;
    return Math.tanh((body + click) * 1.6);
  }, { g: 0.9 * g });
}
function clap(t, g = 1) {
  const bp = new Biquad('bp', 1500, 0.9);
  voice(DRUM, t, 0.35, (τ) => {
    let e = 0;
    for (const o of [0, 0.011, 0.022]) if (τ >= o) e += Math.exp(-(τ - o) * 190);
    if (τ >= 0.022) e += 0.5 * Math.exp(-(τ - 0.022) * 13);
    return bp.p(noise()) * e * 1.6;
  }, { g: 0.4 * g, send: 0.3 });
}
function hat(t, g = 1, open = false, pan = 0.2) {
  const hp = new Biquad('hp', 7500, 0.8);
  voice(DRUM, t, open ? 0.3 : 0.08, (τ) => hp.p(noise()) * Math.exp(-τ * (open ? 13 : 60)), { g: 0.16 * g, pan });
}
function crash(t, g = 1) {
  const hp = new Biquad('hp', 4000, 0.6);
  voice(FX, t, 2.5, (τ) => hp.p(noise()) * Math.exp(-τ * 1.8) * (0.6 + 0.4 * Math.sin(τ * 900)), { g: 0.16 * g, pan: -0.1, send: 0.4 });
  const hp2 = new Biquad('hp', 4200, 0.6);
  voice(FX, t, 2.5, (τ) => hp2.p(noise()) * Math.exp(-τ * 1.9), { g: 0.14 * g, pan: 0.4 });
}
function bass(t, dur, m, g = 1) {
  const f = mtof(m), lp = new Biquad('lp', 400, 1.2);
  let ph = 0, ph2 = 0;
  voice(MUSIC, t, dur, (τ) => {
    ph = (ph + f / SR) % 1; ph2 += (TAU * f) / SR;
    lp.set(160 + 900 * Math.exp(-τ * 14), 1.4);
    const env = Math.min(1, τ / 0.004) * Math.min(1, (dur - τ) / 0.02);
    return (lp.p(ph * 2 - 1) * 0.7 + Math.sin(ph2) * 0.6) * env;
  }, { g: 0.32 * g });
}
function pad(t, dur, notes, g = 1, att = 0.35, rel = 0.6, cut = 1600) {
  notes.forEach((m, ni) => {
    [-1, 0, 1].forEach(d => {
      const f = mtof(m) * Math.pow(2, (d * 8) / 1200);
      const lp = new Biquad('lp', cut, 0.7), lp2 = new Biquad('lp', cut, 0.7);
      let ph = R();
      voice(MUSIC, t, dur + rel, (τ) => {
        ph = (ph + f / SR) % 1;
        const env = Math.min(1, τ / att) * (τ > dur ? Math.max(0, 1 - (τ - dur) / rel) : 1);
        return lp2.p(lp.p(ph * 2 - 1)) * env;
      }, { g: 0.045 * g, pan: d * 0.6 + (ni - 1) * 0.1, send: 0.5 });
    });
  });
}
function pluck(t, m, g = 1, pan = 0) {
  const f = mtof(m), lp = new Biquad('lp', 3000, 1);
  let ph = 0;
  voice(MUSIC, t, 0.5, (τ) => {
    ph = (ph + f / SR) % 1;
    lp.set(600 + 5000 * Math.exp(-τ * 18), 1.5);
    const sq = ph < 0.5 ? 1 : -1, tri = 1 - 4 * Math.abs(ph - 0.5);
    return lp.p(sq * 0.4 + tri * 0.6) * Math.exp(-τ * 9);
  }, { g: 0.11 * g, pan, send: 0.55 });
}
function stab(t, notes, g = 1) {
  notes.forEach((m, i) => {
    const f = mtof(m), lp = new Biquad('lp', 3000, 1.2);
    let ph = R();
    voice(MUSIC, t, 0.45, (τ) => {
      ph = (ph + f / SR) % 1;
      lp.set(400 + 6000 * Math.exp(-τ * 12), 1.6);
      return lp.p(ph * 2 - 1) * Math.exp(-τ * 7);
    }, { g: 0.09 * g, pan: (i - 1) * 0.35, send: 0.5 });
  });
}
function blip(t, m, g = 1, pan = 0, dec = 30) {
  const f = mtof(m);
  voice(FX, t, 0.2, (τ) => Math.sin(TAU * f * τ) * Math.exp(-τ * dec) * Math.min(1, τ / 0.002), { g: 0.09 * g, pan, send: 0.3 });
}
function tick(t, g = 1, pan = 0) {
  const hp = new Biquad('hp', 3500, 0.8);
  voice(FX, t, 0.02, (τ) => hp.p(noise()) * Math.exp(-τ * 700), { g: 0.14 * g, pan });
}
function riser(t0, t1, g = 1) {
  const bp = new Biquad('bp', 400, 2.2);
  let ph = 0;
  const d = t1 - t0;
  voice(FX, t0, d, (τ, i) => {
    const p = τ / d;
    if (i % 32 === 0) bp.set(300 * Math.pow(30, p), 2.2);
    ph += (TAU * (180 * Math.pow(6, p))) / SR;
    return (bp.p(noise()) * 1.4 + Math.sin(ph) * 0.12) * p * p;
  }, { g: 0.32 * g, send: 0.3 });
}
function whoosh(t, d, g = 1, dir = 1) {
  const bp = new Biquad('bp', 500, 1.4);
  voice(FX, t, d, (τ, i) => {
    const p = τ / d;
    const bell = Math.sin(Math.PI * p);
    if (i % 32 === 0) bp.set(300 + 3200 * bell, 1.4);
    return bp.p(noise()) * bell * bell * 1.6;
  }, { g: 0.3 * g, pan: 0, send: 0.2 });
  // stereo motion via second, panned layer
  const bp2 = new Biquad('bp', 500, 1.4);
  voice(FX, t, d, (τ, i) => { const p = τ / d; const bell = Math.sin(Math.PI * p); if (i % 32 === 0) bp2.set(500 + 4000 * bell, 1.2); return bp2.p(noise()) * bell * bell; }, { g: 0.18 * g, pan: 0.7 * dir });
}
function impact(t, g = 1) {
  let ph = 0;
  voice(FX, t, 2.2, (τ) => { ph += (TAU * (28 + 50 * Math.exp(-τ * 4))) / SR; return Math.tanh(Math.sin(ph) * 1.8) * Math.exp(-τ * 2.2); }, { g: 0.55 * g });
  const lp = new Biquad('lp', 2500, 0.7);
  voice(FX, t, 1.6, (τ) => lp.p(noise()) * Math.exp(-τ * 4.5), { g: 0.45 * g, send: 0.7 });
  kick(t, 1.1 * g);
}
function reverseSwell(tEnd, d, g = 1) {
  const bp = new Biquad('bp', 2000, 0.8);
  voice(FX, tEnd - d, d, (τ) => { const p = τ / d; return bp.p(noise()) * Math.pow(p, 3); }, { g: 0.35 * g, send: 0.3 });
}
function glitch(t, d, g = 1) {
  let f = 200, ph = 0;
  voice(FX, t, d, (τ, i) => {
    if (i % 900 === 0) f = 80 + R() * 1800;
    ph = (ph + f / SR) % 1;
    const crush = Math.round((ph < 0.5 ? 1 : -1) * 3 + noise() * 2) / 4;
    return crush * Math.exp(-τ * 6);
  }, { g: 0.09 * g, pan: 0 });
}
function bell(t, m, g = 1) {
  const f = mtof(m);
  voice(FX, t, 2.0, (τ) => (Math.sin(TAU * f * τ) + 0.5 * Math.sin(TAU * f * 2.76 * τ) * Math.exp(-τ * 3) + 0.25 * Math.sin(TAU * f * 5.4 * τ) * Math.exp(-τ * 6)) * Math.exp(-τ * 2.4), { g: 0.08 * g, send: 0.6 });
}

// ---------------- harmony ----------------
const CH = {
  Am: { pad: [57, 60, 64, 71], bass: 33, arp: [69, 72, 76, 79, 81] },
  F: { pad: [53, 57, 60, 64], bass: 29, arp: [65, 69, 72, 76, 77] },
  C: { pad: [55, 60, 64, 67], bass: 36, arp: [67, 72, 76, 79, 84] },
  G: { pad: [55, 59, 62, 69], bass: 31, arp: [67, 71, 74, 79, 83] },
};
const PROG = ['Am', 'F', 'C', 'G'];
const chordAt = t => CH[t < 16 ? 'Am' : PROG[Math.floor((t - 16) / 2) % 4]];
const { T, EVENTS, UP, DOWN } = require('./scenes');
const PENT = [69, 72, 74, 76, 79, 81, 84, 86, 88];

function groove(a, b, o = {}) {
  for (let t = a; t < b - 1e-6; t += 0.5) {
    const half = o.half && Math.round(t * 2) % 2 === 1;
    if (!half) kick(t, o.kickG ?? 1);
    if (Math.round((t - 16) * 2) % 2 === 1 && !o.noClap) clap(t, o.clapG ?? 1);
    hat(t + 0.25, o.hatG ?? 1, true, 0.25);
    if (!o.noGhost) { hat(t + 0.125, 0.35, false, -0.3); hat(t + 0.375, 0.35, false, -0.3); }
    const c = chordAt(t);
    if (!o.noBass) for (let k = 0; k < 4; k++) { const tt = t + k * 0.125; if (tt < b) bass(tt, 0.11, c.bass + (k === 2 ? 12 : 0), o.bassG ?? 1); }
  }
  if (!o.noPad) for (let bt = a; bt < b - 1e-6; bt += 2) pad(bt, Math.min(2, b - bt), chordAt(bt).pad, o.padG ?? 1);
}
function typing(a, b, g = 0.4, seed = 5) { const r = rng(seed); for (let t = a; t < b; t += 0.035 + r() * 0.03) tick(t, g, r() - 0.5); }

// ===== 1 Hook 0–5
riser(0.0, 4.6, 0.5); whoosh(0.0, 0.7, 0.5, 1); bell(0.02, 69, 0.5);
pad(0.0, 4.4, [45, 52], 1.2, 1.5, 0.5, 700);
for (let i = 0; i < 9; i++) tick(0.3 + i * 0.06, 0.8, (i % 2 ? 0.3 : -0.3));
for (const t of [1.0, 2.0]) { kick(t, 1); impact(t, 0.25); stab(t, [57, 64, 69], 0.8); }
kick(3.0, 1.1); impact(3.0, 0.7); glitch(3.0, 0.3, 1.2); stab(3.0, [53, 60, 65], 1.1);
for (let t = 3.5; t < 4.5; t += 0.5) hat(t, 0.5, false, 0.2);
reverseSwell(5.0, 0.5, 0.8); whoosh(4.55, 0.45, 1, -1);

// ===== 2 Stack 5–16
{
  const S = T.stack;
  kick(S, 1); crash(S, 0.5);
  for (let t = S; t < S + 5.4; t += 1.0) kick(t, 0.8);
  for (let t = S; t < S + 5.4; t += 0.25) hat(t, t % 0.5 === 0 ? 0.6 : 1, false, 0.25);
  for (let t = S + 2.5; t < S + 5.4; t += 0.125) hat(t, 0.45, false, -0.25);
  for (let t = S; t < S + 5.4; t += 0.25) bass(t, 0.2, 33 + (Math.round((t - S) * 4) % 4 === 2 ? 12 : 0), 0.55);
  for (let i = 0; i < 9; i++) blip(S + 0.5 + i * 0.3, PENT[i], 1.2, (i % 3 - 1) * 0.5, 25);
  for (const u of [3.4, 3.7, 3.95, 4.2, 4.4]) { blip(S + u, 82, 0.8, 0.3, 40); blip(S + u + 0.05, 77, 0.8, 0.3, 40); }
  riser(S + 4.6, S + 6.0, 0.9); whoosh(S + 5.5, 0.5, 0.8);
  impact(S + 6.0, 1.1); crash(S + 6.0, 1);
  pad(S + 6.0, 3.5, CH.Am.pad, 1.0, 0.3, 0.6);
  for (let t = S + 7.0; t < S + 9.5; t += 0.5) { hat(t + 0.25, 0.6, true, 0.2); if (Math.round((t - S) * 2) % 2 === 0) kick(t, 0.55); }
  for (let t = S + 9.0, i = 0; t < S + 10.0; t += 0.0625, i++) clap(t, 0.08 + i * 0.03);
  whoosh(S + 9.6, 0.45, 0.7, 1);
}

// ===== 3 Code 16–28
{
  const S = T.code;
  groove(S, S + 11.6);
  crash(S, 0.8);
  for (let i = 0; i < 14; i++) tick(S + 0.6 + i * 0.17, 1, 0.2);
  typing(S + 0.6, S + 2.9, 0.35, 5);
  typing(S + 3.4, S + 3.9, 0.5, 6);
  blip(S + 4.05, 76, 1, 0, 20); blip(S + 4.3, 81, 1, 0, 20);
  whoosh(S + 4.5, 0.5, 0.9, 1); impact(S + 5.0, 0.45); bell(S + 5.0, 81, 0.8);
  typing(S + 5.9, S + 6.5, 0.5, 7);
  blip(S + 6.6, 84, 0.9, 0, 20); whoosh(S + 6.6, 0.4, 0.6, -1);
  typing(S + 7.1, S + 8.9, 0.35, 8);
  whoosh(S + 11.55, 0.5, 0.8, -1);
}

// ===== 4 Real-time 28–37
{
  const S = T.rt;
  groove(S, S + 8.5);
  let k = 0;
  for (const e of EVENTS) {
    const c = chordAt(e.t);
    pluck(e.t, c.arp[k % c.arp.length], 0.9, ((k % 5) - 2) * 0.3);
    blip(e.t + UP + DOWN, c.arp[(k + 2) % c.arp.length] + 12, 0.3, ((k % 3) - 1) * 0.6, 40);
    k++;
  }
  riser(S + 7.4, S + 9.0, 1.2);
  for (let t = S + 8.5, i = 0; t < S + 9.0; t += 0.0625, i++) clap(t, 0.2 + i * 0.1);
  reverseSwell(S + 9.0, 0.5, 1);
  impact(S + 9.0, 1.2); crash(S + 9.0, 1.1);
}

// ===== 5 Speed 37–45
{
  const S = T.speed;
  kick(S, 1.1); crash(S, 0.7);
  let last = -1, lt = 0;
  for (let t = S + 0.05; t < S + 2.0; t += 1 / 480) {
    const cp = (t - S - 0.05) / 1.95, v = 303920 * (1 - Math.pow(2, -10 * cp));
    const step = Math.floor(v / 2500);
    if (step !== last && t - lt > 0.03) { tick(t, 0.8, (R() - 0.5) * 0.6); blip(t, 60 + Math.min(36, (v / 303920) * 36), 0.25, 0, 60); last = step; lt = t; }
  }
  pad(S, 2.0, [45, 57, 64], 1.1, 0.6, 0.3, 900);
  bell(S + 2.0, 81, 1.2); bell(S + 2.0, 88, 0.6);
  groove(S + 2.0, S + 7.2);
  riser(S + 3.4, S + 4.0, 0.7); whoosh(S + 3.8, 0.3, 0.9, 1); impact(S + 4.0, 0.6); crash(S + 4.0, 0.7);
  whoosh(S + 7.15, 0.45, 1.1, -1);
}

// ===== 6 Scale 45–57 (half-time, heavier)
{
  const S = T.scale;
  groove(S, S + 6.4, { half: true, noGhost: true, clapG: 0.7, bassG: 0.7 });
  blip(S + 2.2, 57, 1.2, 0, 8); blip(S + 2.2, 58, 1.0, 0, 8); impact(S + 2.2, 0.35);
  whoosh(S + 2.7, 0.35, 0.8, 1); stab(S + 3.0, [69, 76, 81], 1.2); crash(S + 3.0, 0.6); kick(S + 3.0, 1.1);
  whoosh(S + 5.95, 0.5, 0.8, -1);
  impact(S + 6.6, 0.45); bell(S + 6.6, 76, 0.8);
  groove(S + 6.5, S + 11.3);
  for (let ring = 1; ring <= 3; ring++) for (let i = 0; i < 6 * ring; i++) blip(S + 6.4 + 0.2 + (ring - 1) * 0.55 + i * 0.025, PENT[(i + ring * 2) % 9], 0.18, ((i % 5) - 2) * 0.3, 45);
  bell(S + 8.8, 81, 0.6);
  riser(S + 10.0, S + 12.0, 1.4);
  reverseSwell(S + 12.0, 1.0, 1.3);
  for (let t = S + 11.3, i = 0; t < S + 11.8; t += 0.0625, i++) clap(t, 0.15 + i * 0.08);
}

// ===== 7 Maincloud 57–64
{
  const S = T.cloud;
  crash(S, 0.6);
  groove(S, S + 6.7);
  typing(S + 0.6, S + 0.95, 0.5, 9); blip(S + 1.05, 81, 0.9, 0, 20);
  typing(S + 1.3, S + 2.0, 0.5, 10); blip(S + 2.1, 84, 1.0, 0, 20); impact(S + 2.1, 0.35); bell(S + 2.1, 88, 0.5);
  for (let i = 0; i < 4; i++) blip(S + 2.4 + i * 0.15, PENT[3 + i], 0.5, (i - 1.5) * 0.3, 30);
  whoosh(S + 6.65, 0.45, 1.0, -1);
}

// ===== 8 Features 64–71
{
  const S = T.feat;
  groove(S, S + 6.2);
  for (let i = 0; i < 6; i++) { blip(S + 0.4 + i * 0.2, PENT[i + 1], 0.6, (i % 3 - 1) * 0.5, 30); pluck(S + 1.9 + i * 0.7, chordAt(S + 1.9 + i * 0.7).arp[i % 5] + 12, 0.8, (i % 3 - 1) * 0.5); }
  whoosh(S + 6.15, 0.45, 1.0, -1);
}

// ===== 9 Languages 71–77
{
  const S = T.langs;
  const STABS = [[57, 64, 69], [60, 64, 72], [64, 67, 76], [65, 69, 77]];
  [0.1, 0.6, 1.1, 1.6].forEach((u, i) => { kick(S + u, 1.1); clap(S + u, 0.8); stab(S + u, STABS[i], 1.3); crash(S + u, 0.25); });
  whoosh(S + 1.95, 0.35, 0.6, 1);
  groove(S + 2.1, S + 5.7, { padG: 0.9 });
  for (let i = 0; i < 14; i++) { const c = i % 7, r = Math.floor(i / 7); blip(S + 2.35 + (c + r * 1.5) * 0.05, PENT[(c + r * 2) % 9], 0.6, (c - 3) * 0.2, 35); }
  for (let i = 0; i < 8; i++) blip(S + 3.9 + i * 0.05, 81 + [0, 3, 7, 10, 12, 15, 19, 22][i], 0.35, (i - 4) * 0.2, 25);
  whoosh(S + 5.7, 0.45, 1.0, 1);
}

// ===== 10 AI 77–85
{
  const S = T.ai;
  groove(S, S + 9.2, { bassG: 0.8 });
  crash(S, 0.6);
  typing(S + 0.4, S + 1.3, 0.45, 11);
  for (const u of [1.5, 1.85, 2.2, 2.55, 2.9]) { blip(S + u + 0.2, 84, 0.7, 0.2, 30); blip(S + u + 0.24, 88, 0.5, 0.2, 30); }
  for (let i = 0; i < 9; i++) { whoosh(S + 3.1 + i * 0.08, 0.2, 0.18, (i % 3) - 1); blip(S + 3.1 + i * 0.08 + 0.35, PENT[i], 0.35, ((i % 3) - 1) * 0.6, 40); }
  typing(S + 4.3, S + 4.8, 0.45, 12);
  whoosh(S + 6.6, 0.35, 0.5, -1);
  whoosh(S + 9.15, 0.45, 1.1, -1);
}

// ===== 11 Outro 85–90.5
{
  const S = T.outro;
  impact(S, 1.4); crash(S, 1.3);
  pad(S, 1.25, CH.F.pad.concat([53]), 1.5, 0.05, 0.6);
  pad(S + 1.25, 1.25, CH.G.pad.concat([55]), 1.4, 0.2, 0.6);
  pad(S + 2.5, 2.2, CH.Am.pad.concat([57, 76]), 1.5, 0.2, 1.4);
  bass(S, 1.25, 29, 0.8); bass(S + 1.25, 1.25, 31, 0.8); bass(S + 2.5, 2.0, 33, 0.8);
  for (let i = 0; i < 12; i++) blip(S + 0.55 + i * 0.045, [81, 84, 88, 91, 93, 96][i % 6], 0.4, (i % 5 - 2) * 0.3, 18);
  bell(S + 1.1, 76, 0.9); bell(S + 1.1, 81, 0.6);
  bell(S + 1.7, 84, 0.6); blip(S + 1.7, 93, 0.5, 0, 12);
}

// ---------------- reverb (Freeverb) ----------------
function freeverb(inL, inR, room = 0.86, damp = 0.35, spread = 23) {
  const combT = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617].map(x => Math.round(x * SR / 44100));
  const apT = [556, 441, 341, 225].map(x => Math.round(x * SR / 44100));
  const outL = new Float32Array(N), outR = new Float32Array(N);
  for (const [inp, out, off] of [[inL, outL, 0], [inR, outR, spread]]) {
    const combs = combT.map(n => ({ b: new Float32Array(n + off), i: 0, s: 0 }));
    const aps = apT.map(n => ({ b: new Float32Array(n + off), i: 0 }));
    for (let k = 0; k < N; k++) {
      const x = inp[k] * 0.015;
      let y = 0;
      for (const c of combs) {
        const o = c.b[c.i];
        c.s = o * (1 - damp) + c.s * damp;
        c.b[c.i] = x + c.s * room;
        c.i = (c.i + 1) % c.b.length;
        y += o;
      }
      for (const a of aps) { const o = a.b[a.i]; const v = -y + o; a.b[a.i] = y + o * 0.5; a.i = (a.i + 1) % a.b.length; y = v; }
      out[k] = y;
    }
  }
  return [outL, outR];
}
const [RL, RR] = freeverb(SEND[0], SEND[1]);

// ---------------- sidechain + master ----------------
KICKS.sort((a, b) => a - b);
const out = [new Float32Array(N), new Float32Array(N)];
let ki = 0, lastK = -9;
let peak = 0;
const hpL = new Biquad('hp', 25, 0.7), hpR = new Biquad('hp', 25, 0.7);
for (let k = 0; k < N; k++) {
  const t = k / SR;
  while (ki < KICKS.length && KICKS[ki] <= t) lastK = KICKS[ki++];
  const sc = 1 - 0.65 * Math.exp(-(t - lastK) * 9);
  const fade = t > LEN - 0.7 ? Math.max(0, 1 - (t - (LEN - 0.7)) / 0.7) : 1;
  const fin = Math.min(1, t / 0.01);
  for (let c = 0; c < 2; c++) {
    let v = DRUM[c][k] + MUSIC[c][k] * sc + FX[c][k] + (c ? RR[k] : RL[k]) * 0.9 * (0.5 + 0.5 * sc);
    v = (c ? hpR : hpL).p(v);
    v = Math.tanh(v * 0.7) * fade * fin;
    out[c][k] = v;
    peak = Math.max(peak, Math.abs(v));
  }
}
const gain = 0.93 / peak;
const S = Math.floor(LEN * SR);
const buf = Buffer.alloc(44 + S * 4);
buf.write('RIFF', 0); buf.writeUInt32LE(36 + S * 4, 4); buf.write('WAVE', 8); buf.write('fmt ', 12);
buf.writeUInt32LE(16, 16); buf.writeUInt16LE(1, 20); buf.writeUInt16LE(2, 22); buf.writeUInt32LE(SR, 24); buf.writeUInt32LE(SR * 4, 28); buf.writeUInt16LE(4, 32); buf.writeUInt16LE(16, 34);
buf.write('data', 36); buf.writeUInt32LE(S * 4, 40);
for (let k = 0; k < S; k++) for (let c = 0; c < 2; c++) buf.writeInt16LE(Math.round(clamp(out[c][k] * gain, -1, 1) * 32767), 44 + k * 4 + c * 2);
fs.writeFileSync(__dirname + '/out/reel.wav', buf);
console.log('wrote reel.wav, peak', peak.toFixed(3));
