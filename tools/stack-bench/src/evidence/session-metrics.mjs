const number = value => Number.isFinite(value) ? value : 0;

export function summarizeSessions(sessions) {
  const present = sessions.filter(Boolean);
  const usage = { input: 0, output: 0, cacheWrite: 0, cacheRead: 0 };
  const thinking = { blocks: 0, signatureBytes: 0 };
  let thinkingSessions = 0;

  for (const session of present) {
    for (const key of Object.keys(usage)) usage[key] += number(session.usage?.[key]);
    if (session.thinking) {
      thinking.blocks += number(session.thinking.blocks);
      thinking.signatureBytes += number(session.thinking.signatureBytes);
      thinkingSessions += 1;
    }
  }

  return {
    sessions: present.length,
    costUsd: Number(present.reduce((total, session) => total + number(session.costUsd), 0).toFixed(4)),
    tokens: present.reduce((total, session) => total + number(session.tokens), 0),
    outputTokens: present.reduce((total, session) => total + number(session.outputTokens), 0),
    turns: present.reduce((total, session) => total + number(session.turns), 0),
    durationMs: present.reduce((total, session) => total + number(session.durationMs), 0),
    promptBytes: present.reduce((total, session) => total + number(session.promptBytes), 0),
    usage,
    thinking: thinkingSessions ? { ...thinking, sessions: thinkingSessions } : null,
  };
}
