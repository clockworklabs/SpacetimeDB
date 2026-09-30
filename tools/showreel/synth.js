// Synth instruments, buses, reverb and master shared by every reel's soundtrack.
// createSynth(LEN) returns the instruments; call finish(outPath) after arranging to write the wav.
// All randomness comes from one seeded generator, so a given arrangement renders identically.
const fs = require('fs');
const SR = 48000;
const TAU = Math.PI * 2;
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));

function createSynth(LEN, seed = 1234) {
  const N = Math.ceil((LEN + 0.5) * SR);
  function rng(seed) { let a = seed >>> 0; return () => { a |= 0; a = (a + 0x6d2b79f5) | 0; let t = Math.imul(a ^ (a >>> 15), 1 | a); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }; }
  const R = rng(seed);
  const noise = () => R() * 2 - 1;
  const mtof = m => 440 * Math.pow(2, (m - 69) / 12);

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

  // ---------------- sidechain + master ----------------
  function finish(outPath) {
    const [RL, RR] = freeverb(SEND[0], SEND[1]);
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
    fs.writeFileSync(outPath, buf);
    console.log('wrote', require('path').basename(outPath) + ', peak', peak.toFixed(3));
  }

  return { SR, N, R, rng, noise, mtof, voice, Biquad, kick, clap, hat, crash, bass, pad, pluck, stab, blip, tick, riser, whoosh, impact, reverseSwell, glitch, bell, finish };
}

module.exports = { createSynth, SR };
