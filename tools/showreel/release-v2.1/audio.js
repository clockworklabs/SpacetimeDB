// Soundtrack for the v2.1 release video, keyed to the scene timeline (see ../release-audio.js).
const { DUR, T, RB, HT, UR } = require('./scenes');
const A = require('../release-audio').createReleaseAudio({ DUR, T, seed: 2100 });
const { pop, done, fail, whoosh, stab, bell, pluck, impact, tick, blip, chordAt } = A;

A.intro(T.rust);
{ // Rust in the browser: both windows go live, a row lands in both
  const S = T.rust;
  A.scene(S, T.http);
  bell(S + 1.0, 81, 0.4);
  stab(S + RB.live, [57, 64, 69], 0.6);
  pop(S + RB.row); blip(S + RB.row + 0.05, 84, 0.4, 0.5, 30);
}
{ // HTTP timeouts: the old call times out, the new one waits for the answer
  const S = T.http;
  A.scene(S, T.unreal);
  fail(S + HT.run[0] + 0.15);
  for (let i = 0; i < 6; i++) tick(S + HT.run[0] + 0.4 + i * 0.4, 0.4, 0.3);
  const at = S + HT.run[0] + (HT.run[1] - HT.run[0]) * (HT.reply / 16);
  done(at, 3); bell(at, 84, 0.5);
}
{ // Unreal: protocol switches, a blob is eaten, the event pops
  const S = T.unreal;
  A.scene(S, T.cli);
  whoosh(S + 1.6, 0.4, 0.4, 1); done(S + 2.0, 2);
  impact(S + UR.eat, 0.35); stab(S + UR.pop, [57, 64, 69, 76], 0.6); bell(S + UR.pop, 81, 0.5);
}
A.scene(T.cli, T.more); A.cards(T.cli, T.more);
A.scene(T.more, T.outro, { crash: 0.4 }); A.cards(T.more, T.outro, 3);
A.outro();
require('fs').mkdirSync(__dirname + '/out', { recursive: true });
A.finish(__dirname + '/out/reel.wav');
