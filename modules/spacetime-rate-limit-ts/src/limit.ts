import { Timestamp } from 'spacetimedb';
import { SenderError } from 'spacetimedb/server';

const ONE_SECOND_MICROS = 1_000_000n;
const U32_MAX = 0xffff_ffff;

export const DEFAULT_SWEEP_BATCH = 500;
export const MAX_SWEEP_BATCH = 10_000;
const MAX_SCOPE_LENGTH = 128;
const MAX_ACTOR_KEY_LENGTH = 256;

/** Codes a reducer, procedure, or view caller can receive as a `SenderError`. */
export const errors = {
  notAuthorized: 'rate_limit.not_authorized',
  cannotRemoveLastAdmin: 'rate_limit.cannot_remove_last_admin',
  invalidActorKey: 'rate_limit.invalid_actor_key',
  invalidCost: 'rate_limit.invalid_cost',
  invalidSweepBatch: 'rate_limit.invalid_sweep_batch',
  invalidMaxRows: 'rate_limit.invalid_max_rows',
} as const;

// Policy errors fail the host module while it loads, before any caller.
const policyErrors = {
  invalidScope: 'rate_limit.invalid_scope',
  duplicateScope: 'rate_limit.duplicate_scope',
  invalidLimit: 'rate_limit.invalid_limit',
  invalidWindow: 'rate_limit.invalid_window',
} as const;

export interface RateLimitPolicy {
  scope: string;
  limit: number;
  windowSeconds: number;
}

export type RateLimitResult =
  | {
      allowed: true;
      key: string;
      scope: string;
      limit: number;
      used: number;
      remaining: number;
      resetAt: Timestamp;
      retryAfterSeconds: 0;
    }
  | {
      allowed: false;
      key: string;
      scope: string;
      limit: number;
      used: number;
      remaining: 0;
      resetAt: Timestamp;
      retryAfterSeconds: number;
    };

/** Current usage of one actor's bucket. `resetAt` is unset when no window is open. */
export interface RateLimitStatus {
  limit: number;
  windowSeconds: number;
  used: number;
  remaining: number;
  resetAt: Timestamp | undefined;
}

export interface RateLimitBucketRow {
  key: string;
  scope: string;
  windowStart: Timestamp;
  expiresAt: Timestamp;
  count: number;
  updatedAt: Timestamp;
}

export interface RateLimitReadDb {
  rateLimitBucket: {
    key: {
      find(key: string): RateLimitBucketRow | null | undefined;
    };
  };
}

export interface RateLimitTxLike {
  timestamp: Timestamp;
  db: {
    rateLimitBucket: {
      key: {
        find(key: string): RateLimitBucketRow | null | undefined;
        update(row: RateLimitBucketRow): void;
      };
      insert(row: RateLimitBucketRow): void;
      delete(row: RateLimitBucketRow): void;
    };
  };
}

function isPositiveInt(value: number, max = U32_MAX): boolean {
  return Number.isInteger(value) && value > 0 && value <= max;
}

function secondsUntil(now: Timestamp, future: Timestamp): number {
  const delta = future.microsSinceUnixEpoch - now.microsSinceUnixEpoch;
  if (delta <= 0n) return 0;
  return Number((delta + ONE_SECOND_MICROS - 1n) / ONE_SECOND_MICROS);
}

// Module state lasts for one evaluation of the host module. SpacetimeDB
// evaluates it once in each fresh worker isolate.
const registeredScopes = new Set<string>();

/** Forgets every configured scope so `client` can configure them again. */
export function resetRegisteredScopes(): void {
  registeredScopes.clear();
}

/**
 * Configure the policy for one scope. Each scope may be configured once per
 * module so every caller enforces the same limit and window.
 */
