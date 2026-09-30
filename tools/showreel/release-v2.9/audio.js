// Soundtrack for the v2.9 release video, keyed to the scene timeline (see ../release-audio.js).
const { DUR, T, CHAT, TOOL_RUN, TYPE, DONE, SKILLS, GET, TOGGLE, LANES, LANE_SIM, LANE_PLAY } = require('./scenes');
const A = require('../release-audio').createReleaseAudio({ DUR, T, seed: 2900 });
const { typing, pop, done, fail, tick, whoosh, stab, bell, pluck, blip, impact, pad, chordAt, PENT } = A;

A.intro(T.mcp);
{ // MCP: chat messages pop, tool calls blip out and back
  const S = T.mcp;
  A.scene(S, T.plugins, { crash: 0.6 });
  A.chatCues(S, CHAT, TOOL_RUN);
}
{ // plugins: install commands typed, skills pop in
  const S = T.plugins;
  A.scene(S, T.unity, { bassG: 0.85 });
  TYPE.forEach(([a, b], i) => { typing(S + a, S + b, 0.4, 30 + i); if (i % 2) done(S + b + DONE, i); });
  SKILLS.forEach((_, i) => { if (i % 2 === 0) pluck(S + GET + 0.1 + i * 0.05, chordAt(S + GET).arp[(i / 2) % 5], 0.45, ((i % 5) - 2) * 0.3); });
  stab(S + GET + 0.9, [57, 64, 69, 76], 0.7); bell(S + GET + 1.0, 81, 0.5);
}
{ // Unity: the toggle clicks off, play sessions start on the lanes
  const S = T.unity;
  A.scene(S, T.mods, { bassG: 0.8 });
  tick(S + TOGGLE, 1.2, 0.3); blip(S + TOGGLE + 0.3, 76, 0.6, 0.3, 25);
  const at = s => S + LANE_PLAY[0] + (LANE_PLAY[1] - LANE_PLAY[0]) * (s - LANE_SIM[0]) / (LANE_SIM[1] - LANE_SIM[0]);
  LANES[1].segs.forEach(([, s0], i) => pluck(at(s0), chordAt(at(s0)).arp[i + 1], 0.7, 0.3));
  LANES[0].segs.forEach(([kind, s0]) => { if (kind === 'play') blip(at(s0), 64, 0.45, -0.4, 40); });
}
A.scene(T.mods, T.outro); A.cards(T.mods, T.outro, 2);
A.outro();
require('fs').mkdirSync(__dirname + '/out', { recursive: true });
A.finish(__dirname + '/out/reel.wav');
