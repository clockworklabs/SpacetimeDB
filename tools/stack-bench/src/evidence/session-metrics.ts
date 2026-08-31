const USAGE_KEYS = ['input', 'output', 'cacheWrite', 'cacheRead'] as const;

type UsageKey = typeof USAGE_KEYS[number];

export interface SessionMetricsInput {
  costUsd?: number | null;
  tokens?: number | null;
  outputTokens?: number | null;
  turns?: number | null;
  durationMs?: number | null;
  promptBytes?: number | null;
  usage?: Partial<Record<UsageKey, number | null>> | null;
  thinking?: {
    blocks?: number | null;
    signatureBytes?: number | null;
  } | null;
  providerThrottle?: {
    waits?: number | null;
    waitedMs?: number | null;
  } | null;
}

export interface SessionMetricsSummary {
  sessions: number;
  costUsd: number;
  tokens: number;
  outputTokens: number;
  turns: number;
  durationMs: number;
  activeDurationMs: number;
  providerThrottle: { waits: number; waitedMs: number };
  promptBytes: number;
  usage: Record<UsageKey, number>;
  thinking: { blocks: number; signatureBytes: number; sessions: number } | null;
}

const finiteNumber = (value: unknown): number =>
  typeof value === 'number' && Number.isFinite(value) ? value : 0;

export function summarizeSessions(
  sessions: readonly (SessionMetricsInput | null | undefined | false)[],
): SessionMetricsSummary {
  const present = sessions.filter(
    (session): session is SessionMetricsInput => Boolean(session),
  );
  const usage: Record<UsageKey, number> = {
    input: 0,
    output: 0,
    cacheWrite: 0,
    cacheRead: 0,
  };
  const thinking = { blocks: 0, signatureBytes: 0 };
  let thinkingSessions = 0;
  let providerWaits = 0;
  let providerWaitMs = 0;

  for (const session of present) {
    for (const key of USAGE_KEYS) usage[key] += finiteNumber(session.usage?.[key]);
    if (session.thinking) {
      thinking.blocks += finiteNumber(session.thinking.blocks);
      thinking.signatureBytes += finiteNumber(session.thinking.signatureBytes);
      thinkingSessions += 1;
    }
    providerWaits += finiteNumber(session.providerThrottle?.waits);
    providerWaitMs += finiteNumber(session.providerThrottle?.waitedMs);
  }

  const durationMs = present.reduce(
    (total, session) => total + finiteNumber(session.durationMs),
    0,
  );

  return {
    sessions: present.length,
    costUsd: Number(present.reduce(
      (total, session) => total + finiteNumber(session.costUsd),
      0,
    ).toFixed(6)),
    tokens: present.reduce((total, session) => total + finiteNumber(session.tokens), 0),
    outputTokens: present.reduce(
      (total, session) => total + finiteNumber(session.outputTokens),
      0,
    ),
    turns: present.reduce((total, session) => total + finiteNumber(session.turns), 0),
    durationMs,
    activeDurationMs: Math.max(0, durationMs - providerWaitMs),
    providerThrottle: { waits: providerWaits, waitedMs: providerWaitMs },
    promptBytes: present.reduce(
      (total, session) => total + finiteNumber(session.promptBytes),
      0,
    ),
    usage,
    thinking: thinkingSessions ? { ...thinking, sessions: thinkingSessions } : null,
  };
}
