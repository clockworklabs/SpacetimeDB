import { Timestamp } from 'spacetimedb';
import { truncateForLog } from './http.js';

export const RETRY_WINDOW_MICROS = 24n * 60n * 60n * 1_000_000n;
const MAX_RETRY_DELAY_MICROS = 5n * 60n * 1_000_000n;

type OutboxRow = {
  status: { tag: string };
  attempts: number;
  claimId?: string | undefined;
  claimExpiresAt: Timestamp;
  nextAttemptAt: Timestamp;
  createdAt: Timestamp;
  retryExpiresAt: Timestamp;
  lastStatusCode?: number | undefined;
  lastError?: string | undefined;
  updatedAt: Timestamp;
  deliveredAt?: Timestamp | undefined;
};

export function claimHasExpired(
  row: Pick<OutboxRow, 'claimExpiresAt'>,
  now: Timestamp
): boolean {
  return row.claimExpiresAt.microsSinceUnixEpoch <= now.microsSinceUnixEpoch;
}

export function retryDelayMicros(attempt: number): bigint {
  return attempt <= 5
    ? 5_000_000n * (1n << BigInt(attempt - 1))
    : MAX_RETRY_DELAY_MICROS;
}

export function expireOutboxRow<T extends OutboxRow>(
  row: T,
  timestamp: Timestamp
): T {
  return {
    ...row,
    status: { tag: 'Failed' },
    claimId: undefined,
    claimExpiresAt: Timestamp.UNIX_EPOCH,
    nextAttemptAt: timestamp,
    lastError: 'posthog.retry_expired',
    updatedAt: timestamp,
  };
}

export function releaseExpiredClaim<T extends OutboxRow>(
  row: T,
  timestamp: Timestamp
): T {
  return {
    ...row,
    status: { tag: 'Queued' },
    claimId: undefined,
    claimExpiresAt: Timestamp.UNIX_EPOCH,
    nextAttemptAt: timestamp,
    updatedAt: timestamp,
  };
}

/** 4xx responses other than 408 and 429 will not succeed on retry. */
export function isPermanentFailure(statusCode: number): boolean {
  return (
    statusCode >= 400 &&
    statusCode < 500 &&
    statusCode !== 408 &&
    statusCode !== 429
  );
}

export function requeueFailedRow<T extends OutboxRow>(
  row: T,
  timestamp: Timestamp
): T {
  return {
    ...row,
    status: { tag: 'Queued' },
    attempts: 0,
    claimId: undefined,
    claimExpiresAt: Timestamp.UNIX_EPOCH,
    nextAttemptAt: timestamp,
    retryExpiresAt: new Timestamp(
      timestamp.microsSinceUnixEpoch + RETRY_WINDOW_MICROS
    ),
    updatedAt: timestamp,
  };
}

export function claimOutboxRow<T extends OutboxRow>(
  row: T,
  claimId: string,
  expiresAt: Timestamp,
  timestamp: Timestamp
): T {
  return {
    ...row,
    status: { tag: 'Processing' },
    claimId,
    claimExpiresAt: expiresAt,
    updatedAt: timestamp,
  };
}

export function settleOutboxClaim<T extends OutboxRow>(
  row: T,
  result: { ok: boolean; statusCode: number; responseBody: string },
  timestamp: Timestamp
): T {
  const attempts = row.attempts + 1;
  const deadline = row.retryExpiresAt;
  const terminal =
    result.ok ||
    isPermanentFailure(result.statusCode) ||
    timestamp.microsSinceUnixEpoch >= deadline.microsSinceUnixEpoch;
  const retryAt = new Timestamp(
    timestamp.microsSinceUnixEpoch + retryDelayMicros(attempts)
  );
  return {
    ...row,
    status: result.ok
      ? { tag: 'Delivered' }
      : terminal
        ? { tag: 'Failed' }
        : { tag: 'Queued' },
    attempts,
    claimId: undefined,
    claimExpiresAt: Timestamp.UNIX_EPOCH,
    nextAttemptAt: terminal
      ? timestamp
      : retryAt.microsSinceUnixEpoch < deadline.microsSinceUnixEpoch
        ? retryAt
        : deadline,
    lastStatusCode: result.statusCode,
    lastError: result.ok ? undefined : truncateForLog(result.responseBody),
    updatedAt: timestamp,
    deliveredAt: result.ok ? timestamp : undefined,
  };
}
