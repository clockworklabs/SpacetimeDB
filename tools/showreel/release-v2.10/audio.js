// Soundtrack for the v2.10 release video, keyed to the scene timeline (see ../release-audio.js).
const { DUR, T, CHAT, TOOL_RUN } = require('./scenes');
const A = require('../release-audio').createReleaseAudio({ DUR, T, seed: 3100 });
const { typing, pop, done, fail, stab, bell, blip, impact, PENT } = A;

A.intro(T.mcp);
{ // MCP on Maincloud: chat messages pop, tool calls blip out and back
  const S = T.mcp;
  A.scene(S, T.plus, { crash: 0.6 });
  A.chatCues(S, CHAT, TOOL_RUN);
}
A.scene(T.plus, T.outro); A.cards(T.plus, T.outro, 1);
A.outro();
require('fs').mkdirSync(__dirname + '/out', { recursive: true });
A.finish(__dirname + '/out/reel.wav');
