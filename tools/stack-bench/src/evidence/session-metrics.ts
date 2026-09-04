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

function nonNegative(value: unknown, at: string, { integer = false } = {}): number {
  if (value === null || value === undefined) return 0;
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0
      || (integer && !Number.isSafeInteger(value))) {
    throw new Error(`${at} must be a non-negative ${integer ? 'integer' : 'number'}`);
  }
  return value;
}

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

  for (const [index, session] of present.entries()) {
    const at = `sessions[${index}]`;
    for (const key of USAGE_KEYS) {
      usage[key] += nonNegative(session.usage?.[key], `${at}.usage.${key}`, { integer: true });
    }
    if (session.thinking) {
      thinking.blocks += nonNegative(session.thinking.blocks, `${at}.thinking.blocks`, { integer: true });
      thinking.signatureBytes += nonNegative(session.thinking.signatureBytes,
        `${at}.thinking.signatureBytes`, { integer: true });
      thinkingSessions += 1;
    }
    providerWaits += nonNegative(session.providerThrottle?.waits,
      `${at}.providerThrottle.waits`, { integer: true });
    providerWaitMs += nonNegative(session.providerThrottle?.waitedMs,
      `${at}.providerThrottle.waitedMs`, { integer: true });
  }

  const durationMs = present.reduce(
    (total, session, index) => total + nonNegative(session.durationMs,
      `sessions[${index}].durationMs`, { integer: true }),
    0,
  );
  if (providerWaitMs > durationMs) {
    throw new Error('provider throttle wait time exceeds total session duration');
  }

  return {
    sessions: present.length,
    costUsd: Number(present.reduce(
      (total, session, index) => total + nonNegative(session.costUsd,
        `sessions[${index}].costUsd`),
      0,
    ).toFixed(6)),
    tokens: present.reduce((total, session, index) => total + nonNegative(session.tokens,
      `sessions[${index}].tokens`, { integer: true }), 0),
    outputTokens: present.reduce(
      (total, session, index) => total + nonNegative(session.outputTokens,
        `sessions[${index}].outputTokens`, { integer: true }),
      0,
    ),
    turns: present.reduce((total, session, index) => total + nonNegative(session.turns,
      `sessions[${index}].turns`, { integer: true }), 0),
    durationMs,
    activeDurationMs: Math.max(0, durationMs - providerWaitMs),
    providerThrottle: { waits: providerWaits, waitedMs: providerWaitMs },
    promptBytes: present.reduce(
      (total, session, index) => total + nonNegative(session.promptBytes,
        `sessions[${index}].promptBytes`, { integer: true }),
      0,
    ),
    usage,
    thinking: thinkingSessions ? { ...thinking, sessions: thinkingSessions } : null,
  };
}
