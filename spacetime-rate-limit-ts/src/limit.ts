import { ScheduleAt, Timestamp } from 'spacetimedb';

const ONE_SECOND_MICROS = 1_000_000n;
const U32_MAX = 0xffff_ffff;

export const DEFAULT_SWEEP_BATCH = 500;
export const MAX_SWEEP_BATCH = 10_000;
export const DEFAULT_SWEEP_INTERVAL_SECONDS = 30n;

export interface ConsumeRateLimitOpts {
  key: string;
  scope: string;
  limit: number;
  windowSeconds: number;
  cost?: number;
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

function assertPositiveInt(name: string, value: number): void {
  if (!Number.isInteger(value) || value <= 0 || value > U32_MAX) {
    throw new Error(`rate_limit.invalid_${name}`);
  }
}

export function assertRateLimitSweepBatch(value: number): void {
  assertPositiveInt('sweep_batch', value);
  if (value > MAX_SWEEP_BATCH) {
    throw new Error('rate_limit.invalid_sweep_batch');
  }
}

function plusSeconds(timestamp: Timestamp, seconds: number): Timestamp {
  return new Timestamp(
    (timestamp.microsSinceUnixEpoch as bigint) +
      BigInt(seconds) * ONE_SECOND_MICROS
  );
}

function secondsUntil(now: Timestamp, future: Timestamp): number {
  const delta =
    (future.microsSinceUnixEpoch as bigint) -
    (now.microsSinceUnixEpoch as bigint);
  if (delta <= 0n) return 0;
  return Number((delta + ONE_SECOND_MICROS - 1n) / ONE_SECOND_MICROS);
}

export interface RateLimitBucketRow {
  key: string;
  scope: string;
  windowStart: Timestamp;
  expiresAt: Timestamp;
  count: number;
  updatedAt: Timestamp;
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

export interface RateLimitInitCtxLike {
  timestamp: Timestamp;
  db: {
    rateLimitConfig: {
      singleton: {
        find(
          key: boolean
        ):
          | { singleton: boolean; sweepBatch: number; updatedAt: Timestamp }
          | null
          | undefined;
        update(row: {
          singleton: boolean;
          sweepBatch: number;
          updatedAt: Timestamp;
        }): void;
      };
      insert(row: {
        singleton: boolean;
        sweepBatch: number;
        updatedAt: Timestamp;
      }): void;
    };
    rateLimitSweepTick: {
      insert(row: { scheduledId: bigint; scheduledAt: ScheduleAt }): void;
    };
  };
}

export interface RateLimitInstallOpts {
  sweepBatch?: number;
  sweepIntervalSeconds?: bigint;
}

export function installRateLimitState(
  ctx: RateLimitInitCtxLike,
  opts?: RateLimitInstallOpts
): void {
  const sweepBatch = opts?.sweepBatch ?? DEFAULT_SWEEP_BATCH;
  assertRateLimitSweepBatch(sweepBatch);
  const sweepIntervalSeconds =
    opts?.sweepIntervalSeconds ?? DEFAULT_SWEEP_INTERVAL_SECONDS;
  if (sweepIntervalSeconds <= 0n)
    throw new Error('rate_limit.invalid_sweep_interval_seconds');

  const existing = ctx.db.rateLimitConfig.singleton.find(true);
  if (!existing) {
    ctx.db.rateLimitConfig.insert({
      singleton: true,
      sweepBatch,
      updatedAt: ctx.timestamp,
    });
  }

  ctx.db.rateLimitSweepTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(sweepIntervalSeconds * ONE_SECOND_MICROS),
  });
}

export function resolveRateLimitSweepBatch(
  ctx: {
    db: {
      rateLimitConfig: {
        singleton: {
          find(key: boolean): { sweepBatch: number } | null | undefined;
        };
      };
    };
  },
  fallback = DEFAULT_SWEEP_BATCH
): number {
  const cfg = ctx.db.rateLimitConfig.singleton.find(true);
  if (!cfg) return fallback;
  const batch = Number(cfg.sweepBatch);
  const safeFallback =
    Number.isInteger(fallback) && fallback > 0 && fallback <= MAX_SWEEP_BATCH
      ? fallback
      : DEFAULT_SWEEP_BATCH;
  return Number.isInteger(batch) && batch > 0 && batch <= MAX_SWEEP_BATCH
    ? batch
    : safeFallback;
}

