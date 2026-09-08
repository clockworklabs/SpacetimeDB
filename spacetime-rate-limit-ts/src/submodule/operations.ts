import { Range, SenderError } from 'spacetimedb/server';
import {
  consumeRateLimit,
  DEFAULT_SWEEP_BATCH,
  MAX_SWEEP_BATCH,
  runRateLimitSweep,
  sweepRateLimits,
} from '../index';
import {
  rateLimitBucket,
  rateLimitSweepTick,
  spacetimedb,
  t,
  type ReducerModuleCtx,
  type ViewModuleCtx,
} from './schema';
import { buildRateLimitKey } from '../key';

const MAX_SCOPE_LENGTH = 128;
const MAX_ACTOR_KEY_LENGTH = 256;

export const consumeResult = t.object('RateLimitConsumeResult', {
  allowed: t.bool(),
  scope: t.string(),
  key: t.string(),
  limit: t.u32(),
  used: t.u32(),
  remaining: t.u32(),
  retryAfterSeconds: t.u32(),
  resetAt: t.timestamp(),
});

function isAdmin(ctx: ViewModuleCtx): boolean {
  return ctx.db.rateLimitAdminIdentity.identity.find(ctx.sender) != null;
}

function requireAdmin(ctx: ReducerModuleCtx): void {
  if (ctx.db.rateLimitAdminIdentity.identity.find(ctx.sender) == null) {
    throw new SenderError('rate_limit.not_authorized');
  }
}

function toU32(name: string, value: number, max = 0xffff_ffff): number {
  if (!Number.isInteger(value) || value <= 0 || value > max) {
    throw new SenderError(`rate_limit.invalid_${name}`);
  }
  return value;
}

function sanitizePart(s: string): string {
  return s.trim().replace(/\s+/g, ' ');
}

export const consume = spacetimedb.procedure(
  {
    scope: t.string(),
    actorKey: t.string(),
    limit: t.u32(),
    windowSeconds: t.u32(),
    cost: t.option(t.u32()),
  },
  consumeResult,
  (ctx, args) => {
    const scope = sanitizePart(args.scope);
    const actorKey = sanitizePart(args.actorKey);
    if (scope.length === 0 || scope.length > MAX_SCOPE_LENGTH) {
      throw new SenderError('rate_limit.invalid_scope');
    }
    if (actorKey.length === 0 || actorKey.length > MAX_ACTOR_KEY_LENGTH) {
      throw new SenderError('rate_limit.invalid_actor_key');
    }
    const limit = toU32('limit', Number(args.limit));
    const windowSeconds = toU32('window_seconds', Number(args.windowSeconds));
    const cost = toU32('cost', Number(args.cost ?? 1));
    const key = buildRateLimitKey(scope, actorKey);

    const out = ctx.withTx(tx => {
      requireAdmin(tx);
      const r = consumeRateLimit(tx, {
        key,
        scope,
        limit,
        windowSeconds,
        cost,
      });
      return {
        allowed: r.allowed,
        scope: r.scope,
        key: r.key,
        limit,
        used: r.used,
        remaining: r.remaining,
        retryAfterSeconds: r.retryAfterSeconds,
        resetAt: r.resetAt,
      };
    });
    if (!out) throw new Error('rate_limit.consume_tx_failed');
    return out;
  }
);

export const runSweep = spacetimedb.procedure(
  { maxRows: t.option(t.u32()) },
  t.u32(),
  (ctx, args) => {
    const maxRows =
      args.maxRows === undefined
        ? undefined
        : toU32('sweep_batch', Number(args.maxRows), MAX_SWEEP_BATCH);
    return ctx.withTx(tx => {
      requireAdmin(tx);
      return sweepRateLimits(
        tx,
        tx.db.rateLimitBucket.expiresAt.filter(
          new Range(undefined, { tag: 'included', value: tx.timestamp })
        ),
        maxRows ?? DEFAULT_SWEEP_BATCH
      );
    });
  }
);

export const addRateLimitAdmin = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, args) => {
    requireAdmin(ctx);
    if (ctx.db.rateLimitAdminIdentity.identity.find(args.identity) == null) {
      ctx.db.rateLimitAdminIdentity.insert({
        identity: args.identity,
        addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
      });
    }
  }
);

export const updateConfig = spacetimedb.reducer(
  { sweepBatch: t.u32() },
  (ctx, args) => {
    requireAdmin(ctx);
    const cfg = ctx.db.rateLimitConfig.singleton.find(true);
    if (!cfg) throw new Error('rate_limit.config_missing');
    ctx.db.rateLimitConfig.singleton.update({
      ...cfg,
      sweepBatch: toU32(
        'sweep_batch',
        Number(args.sweepBatch),
        MAX_SWEEP_BATCH
      ),
      updatedAt: ctx.timestamp,
    });
  }
);

export const resetBuckets = spacetimedb.reducer(
  { maxRows: t.option(t.u32()) },
  (ctx, args) => {
    requireAdmin(ctx);
    const maxRows = Math.min(Number(args.maxRows ?? 1000), 10_000);
    let removed = 0;
    for (const row of ctx.db.rateLimitBucket.iter()) {
      if (removed >= maxRows) break;
      ctx.db.rateLimitBucket.delete(row);
      removed++;
    }
  }
);

export const adminRateLimitBuckets = spacetimedb.view(
  { name: 'admin_rate_limit_buckets', public: true },
  t.array(rateLimitBucket.rowType),
  ctx => {
    if (!isAdmin(ctx)) return [];
    const rows = [];
    for (const row of ctx.db.rateLimitBucket.iter()) {
      if (rows.length >= 1000) break;
      rows.push(row);
    }
    return rows;
  }
);

export const rate_limit_sweep = spacetimedb.reducer(
  { onSchedule: rateLimitSweepTick },
  { arg: rateLimitSweepTick.rowType },
  (ctx, _args) => {
    runRateLimitSweep(
      ctx,
      ctx.db.rateLimitBucket.expiresAt.filter(
        new Range(undefined, { tag: 'included', value: ctx.timestamp })
      )
    );
  }
);
