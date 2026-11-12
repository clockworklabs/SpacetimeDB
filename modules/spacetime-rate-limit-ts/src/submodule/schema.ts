import {
  schema,
  table,
  t,
  type InferSchema,
  type ReducerCtx,
  type ViewCtx,
} from 'spacetimedb/server';

const bucketOptions = { name: 'rate_limit_bucket', public: false } as const;
const bucketColumns = {
  key: t.string().primaryKey(),
  scope: t.string().index(),
  windowStart: t.timestamp().index(),
  expiresAt: t.timestamp().index(),
  count: t.u32(),
  updatedAt: t.timestamp(),
};
export const rateLimitBucket: ReturnType<
  typeof table<typeof bucketColumns, typeof bucketOptions>
> = table(bucketOptions, bucketColumns);

const adminOptions = {
  name: 'rate_limit_admin_identity',
  public: false,
} as const;
const adminColumns = {
  identity: t.identity().primaryKey(),
  addedAt: t.timestamp(),
};
export const rateLimitAdminIdentity: ReturnType<
  typeof table<typeof adminColumns, typeof adminOptions>
> = table(adminOptions, adminColumns);

const configOptions = { name: 'rate_limit_config', public: true } as const;
const configColumns = {
  singleton: t.bool().primaryKey(),
  sweepBatch: t.u32(),
  updatedAt: t.timestamp(),
};
export const rateLimitConfig: ReturnType<
  typeof table<typeof configColumns, typeof configOptions>
> = table(configOptions, configColumns);

const sweepOptions = { name: 'rate_limit_sweep_tick' } as const;
const sweepColumns = {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
};
export const rateLimitSweepTick: ReturnType<
  typeof table<typeof sweepColumns, typeof sweepOptions>
> = table(sweepOptions, sweepColumns);

const tables: {
  rateLimitBucket: typeof rateLimitBucket;
  rateLimitAdminIdentity: typeof rateLimitAdminIdentity;
  rateLimitConfig: typeof rateLimitConfig;
  rateLimitSweepTick: typeof rateLimitSweepTick;
} = {
  rateLimitBucket,
  rateLimitAdminIdentity,
  rateLimitConfig,
  rateLimitSweepTick,
};
export const spacetimedb: ReturnType<typeof schema<typeof tables>> =
  schema(tables);
export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
export type ReducerModuleCtx = ReducerCtx<Schema>;
export type ViewModuleCtx = ViewCtx<Schema>;

export { t };
