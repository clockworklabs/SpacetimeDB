import { Range, SenderError } from 'spacetimedb/server';
import type { Identity } from 'spacetimedb';
import {
  DEFAULT_SWEEP_BATCH,
  MAX_SWEEP_BATCH,
  sweepRateLimits,
  errors,
} from '../limit';
import {
  rateLimitBucket,
  rateLimitSweepTick,
  spacetimedb,
  t,
  type ViewModuleCtx,
} from './schema';

/** Whether `identity` is a rate-limit administrator. */
export function isAdmin(db: ViewModuleCtx['db'], identity: Identity): boolean {
  return db.rateLimitAdminIdentity.identity.find(identity) != null;
}

/** Throw `errors.notAuthorized` unless the caller is a rate-limit administrator. */
export function requireAdmin(ctx: Pick<ViewModuleCtx, 'db' | 'sender'>): void {
  if (!isAdmin(ctx.db, ctx.sender)) {
    throw new SenderError(errors.notAuthorized);
  }
}

function positiveU32(code: string, value: number, max: number): number {
  if (!Number.isInteger(value) || value <= 0 || value > max) {
    throw new SenderError(code);
  }
  return value;
}

function maxRowsArg(value: number | undefined): number {
  return value === undefined
    ? DEFAULT_SWEEP_BATCH
    : positiveU32(errors.invalidMaxRows, value, MAX_SWEEP_BATCH);
}

export const runSweep = spacetimedb.procedure(
  { maxRows: t.option(t.u32()) },
  t.u32(),
  (ctx, args) => {
    const maxRows = maxRowsArg(args.maxRows);
    return ctx.withTx(tx => {
      requireAdmin(tx);
      return sweepRateLimits(
        tx,
        tx.db.rateLimitBucket.expiresAt.filter(
          new Range(undefined, { tag: 'included', value: tx.timestamp })
        ),
        maxRows
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
        addedAt: ctx.timestamp,
      });
    }
  }
);

export const removeRateLimitAdmin = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, { identity }) => {
    requireAdmin(ctx);
    const existing = ctx.db.rateLimitAdminIdentity.identity.find(identity);
    if (!existing) return;
    if (ctx.db.rateLimitAdminIdentity.count() <= 1n) {
      throw new SenderError(errors.cannotRemoveLastAdmin);
    }
    ctx.db.rateLimitAdminIdentity.delete(existing);
  }
);

export const updateConfig = spacetimedb.reducer(
  { sweepBatch: t.u32() },
  (ctx, args) => {
    requireAdmin(ctx);
    const cfg = ctx.db.rateLimitConfig.singleton.find(true);
    if (!cfg) throw new Error(errors.configMissing);
    ctx.db.rateLimitConfig.singleton.update({
      ...cfg,
      sweepBatch: positiveU32(
        errors.invalidSweepBatch,
        args.sweepBatch,
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
    const maxRows = maxRowsArg(args.maxRows);
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
    if (!isAdmin(ctx.db, ctx.sender)) return [];
    const rows = [];
    for (const row of ctx.db.rateLimitBucket.iter()) {
      if (rows.length >= 1000) break;
      rows.push(row);
    }
    return rows;
  }
);

export const rateLimitSweep = spacetimedb.reducer(
  { onSchedule: rateLimitSweepTick },
  { arg: rateLimitSweepTick.rowType },
  (ctx, _args) => {
    sweepRateLimits(
      ctx,
      ctx.db.rateLimitBucket.expiresAt.filter(
        new Range(undefined, { tag: 'included', value: ctx.timestamp })
      ),
      ctx.db.rateLimitConfig.singleton.find(true)?.sweepBatch ??
        DEFAULT_SWEEP_BATCH
    );
  }
);
