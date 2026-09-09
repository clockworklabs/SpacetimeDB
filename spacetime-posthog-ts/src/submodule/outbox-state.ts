export const MAX_DELIVERY_ATTEMPTS = 5;
const INITIAL_RETRY_DELAY_MICROS = 1_000_000n;
const MAX_RETRY_DELAY_MICROS = 5n * 60n * 1_000_000n;

type OutboxRow = {
  status: { tag: string };
  attempts: number;
  claimId?: string | undefined;
  claimExpiresAtMicros: bigint;
  nextAttemptAt: unknown;
  lastStatusCode?: number | undefined;
  lastError?: string | undefined;
  updatedAt: unknown;
  deliveredAt?: unknown;
};

export function claimHasExpired(
  row: Pick<OutboxRow, 'claimExpiresAtMicros'>,
  nowMicros: bigint
): boolean {
  return row.claimExpiresAtMicros <= nowMicros;
}

export function retryDelayMicros(attempt: number): bigint {
  const exponent = Math.max(0, Math.min(30, Math.trunc(attempt) - 1));
  const delay = INITIAL_RETRY_DELAY_MICROS * (1n << BigInt(exponent));
  return delay > MAX_RETRY_DELAY_MICROS ? MAX_RETRY_DELAY_MICROS : delay;
}

export function releaseExpiredClaim<T extends OutboxRow>(
  row: T,
  timestamp: T['updatedAt']
): T {
  return {
    ...row,
    status: { tag: 'Queued' },
    claimId: undefined,
    claimExpiresAtMicros: 0n,
    nextAttemptAt: timestamp,
    updatedAt: timestamp,
  };
}

export function claimOutboxRow<T extends OutboxRow>(
  row: T,
  claimId: string,
  expiresAtMicros: bigint,
  timestamp: T['updatedAt']
): T {
  return {
    ...row,
    status: { tag: 'Processing' },
    claimId,
    claimExpiresAtMicros: expiresAtMicros,
    updatedAt: timestamp,
  };
}

export function settleOutboxClaim<T extends OutboxRow>(
  row: T,
  result: { ok: boolean; statusCode: number; responseBody: string },
  timestamp: T['updatedAt'],
  retryAt: T['nextAttemptAt']
): { row: T; terminal: boolean } {
  const attempts = row.attempts + 1;
  const terminal = result.ok || attempts >= MAX_DELIVERY_ATTEMPTS;
  return {
    terminal,
    row: {
      ...row,
      status: result.ok
        ? { tag: 'Delivered' }
        : terminal
          ? { tag: 'Failed' }
          : { tag: 'Queued' },
      attempts,
      claimId: undefined,
      claimExpiresAtMicros: 0n,
      nextAttemptAt: terminal ? timestamp : retryAt,
      lastStatusCode: result.statusCode,
      lastError: result.ok ? undefined : result.responseBody,
      updatedAt: timestamp,
      deliveredAt: result.ok ? timestamp : undefined,
    },
  };
}
