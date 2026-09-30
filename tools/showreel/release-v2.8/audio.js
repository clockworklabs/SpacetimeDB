// Soundtrack for the v2.8 release video, keyed to the scene timeline (see ../release-audio.js).
const { DUR, T, SM, TB } = require('./scenes');
const A = require('../release-audio').createReleaseAudio({ DUR, T, seed: 2800 });
const { typing, pop, done, fail, whoosh, stab, bell, pluck, impact, pad, chordAt } = A;

A.intro(T.sub);
{ // submodules: code types in, the library snaps into its slot, names appear
  const S = T.sub;
  A.scene(S, T.tab);
  typing(S + SM.type[0], S + SM.type[1], 0.35, 41);
  whoosh(S + 1.2, SM.snap - 1.2, 0.5, 1); impact(S + SM.snap, 0.4); stab(S + SM.snap, [57, 64, 69, 76], 0.8); bell(S + SM.snap + 0.1, 81, 0.5);
  for (let i = 0; i < 3; i++) pluck(S + SM.names + i * 0.2, chordAt(S + SM.names).arp[i + 1], 0.6, (i - 1) * 0.4);
}
{ // tabs: away (quiet), back, one lane recovers
  const S = T.tab;
  A.crash(S, 0.5);
  A.groove(S, S + TB.away, { bassG: 0.85 });
  pad(S + TB.away, TB.back - TB.away, [45, 52, 57], 0.8, 0.1, 0.4, 600);
  whoosh(S + TB.back - 0.4, 0.4, 0.6, 1); pop(S + TB.back);
  fail(S + TB.fix); done(S + TB.fix + 0.1, 4); bell(S + TB.fix + 0.3, 84, 0.5);
  A.groove(S + TB.back, T.outro - 0.5);
}
A.outro();
require('fs').mkdirSync(__dirname + '/out', { recursive: true });
A.finish(__dirname + '/out/reel.wav');
