// Soundtrack for the v2.2 release video, keyed to the scene timeline (see ../release-audio.js).
const { DUR, T, DR, CL } = require('./scenes');
const A = require('../release-audio').createReleaseAudio({ DUR, T, seed: 2200 });
const { typing, pop, done, fail, whoosh, stab, bell, pluck, tick, chordAt } = A;

A.intro(T.drop);
{ // remove tables: three steps, the old publish fails, the new one drops the table
  const S = T.drop;
  A.scene(S, T.cli);
  DR.steps.forEach((s, i) => pluck(S + s, chordAt(S + s).arp[i + 1], 0.6, (i - 1) * 0.5));
  fail(S + DR.out); done(S + DR.out + 0.1, 3);
  whoosh(S + DR.gone, 0.5, 0.4, 1); bell(S + DR.gone + 0.3, 81, 0.45);
}
{ // safer CLI: list, delete asks, Enter aborts, CI line
  const S = T.cli;
  A.scene(S, T.apps);
  typing(S + CL.list[0], S + CL.list[1], 0.4, 91); pop(S + CL.rows);
  typing(S + CL.del[0], S + CL.del[1], 0.4, 92); blip(S + CL.ask);
  tick(S + CL.enter, 1.2, 0); stab(S + CL.abort, [57, 64, 69], 0.6);
  typing(S + CL.ci[0], S + CL.ci[1], 0.4, 93); done(S + CL.ciNote, 4);
  function blip(t) { A.blip(t, 64, 0.6, 0, 20); A.blip(t + 0.08, 60, 0.4, 0, 20); }
}
A.scene(T.apps, T.outro); A.cards(T.apps, T.outro);
A.outro();
require('fs').mkdirSync(__dirname + '/out', { recursive: true });
A.finish(__dirname + '/out/reel.wav');
