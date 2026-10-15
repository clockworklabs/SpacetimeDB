import { Timestamp } from 'spacetimedb';
import { truncateForLog } from './http.js';

export const MAX_DELIVERY_ATTEMPTS = 5;
const INITIAL_RETRY_DELAY_MICROS = 1_000_000n;
const MAX_RETRY_DELAY_MICROS = 5n * 60n * 1_000_000n;

type OutboxRow = {
  status: { tag: string };
  attempts: number;
  claimId?: string | undefined;
  claimExpiresAt: Timestamp;
  nextAttemptAt: unknown;
  lastStatusCode?: number | undefined;
  lastError?: string | undefined;
  updatedAt: unknown;
  deliveredAt?: unknown;
};

export function claimHasExpired(
  row: Pick<OutboxRow, 'claimExpiresAt'>,
  now: Timestamp
): boolean {
  return row.claimExpiresAt.microsSinceUnixEpoch <= now.microsSinceUnixEpoch;
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
  timestamp: T['updatedAt']
): T {
  return {
    ...row,
    status: { tag: 'Queued' },
    attempts: 0,
    claimId: undefined,
    claimExpiresAt: Timestamp.UNIX_EPOCH,
    nextAttemptAt: timestamp,
    updatedAt: timestamp,
  };
}

export function claimOutboxRow<T extends OutboxRow>(
  row: T,
  claimId: string,
  expiresAt: Timestamp,
  timestamp: T['updatedAt']
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
  timestamp: T['updatedAt'],
  retryAt: T['nextAttemptAt']
): T {
  const attempts = row.attempts + 1;
  const terminal =
    result.ok ||
    isPermanentFailure(result.statusCode) ||
    attempts >= MAX_DELIVERY_ATTEMPTS;
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
    nextAttemptAt: terminal ? timestamp : retryAt,
    lastStatusCode: result.statusCode,
    lastError: result.ok ? undefined : truncateForLog(result.responseBody),
    updatedAt: timestamp,
    deliveredAt: result.ok ? timestamp : undefined,
  };
}
