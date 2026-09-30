// Shared soundtrack skeleton for the release videos (120 BPM, A minor): the groove, the intro and outro
// cues, scene transitions and the card-grid cue. Each reel's audio.js adds its own scene cues on top.
const { createSynth } = require('../synth');

const CH = {
  Am: { pad: [57, 60, 64, 71], bass: 33, arp: [69, 72, 76, 79, 81] },
  F: { pad: [53, 57, 60, 64], bass: 29, arp: [65, 69, 72, 76, 77] },
  C: { pad: [55, 60, 64, 67], bass: 36, arp: [67, 72, 76, 79, 84] },
  G: { pad: [55, 59, 62, 69], bass: 31, arp: [67, 71, 74, 79, 83] },
};
const PROG = ['Am', 'F', 'C', 'G'];
const PENT = [69, 72, 74, 76, 79, 81, 84, 86, 88];

// T: scene starts (intro: 0, outro); scenes: [{t, cards?}] for every scene between intro and outro, in order.
function createReleaseAudio({ DUR, T, seed }) {
  const S = createSynth(DUR, seed);
  const { rng, kick, clap, hat, crash, bass, pad, pluck, stab, blip, tick, riser, whoosh, impact, reverseSwell, bell } = S;
  const K = { ver: T.intro + 2.5, outro: T.outro };
  const chordAt = t => CH[t < K.ver ? 'Am' : PROG[Math.floor((t - K.ver) / 2) % 4]];
  const offbeat = t => Math.round((t - K.ver) * 2) % 2 !== 0;
  const onBeat = t => K.ver + Math.ceil((t - K.ver) * 2 - 1e-6) / 2; // next beat at or after t

  function groove(a, b, o = {}) {
    for (let t = onBeat(a); t < b - 1e-6; t += 0.5) {
      if (!(o.half && offbeat(t))) kick(t, o.kickG ?? 1);
      if (offbeat(t) && !o.noClap) clap(t, o.clapG ?? 1);
      hat(t + 0.25, o.hatG ?? 1, true, 0.25);
      if (!o.noGhost) { hat(t + 0.125, 0.35, false, -0.3); hat(t + 0.375, 0.35, false, -0.3); }
      const c = chordAt(t);
      if (!o.noBass) for (let k = 0; k < 4; k++) { const tt = t + k * 0.125; if (tt < b) bass(tt, 0.11, c.bass + (k === 2 ? 12 : 0), o.bassG ?? 1); }
    }
    if (!o.noPad) for (let bt = onBeat(a); bt < b - 1e-6; bt += 2) pad(bt, Math.min(2, b - bt), chordAt(bt).pad, o.padG ?? 1);
  }
  const typing = (a, b, g = 0.4, s = 5) => { const r = rng(s); for (let t = a; t < b; t += 0.035 + r() * 0.03) tick(t, g, r() - 0.5); };
  const transition = next => whoosh(next - 0.45, 0.45, 1.0, -1);
  // a user message / a result landing / a tool call going out and back
  const pop = t => { blip(t, 76, 0.6, 0.3, 30); blip(t + 0.05, 81, 0.35, 0.3, 30); };
  const done = (t, i = 0) => { blip(t, PENT[2 + (i % 6)], 0.8, 0, 25); blip(t + 0.04, PENT[2 + (i % 6)] + 12, 0.3, 0, 25); };
  const fail = t => { blip(t, 60, 0.5, -0.4, 40); blip(t + 0.1, 58, 0.4, -0.4, 40); };

  function intro(firstScene) {
    riser(0.0, K.ver, 0.6); whoosh(0.0, 0.7, 0.5, 1); bell(0.02, 69, 0.5);
    pad(0.0, K.ver, [45, 52], 1.2, 1.5, 0.3, 700);
    for (let i = 0; i < 9; i++) tick(0.3 + i * 0.06, 0.8, (i % 2 ? 0.3 : -0.3));
    whoosh(0.7, 0.5, 0.4, -1);
    kick(1.2, 0.9); stab(1.2, [57, 64, 69], 0.7);
    for (let i = 0; i < 6; i++) tick(2.05 + i * 0.07, 1.0, 0); // odometer roll
    impact(K.ver, 1.0); crash(K.ver, 0.9); stab(K.ver, [57, 64, 69, 76], 1.2); bell(K.ver, 81, 0.8);
    groove(K.ver + 0.5, firstScene - 0.5, { half: true, noGhost: true, clapG: 0.6, bassG: 0.6 });
    bell(T.intro + 2.9, 76, 0.4);
    reverseSwell(firstScene, 0.5, 0.8); whoosh(firstScene - 0.45, 0.45, 1, -1);
  }
  // A scene's bed: crash, groove to the next scene, transition into it.
  function scene(start, next, o = {}) {
    crash(start, o.crash ?? 0.5);
    groove(start, next - 0.5, o);
    if (next !== T.outro) transition(next);
  }
  function cards(start, next, n = 4) {
    for (let i = 0; i < n; i++) { whoosh(start + 0.4 + i * 0.22, 0.25, 0.3, i - 1.5); blip(start + 0.8 + i * 0.22, PENT[(2 + i * 2) % PENT.length], 0.7, (i - 1.5) * 0.4, 30); }
  }
  function outro() {
    const S0 = T.outro;
    riser(S0 - 2.0, S0, 1.2);
    for (let t = S0 - 0.5, i = 0; t < S0; t += 0.0625, i++) clap(t, 0.15 + i * 0.08);
    reverseSwell(S0, 0.6, 1.1);
    impact(S0, 1.4); crash(S0, 1.3);
    pad(S0, 1.25, CH.F.pad.concat([53]), 1.5, 0.05, 0.6);
    pad(S0 + 1.25, 1.25, CH.G.pad.concat([55]), 1.4, 0.2, 0.6);
    pad(S0 + 2.5, 3.0, CH.Am.pad.concat([57, 76]), 1.5, 0.2, 1.5);
    bass(S0, 1.25, 29, 0.8); bass(S0 + 1.25, 1.25, 31, 0.8); bass(S0 + 2.5, 2.5, 33, 0.8);
    for (let i = 0; i < 12; i++) blip(S0 + 0.55 + i * 0.045, [81, 84, 88, 91, 93, 96][i % 6], 0.4, (i % 5 - 2) * 0.3, 18);
    bell(S0 + 1.1, 76, 0.9); bell(S0 + 1.1, 81, 0.6);
    bell(S0 + 1.8, 84, 0.6); blip(S0 + 1.8, 93, 0.5, 0, 12);
  }
  return { ...S, K, CH, PENT, chordAt, groove, typing, transition, pop, done, fail, intro, scene, cards, outro };
}

module.exports = { createReleaseAudio };