export function client({ scope, limit, windowSeconds }: RateLimitPolicy) {
  if (scope.length === 0 || scope.length > MAX_SCOPE_LENGTH)
    throw new Error(policyErrors.invalidScope);
  if (!isPositiveInt(limit)) throw new Error(policyErrors.invalidLimit);
  if (!isPositiveInt(windowSeconds))
    throw new Error(policyErrors.invalidWindow);
  if (registeredScopes.has(scope)) throw new Error(policyErrors.duplicateScope);
  registeredScopes.add(scope);

  function bucketKey(actorKey: string): string {
    if (actorKey.length === 0 || actorKey.length > MAX_ACTOR_KEY_LENGTH)
      throw new SenderError(errors.invalidActorKey);
    // Length-prefix both parts so delimiters inside them cannot collide.
    return `${scope.length}:${scope}${actorKey.length}:${actorKey}`;
  }

  return {
    scope,
    limit,
    windowSeconds,

    /**
     * Spend `cost` (default 1) from the actor's bucket in the caller's
     * transaction. A cost above `limit` can never be allowed, so it throws
     * `errors.invalidCost`.
     */
    consume(
      tx: RateLimitTxLike,
      opts: { key: string; cost?: number }
    ): RateLimitResult {
      const key = bucketKey(opts.key);
      const cost = opts.cost ?? 1;
      if (!isPositiveInt(cost, limit)) {
        throw new SenderError(errors.invalidCost);
      }
      const now = tx.timestamp;
      const existing = tx.db.rateLimitBucket.key.find(key);
      const open =
        existing != null &&
        existing.expiresAt.microsSinceUnixEpoch > now.microsSinceUnixEpoch;
      const used = open ? existing.count : 0;
      const resetAt = open
        ? existing.expiresAt
        : new Timestamp(
            now.microsSinceUnixEpoch + BigInt(windowSeconds) * ONE_SECOND_MICROS
          );

      if (used + cost > limit) {
        return {
          allowed: false,
          key,
          scope,
          limit,
          used,
          remaining: 0,
          resetAt,
          retryAfterSeconds: secondsUntil(now, resetAt),
        };
      }

      const row = {
        key,
        scope,
        windowStart: open ? existing.windowStart : now,
        expiresAt: resetAt,
        count: used + cost,
        updatedAt: now,
      };
      // Update any existing row, including an expired one not yet swept.
      if (existing) tx.db.rateLimitBucket.key.update(row);
      else tx.db.rateLimitBucket.insert(row);
      return {
        allowed: true,
        key,
        scope,
        limit,
        used: row.count,
        remaining: limit - row.count,
        resetAt,
        retryAfterSeconds: 0,
      };
    },

    /**
     * Read the actor's bucket without spending from it. Views have no clock, so
     * pass `now` from a reducer or procedure to report an expired window as
     * fresh; without it, compare `resetAt` against the caller's clock.
     */
    peek(db: RateLimitReadDb, key: string, now?: Timestamp): RateLimitStatus {
      const row = db.rateLimitBucket.key.find(bucketKey(key));
      if (
        !row ||
        (now && row.expiresAt.microsSinceUnixEpoch <= now.microsSinceUnixEpoch)
      ) {
        return {
          limit,
          windowSeconds,
          used: 0,
          remaining: limit,
          resetAt: undefined,
        };
      }
      return {
        limit,
        windowSeconds,
        used: row.count,
        remaining: Math.max(0, limit - row.count),
        resetAt: row.expiresAt,
      };
    },
  };
}

export type RateLimitClient = ReturnType<typeof client>;

export function sweepRateLimits(
  tx: RateLimitTxLike,
  expiredRows: Iterable<RateLimitBucketRow>,
  maxRows: number
): number {
  if (!isPositiveInt(maxRows, MAX_SWEEP_BATCH)) {
    throw new Error(errors.invalidSweepBatch);
  }
  const now = tx.timestamp.microsSinceUnixEpoch;
  let deleted = 0;
  for (const row of expiredRows) {
    if (deleted >= maxRows) break;
    if (row.expiresAt.microsSinceUnixEpoch > now) continue;
    tx.db.rateLimitBucket.delete(row);
    deleted++;
  }
  return deleted;
}
