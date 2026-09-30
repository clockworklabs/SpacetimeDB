// Soundtrack for the v2.7 release video, keyed to the scene timeline (see ../release-audio.js).
const { DUR, T, CHAT, TOOL_RUN, LK, UQ } = require('./scenes');
const A = require('../release-audio').createReleaseAudio({ DUR, T, seed: 2700 });
const { typing, pop, done, fail, stab, bell, blip, impact, tick, PENT } = A;

A.intro(T.mcp);
{ // MCP: chat messages pop, tool calls blip out and back
  const S = T.mcp;
  A.scene(S, T.lock, { crash: 0.6 });
  A.chatCues(S, CHAT, TOOL_RUN);
}
{ // lock: commands typed; the lock clicks shut, deletes bounce, then it opens
  const S = T.lock;
  A.scene(S, T.uniq);
  LK.rows.forEach((r, i) => {
    typing(S + r.c0, S + r.c1, 0.4, 50 + i);
    if (r.ok) { done(S + r.out, i); if (i === 0) { impact(S + r.out, 0.35); tick(S + r.out, 1.2, 0); } }
    else { fail(S + r.out); impact(S + r.out, 0.3); }
  });
  stab(S + LK.rows[3].out, [57, 64, 69, 76], 0.6);
}
{ // unique: publish on both sides; the old one fails, the new one migrates in place
  const S = T.uniq;
  A.scene(S, T.dx);
  pop(S + 0.8);
  typing(S + UQ.pub, S + UQ.pub + 0.3, 0.3, 61);
  fail(S + UQ.out); done(S + UQ.out + 0.1, 3);
  bell(S + UQ.badge, 81, 0.5);
}
A.scene(T.dx, T.outro); A.cards(T.dx, T.outro, 5);
A.outro();
require('fs').mkdirSync(__dirname + '/out', { recursive: true });
A.finish(__dirname + '/out/reel.wav');
