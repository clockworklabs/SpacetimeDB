import {
  schema,
  table,
  t,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
  type ViewCtx,
} from 'spacetimedb/server';

export const rateLimitBucket = table(
  { name: 'rate_limit_bucket', public: false },
  {
    key: t.string().primaryKey(),
    scope: t.string().index(),
    windowStart: t.timestamp().index(),
    expiresAt: t.timestamp().index(),
    count: t.u32(),
    updatedAt: t.timestamp(),
  }
);

export const rateLimitAdminIdentity = table(
  { name: 'rate_limit_admin_identity', public: false },
  {
    identity: t.identity().primaryKey(),
    addedAtMicros: t.i64(),
  }
);

export const rateLimitConfig = table(
  { name: 'rate_limit_config', public: true },
  {
    singleton: t.bool().primaryKey(),
    sweepBatch: t.u32(),
    updatedAt: t.timestamp(),
  }
);

export const rateLimitSweepTick = table(
  { name: 'rate_limit_sweep_tick' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

export const spacetimedb = schema({
  rateLimitBucket,
  rateLimitAdminIdentity,
  rateLimitConfig,
  rateLimitSweepTick,
});
export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
export type ReducerModuleCtx = ReducerCtx<Schema>;
export type ProcedureModuleCtx = ProcedureCtx<Schema>;
export type TransactionModuleCtx = TransactionCtx<Schema>;
export type ViewModuleCtx = ViewCtx<Schema>;

export { t };
