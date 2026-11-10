import {
  Range,
  SenderError,
  type ProcedureExport,
  type ReducerExport,
  type ViewExport,
  type ArrayBuilder,
  type Infer,
} from 'spacetimedb/server';
import type { Identity } from 'spacetimedb';
import {
  DEFAULT_SWEEP_BATCH,
  MAX_SWEEP_BATCH,
  sweepRateLimits,
  errors,
} from '../limit.js';
import {
  rateLimitBucket,
  rateLimitSweepTick,
  spacetimedb,
  t,
  type ViewModuleCtx,
  type Schema,
} from './schema.js';

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

const runSweepArgs = { maxRows: t.option(t.u32()) };
const runSweepResult = t.u32();
export const runSweep: ProcedureExport<
  Schema,
  typeof runSweepArgs,
  typeof runSweepResult
> = spacetimedb.procedure(runSweepArgs, runSweepResult, (ctx, args) => {
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
});

const addRateLimitAdminArgs = { identity: t.identity() };
export const addRateLimitAdmin: ReducerExport<
  Schema,
  typeof addRateLimitAdminArgs
> = spacetimedb.reducer(addRateLimitAdminArgs, (ctx, args) => {
  requireAdmin(ctx);
  if (ctx.db.rateLimitAdminIdentity.identity.find(args.identity) == null) {
    ctx.db.rateLimitAdminIdentity.insert({
      identity: args.identity,
      addedAt: ctx.timestamp,
    });
  }
});

const removeRateLimitAdminArgs = { identity: t.identity() };
export const removeRateLimitAdmin: ReducerExport<
  Schema,
  typeof removeRateLimitAdminArgs
> = spacetimedb.reducer(removeRateLimitAdminArgs, (ctx, { identity }) => {
  requireAdmin(ctx);
  const existing = ctx.db.rateLimitAdminIdentity.identity.find(identity);
  if (!existing) return;
  if (ctx.db.rateLimitAdminIdentity.count() <= 1n) {
    throw new SenderError(errors.cannotRemoveLastAdmin);
  }
  ctx.db.rateLimitAdminIdentity.delete(existing);
});

const updateConfigArgs = { sweepBatch: t.u32() };
export const updateConfig: ReducerExport<Schema, typeof updateConfigArgs> =
  spacetimedb.reducer(updateConfigArgs, (ctx, args) => {
    requireAdmin(ctx);
    const cfg = ctx.db.rateLimitConfig.singleton.find(true);
    if (!cfg) throw new Error('rate_limit.config_missing');
    ctx.db.rateLimitConfig.singleton.update({
      ...cfg,
      sweepBatch: positiveU32(
        errors.invalidSweepBatch,
        args.sweepBatch,
        MAX_SWEEP_BATCH
      ),
      updatedAt: ctx.timestamp,
    });
  });

const resetBucketsArgs = { maxRows: t.option(t.u32()) };
export const resetBuckets: ReducerExport<Schema, typeof resetBucketsArgs> =
  spacetimedb.reducer(resetBucketsArgs, (ctx, args) => {
    requireAdmin(ctx);
    const maxRows = maxRowsArg(args.maxRows);
    let removed = 0;
    for (const row of ctx.db.rateLimitBucket.iter()) {
      if (removed >= maxRows) break;
      ctx.db.rateLimitBucket.delete(row);
      removed++;
    }
  });

const adminRateLimitBucketsResult: ArrayBuilder<
  typeof rateLimitBucket.rowType
> = t.array(rateLimitBucket.rowType);
export const adminRateLimitBuckets: ViewExport<
  (ctx: ViewModuleCtx) => Infer<typeof adminRateLimitBucketsResult>
> = spacetimedb.view(
  { name: 'admin_rate_limit_buckets', public: true },
  adminRateLimitBucketsResult,
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

const rateLimitSweepArgs: { arg: typeof rateLimitSweepTick.rowType } = {
  arg: rateLimitSweepTick.rowType,
};
export const rateLimitSweep: ReducerExport<Schema, typeof rateLimitSweepArgs> =
  spacetimedb.reducer(
    { onSchedule: rateLimitSweepTick },
    rateLimitSweepArgs,
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
