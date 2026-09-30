// Soundtrack for the v2.3 release video, keyed to the scene timeline (see ../release-audio.js).
const { DUR, T, GD, PL } = require('./scenes');
const A = require('../release-audio').createReleaseAudio({ DUR, T, seed: 2300 });
const { typing, done, tick, blip, stab, bell, pluck, chordAt, PENT } = A;

A.intro(T.godot);
{ // Godot: package added, the game starts
  const S = T.godot;
  A.scene(S, T.pipe);
  typing(S + GD.cmd[0], S + GD.cmd[1], 0.4, 81);
  done(S + GD.added, 2);
  stab(S + GD.play, [57, 64, 69, 76], 0.7); bell(S + GD.play, 81, 0.5);
  for (let i = 0; i < 6; i++) blip(S + GD.play + 0.4 + i * 0.35, PENT[(i * 2) % 8], 0.3, (i % 3 - 1) * 0.5, 40); // eating food
}
{ // pipelining: slow ticks in the old lane, a quick stream in the new one
  const S = T.pipe;
  A.scene(S, T.plus);
  for (let t = S + PL.start; t < S + PL.end; t += 1.5) tick(t, 0.8, -0.4);
  for (let t = S + PL.start, i = 0; t < S + PL.end; t += 0.15, i++) tick(t, 0.35, 0.4);
  for (let t = S + PL.start + 0.7, i = 0; t < S + PL.end; t += 0.6, i++) pluck(t, chordAt(t).arp[i % 5], 0.35, 0.3);
}
A.scene(T.plus, T.outro); A.cards(T.plus, T.outro, 3);
A.outro();
require('fs').mkdirSync(__dirname + '/out', { recursive: true });
A.finish(__dirname + '/out/reel.wav');
