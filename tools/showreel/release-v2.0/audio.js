// Soundtrack for the v2.0 release video, keyed to the scene timeline (see ../release-audio.js).
const { DUR, T, EV, AW, CF } = require('./scenes');
const A = require('../release-audio').createReleaseAudio({ DUR, T, seed: 2000 });
const { typing, pop, done, fail, whoosh, stab, bell, pluck, impact, tick, blip, chordAt, PENT } = A;

A.intro(T.events);
{ // event tables: each insert pops on all three clients
  const S = T.events;
  A.scene(S, T.await);
  EV.fire.forEach((f, i) => { impact(S + f, 0.25); for (let c = 0; c < 3; c++) blip(S + f + c * 0.05, PENT[3 + i] + c * 2, 0.45, (c - 1) * 0.6, 30); });
}
{ // await: one call resolves, one rejects; the peer view
  const S = T.await;
  A.scene(S, T.config);
  pop(S + AW.ok); done(S + AW.okDone, 2);
  pop(S + AW.err); fail(S + AW.errDone);
  bell(S + AW.peer, 81, 0.45);
}
{ // spacetime.json: long commands collapse; dev brings everything up
  const S = T.config;
  A.scene(S, T.tmpl);
  for (let i = 0; i < 4; i++) tick(S + CF.strike + i * 0.07, 0.7, 0);
  whoosh(S + CF.short - 0.3, 0.3, 0.5, -1); stab(S + CF.short, [57, 64, 69], 0.6);
  typing(S + CF.dev[0], S + CF.dev[1], 0.4, 101);
  [CF.l1, CF.l2, CF.l3].forEach((t, i) => done(S + t, i));
  bell(S + CF.app, 84, 0.5);
}
{ // templates: logos pop in
  const S = T.tmpl;
  A.scene(S, T.secure);
  for (let i = 0; i < 10; i++) pluck(S + 0.5 + i * 0.12, chordAt(S + 0.5 + i * 0.12).arp[i % 5], 0.4, ((i % 5) - 2) * 0.3);
  stab(S + 3.0, [57, 64, 69, 76], 0.6);
}
A.scene(T.secure, T.plus); A.cards(T.secure, T.plus, 3);
A.scene(T.plus, T.outro, { crash: 0.4 }); A.cards(T.plus, T.outro);
A.outro();
require('fs').mkdirSync(__dirname + '/out', { recursive: true });
A.finish(__dirname + '/out/reel.wav');