export interface RateLimitSweepCtxLike extends RateLimitTxLike {
  db: RateLimitTxLike['db'] & {
    rateLimitConfig: {
      singleton: {
        find(key: boolean): { sweepBatch: number } | null | undefined;
      };
    };
  };
}

export function runRateLimitSweep(
  ctx: RateLimitSweepCtxLike,
  expiredRows: Iterable<RateLimitBucketRow>
): number {
  const batch = resolveRateLimitSweepBatch(ctx);
  return sweepRateLimits(ctx, expiredRows, batch);
}

export function consumeRateLimit(
  tx: RateLimitTxLike,
  opts: ConsumeRateLimitOpts
): RateLimitResult {
  assertPositiveInt('limit', opts.limit);
  assertPositiveInt('window', opts.windowSeconds);
  const cost = opts.cost ?? 1;
  assertPositiveInt('cost', cost);

  const now = tx.timestamp as Timestamp;
  const resetAt = plusSeconds(now, opts.windowSeconds);
  const existing = tx.db.rateLimitBucket.key.find(opts.key);

  if (
    !existing ||
    (existing.expiresAt.microsSinceUnixEpoch as bigint) <=
      (now.microsSinceUnixEpoch as bigint)
  ) {
    if (cost > opts.limit) {
      return {
        allowed: false,
        key: opts.key,
        scope: opts.scope,
        limit: opts.limit,
        used: 0,
        remaining: 0,
        resetAt,
        retryAfterSeconds: opts.windowSeconds,
      };
    }
    // Reuse an expired row until the sweeper deletes it. Update in place to
    // avoid a key collision.
    if (existing) {
      tx.db.rateLimitBucket.key.update({
        ...existing,
        scope: opts.scope,
        windowStart: now,
        expiresAt: resetAt,
        count: cost,
        updatedAt: now,
      });
    } else {
      tx.db.rateLimitBucket.insert({
        key: opts.key,
        scope: opts.scope,
        windowStart: now,
        expiresAt: resetAt,
        count: cost,
        updatedAt: now,
      });
    }
    return {
      allowed: true,
      key: opts.key,
      scope: opts.scope,
      limit: opts.limit,
      used: cost,
      remaining: opts.limit - cost,
      resetAt,
      retryAfterSeconds: 0,
    };
  }

  const used = existing.count as number;
  if (used + cost > opts.limit) {
    return {
      allowed: false,
      key: opts.key,
      scope: opts.scope,
      limit: opts.limit,
      used,
      remaining: 0,
      resetAt: existing.expiresAt,
      retryAfterSeconds: secondsUntil(now, existing.expiresAt),
    };
  }

  const nextUsed = used + cost;
  tx.db.rateLimitBucket.key.update({
    ...existing,
    scope: opts.scope,
    count: nextUsed,
    updatedAt: now,
  });
  return {
    allowed: true,
    key: opts.key,
    scope: opts.scope,
    limit: opts.limit,
    used: nextUsed,
    remaining: opts.limit - nextUsed,
    resetAt: existing.expiresAt,
    retryAfterSeconds: 0,
  };
}

export function sweepRateLimits(
  tx: RateLimitTxLike,
  expiredRows: Iterable<RateLimitBucketRow>,
  maxRows = DEFAULT_SWEEP_BATCH
): number {
  assertRateLimitSweepBatch(maxRows);
  const nowMicros = tx.timestamp.microsSinceUnixEpoch as bigint;
  let deleted = 0;
  for (const row of expiredRows) {
    if (deleted >= maxRows) break;
    if ((row.expiresAt.microsSinceUnixEpoch as bigint) > nowMicros) break;
    tx.db.rateLimitBucket.delete(row);
    deleted++;
  }
  return deleted;
}
