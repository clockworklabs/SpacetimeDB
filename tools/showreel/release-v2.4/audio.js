// Soundtrack for the v2.4 release video, keyed to the scene timeline (see ../release-audio.js).
const { DUR, T, HT, TP } = require('./scenes');
const A = require('../release-audio').createReleaseAudio({ DUR, T, seed: 2400 });
const { typing, pop, done, whoosh, stab, bell, pluck, blip, chordAt, PENT } = A;

A.intro(T.http);
{ // HTTP handlers: code, route appears, curl goes in and "Hello!" comes back
  const S = T.http;
  A.scene(S, T.tmpl);
  typing(S + HT.code[0], S + HT.code[1], 0.35, 71);
  bell(S + HT.code[1], 81, 0.45);
  for (let i = 0; i < 4; i++) blip(S + 1.4 + i * 0.12, PENT[1 + i], 0.4, (i - 1.5) * 0.4, 30);
  typing(S + HT.curl[0], S + HT.curl[1], 0.4, 72);
  whoosh(S + HT.req, HT.res - HT.req, 0.4, 1);
  done(S + HT.res, 3); stab(S + HT.res, [57, 64, 69, 76], 0.6);
}
{ // templates: command, three cards, the chat answers
  const S = T.tmpl;
  A.scene(S, T.plus);
  typing(S + TP.cmd[0], S + TP.cmd[1], 0.4, 73);
  for (let i = 0; i < 3; i++) pluck(S + TP.cards + i * 0.18, chordAt(S + TP.cards).arp[i + 1], 0.6, (i - 1) * 0.5);
  pop(S + TP.cards + 0.6);
  done(S + TP.reply, 4); bell(S + TP.reply, 84, 0.45);
  blip(S + TP.cards + 2.0, 76, 0.5, 0.4, 30);
}
A.scene(T.plus, T.outro); A.cards(T.plus, T.outro, 2);
A.outro();
require('fs').mkdirSync(__dirname + '/out', { recursive: true });
A.finish(__dirname + '/out/reel.wav');
