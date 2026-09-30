// Soundtrack for the v2.10 release video, keyed to the scene timeline (see ../release-audio.js).
const { DUR, T, CHAT, TOOL_RUN } = require('./scenes');
const A = require('../release-audio').createReleaseAudio({ DUR, T, seed: 3100 });
const { typing, pop, done, fail, stab, bell, blip, impact, PENT } = A;

A.intro(T.mcp);
{ // MCP on Maincloud: chat messages pop, tool calls blip out and back
  const S = T.mcp;
  A.scene(S, T.plus, { crash: 0.6 });
  CHAT.forEach((m, i) => {
    const t = S + m.u;
    if (m.kind === 'user') pop(t);
    else if (m.kind === 'tool') { blip(t, PENT[i + 1], 0.5, -0.2, 35); blip(t + 0.12, PENT[i + 2], 0.35, 0.2, 35); done(t + TOOL_RUN, i); }
    else { typing(t, t + 0.5, 0.3, 20 + i); bell(t, 84, 0.35); }
  });
}
A.scene(T.plus, T.outro); A.cards(T.plus, T.outro, 1);
A.outro();
require('fs').mkdirSync(__dirname + '/out', { recursive: true });
A.finish(__dirname + '/out/reel.wav');
