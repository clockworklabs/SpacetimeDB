// Procedural soundtrack, 120 BPM, A minor — synced to the scene timeline.
const LEN = require('./lib').DUR;
const { R, rng, mtof, kick, clap, hat, crash, bass, pad, pluck, stab, blip, tick, riser, whoosh, impact, reverseSwell, glitch, bell, finish } = require('./synth').createSynth(LEN);

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

finish(__dirname + '/out/reel.wav');
