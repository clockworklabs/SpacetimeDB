// Soundtrack for the v2.6 release video, keyed to the scene timeline (see ../release-audio.js).
const { DUR, T, RC, CQ } = require('./scenes');
const A = require('../release-audio').createReleaseAudio({ DUR, T, seed: 2600 });
const { typing, pop, done, fail, whoosh, stab, bell, pluck, impact, pad, tick, chordAt } = A;

A.intro(T.recon);
{ // React: the connection drops (drums cut), two failed retries, then back
  const S = T.recon;
  A.crash(S, 0.5);
  A.groove(S, S + RC.drop, { bassG: 0.85 });
  impact(S + RC.drop, 0.5); fail(S + RC.drop);
  pad(S + RC.drop, RC.tries[2][0] - RC.drop, [45, 52, 57], 0.8, 0.1, 0.4, 600);
  RC.tries.forEach(([at, ok], i) => ok ? done(S + at, 3) : fail(S + at));
  stab(S + RC.tries[2][0], [57, 64, 69], 0.7);
  A.groove(S + RC.tries[2][0], T.cpp - 0.5);
  pop(S + RC.msg); bell(S + RC.msg, 84, 0.4);
  A.transition(T.cpp);
}
{ // C++: code types in, the build goes green, rows arrive
  const S = T.cpp;
  A.scene(S, T.plus);
  typing(S + CQ.type[0], S + CQ.type[1], 0.35, 31);
  done(S + CQ.built, 2); stab(S + CQ.built, [57, 64, 69, 76], 0.6);
  for (let i = 0; i < 3; i++) pluck(S + CQ.built + 0.2 + i * 0.12, chordAt(S + CQ.built).arp[i + 1], 0.6, (i - 1) * 0.4);
  for (let i = 0; i < 4; i++) tick(S + 5.0 + i * 0.1, 0.6, 0);
}
A.scene(T.plus, T.outro); A.cards(T.plus, T.outro, 1);
A.outro();
require('fs').mkdirSync(__dirname + '/out', { recursive: true });
A.finish(__dirname + '/out/reel.wav');
