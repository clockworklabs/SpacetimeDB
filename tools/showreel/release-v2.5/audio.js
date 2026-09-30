// Soundtrack for the v2.5 release video, keyed to the scene timeline (see ../release-audio.js).
const { DUR, T, PR, SO, VW } = require('./scenes');
const A = require('../release-audio').createReleaseAudio({ DUR, T, seed: 2500 });
const { typing, pop, done, fail, tick, whoosh, stab, bell, pluck, blip, chordAt, PENT } = A;

A.intro(T.proc);
{ // procedures: the flag is struck and removed, the build goes green
  const S = T.proc;
  A.scene(S, T.solid);
  blip(S + PR.badge, 60, 0.5, 0, 30); whoosh(S + PR.badge, 0.8, 0.4, -1);
  for (let i = 0; i < 4; i++) tick(S + PR.strike + i * 0.07, 0.7, 0.2);
  whoosh(S + PR.gone, 0.4, 0.4, 1);
  typing(S + PR.build, S + PR.build + 0.3, 0.3, 11);
  done(S + PR.built, 2); stab(S + PR.built, [57, 64, 69], 0.6);
  for (let i = 0; i < 3; i++) pluck(S + 3.6 + i * 0.15, chordAt(S + 3.6).arp[i + 1], 0.55, (i - 1) * 0.4);
}
{ // SolidJS: command typed, app runs, a name syncs to the second window
  const S = T.solid;
  A.scene(S, T.views);
  typing(S + SO.type[0], S + SO.type[1], 0.4, 21);
  done(S + SO.ran, 1);
  typing(S + SO.name[0], S + SO.name[1], 0.35, 22);
  pop(S + SO.click); whoosh(S + SO.click, SO.sync - SO.click + 0.1, 0.3, 1); bell(S + SO.sync, 84, 0.5);
  stab(S + 5.3, [57, 64, 69, 76], 0.6);
}
{ // views: delete + insert on the left, one update on the right
  const S = T.views;
  A.scene(S, T.plus);
  pop(S + 1.1);
  fail(S + VW.change + 0.1); blip(S + VW.change + 0.8, 64, 0.4, -0.4, 40);
  done(S + VW.change + 0.25, 4); bell(S + VW.change + 0.25, 81, 0.5);
}
A.scene(T.plus, T.outro); A.cards(T.plus, T.outro, 3);
A.outro();
require('fs').mkdirSync(__dirname + '/out', { recursive: true });
A.finish(__dirname + '/out/reel.wav');
