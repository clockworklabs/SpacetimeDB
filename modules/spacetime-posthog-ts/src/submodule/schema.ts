import {
  schema,
  table,
  t,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
  type ViewCtx,
  type SumBuilder,
} from 'spacetimedb/server';
import { install } from './install.js';
import { Timestamp } from 'spacetimedb';

type PostHogOutboxStatusBuilder = SumBuilder<
  Record<
    'Queued' | 'Processing' | 'Delivered' | 'Failed' | 'Rejected',
    ReturnType<typeof t.unit>
  >
>;
export const outboxStatus: PostHogOutboxStatusBuilder = t.enum(
  'PostHogOutboxStatus',
  {
    Queued: t.unit(),
    Processing: t.unit(),
    Delivered: t.unit(),
    Failed: t.unit(),
    Rejected: t.unit(),
  }
);
export const OutboxStatus = {
  Queued: { tag: 'Queued' as const },
  Processing: { tag: 'Processing' as const },
  Delivered: { tag: 'Delivered' as const },
  Failed: { tag: 'Failed' as const },
  Rejected: { tag: 'Rejected' as const },
};

type PostHogDeliverySourceBuilder = SumBuilder<
  Record<'Direct' | 'Flush' | 'FeatureFlag', ReturnType<typeof t.unit>>
>;
export const deliverySource: PostHogDeliverySourceBuilder = t.enum(
  'PostHogDeliverySource',
  { Direct: t.unit(), Flush: t.unit(), FeatureFlag: t.unit() }
);
export const DeliverySource = {
  Direct: { tag: 'Direct' as const },
  Flush: { tag: 'Flush' as const },
  FeatureFlag: { tag: 'FeatureFlag' as const },
};

const posthogConfigOptions = { name: 'posthog_config', public: false };
const posthogConfigColumns = {
  singleton: t.bool().primaryKey(),
  host: t.string(),
  projectApiKey: t.string(),
  updatedAt: t.timestamp(),
};
export const posthogConfig: ReturnType<
  typeof table<typeof posthogConfigColumns, typeof posthogConfigOptions>
> = table(posthogConfigOptions, posthogConfigColumns);

const posthogAdminIdentityOptions = {
  name: 'posthog_admin_identity',
  public: false,
};
const posthogAdminIdentityColumns = {
  identity: t.identity().primaryKey(),
  addedAt: t.timestamp(),
};
export const posthogAdminIdentity: ReturnType<
  typeof table<
    typeof posthogAdminIdentityColumns,
    typeof posthogAdminIdentityOptions
  >
> = table(posthogAdminIdentityOptions, posthogAdminIdentityColumns);

const posthogOutboxOptions = {
  name: 'posthog_outbox',
  public: false,
  indexes: [
    { accessor: 'byStatus', algorithm: 'btree', columns: ['status'] } as const,
    {
      accessor: 'byCreatedAt',
      algorithm: 'btree',
      columns: ['createdAt'],
    } as const,
    {
      accessor: 'byStatusCreatedAt',
      algorithm: 'btree',
      columns: ['status', 'createdAt'],
    } as const,
    {
      accessor: 'byStatusNextAttemptAt',
      algorithm: 'btree',
      columns: ['status', 'nextAttemptAt'],
    } as const,
    {
      accessor: 'byStatusClaimExpiresAt',
      algorithm: 'btree',
      columns: ['status', 'claimExpiresAt'],
    } as const,
    {
      accessor: 'byStatusUpdatedAt',
      algorithm: 'btree',
      columns: ['status', 'updatedAt'],
    } as const,
    {
      accessor: 'byStatusRetryExpiresAt',
      algorithm: 'btree',
      columns: ['status', 'retryExpiresAt'],
    } as const,
  ],
};
const posthogOutboxColumns = {
  outboxId: t.string().primaryKey(),
  uuid: t.string(),
  idempotencyKey: t.option(t.string()),
  distinctId: t.string(),
  event: t.string(),
  propertiesJson: t.option(t.string()),
  status: outboxStatus,
  attempts: t.u32(),
  claimId: t.option(t.string()),
  claimExpiresAt: t.timestamp(),
  nextAttemptAt: t.timestamp(),
  lastStatusCode: t.option(t.u16()),
  lastError: t.option(t.string()),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
  deliveredAt: t.option(t.timestamp()),
  retryExpiresAt: t.timestamp().default(Timestamp.UNIX_EPOCH),
};
export const posthogOutbox: ReturnType<
  typeof table<typeof posthogOutboxColumns, typeof posthogOutboxOptions>
> = table(posthogOutboxOptions, posthogOutboxColumns);

const posthogFlushTickOptions = { name: 'posthog_flush_tick', public: false };
const posthogFlushTickColumns = {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
};
export const posthogFlushTick: ReturnType<
  typeof table<typeof posthogFlushTickColumns, typeof posthogFlushTickOptions>
> = table(posthogFlushTickOptions, posthogFlushTickColumns);

// Counts of outbox rows by state. pending covers Queued and Processing;
// failed covers Failed and Rejected.
const posthogDeliveryStatsOptions = {
  name: 'posthog_delivery_stats',
  public: false,
};
const posthogDeliveryStatsColumns = {
  singleton: t.bool().primaryKey(),
  pending: t.u64(),
  delivered: t.u64(),
  failed: t.u64(),
  updatedAt: t.timestamp(),
};
export const posthogDeliveryStats: ReturnType<
  typeof table<
    typeof posthogDeliveryStatsColumns,
    typeof posthogDeliveryStatsOptions
  >
> = table(posthogDeliveryStatsOptions, posthogDeliveryStatsColumns);

const posthogDeliveryLogOptions = {
  name: 'posthog_delivery_log',
  public: false,
  indexes: [
    {
      accessor: 'byAttemptedAt',
      algorithm: 'btree',
      columns: ['attemptedAt'],
    } as const,
    {
      accessor: 'byAttemptedAtOrder',
      algorithm: 'btree',
      columns: ['attemptedAtOrder'],
    } as const,
    { accessor: 'byOk', algorithm: 'btree', columns: ['ok'] } as const,
  ],
};
const posthogDeliveryLogColumns = {
  deliveryId: t.u64().primaryKey().autoInc(),
  source: deliverySource,
  outboxId: t.option(t.string()),
  distinctId: t.string(),
  event: t.string(),
  ok: t.bool(),
  statusCode: t.u16(),
  responseBody: t.string(),
  errorMessage: t.option(t.string()),
  attemptedAt: t.timestamp(),
  attemptedAtOrder: t.i64(),
};
export const posthogDeliveryLog: ReturnType<
  typeof table<
    typeof posthogDeliveryLogColumns,
    typeof posthogDeliveryLogOptions
  >
> = table(posthogDeliveryLogOptions, posthogDeliveryLogColumns);

export const posthogDeliveryLogRow = t.object('PostHogDeliveryLogRow', {
  deliveryId: t.u64(),
  source: deliverySource,
  outboxId: t.option(t.string()),
  distinctId: t.string(),
  event: t.string(),
  ok: t.bool(),
  statusCode: t.u16(),
  responseBody: t.string(),
  errorMessage: t.option(t.string()),
  attemptedAt: t.timestamp(),
});

const tables: {
  posthogConfig: typeof posthogConfig;
  posthogAdminIdentity: typeof posthogAdminIdentity;
  posthogOutbox: typeof posthogOutbox;
  posthogDeliveryLog: typeof posthogDeliveryLog;
  posthogDeliveryStats: typeof posthogDeliveryStats;
  posthogFlushTick: typeof posthogFlushTick;
} = {
  posthogConfig,
  posthogAdminIdentity,
  posthogOutbox,
  posthogDeliveryLog,
  posthogDeliveryStats,
  posthogFlushTick,
};
export const spacetimedb: ReturnType<typeof schema<typeof tables>> =
  schema(tables);

export const init: ReturnType<typeof spacetimedb.init> = spacetimedb.init(
  ctx => {
    install(ctx);
  }
);

export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
export type ReducerModuleCtx = ReducerCtx<Schema>;
export type ProcedureModuleCtx = ProcedureCtx<Schema>;
export type TransactionModuleCtx = TransactionCtx<Schema>;
export type ViewModuleCtx = ViewCtx<Schema>;
export type WriteCtx = ReducerModuleCtx | TransactionModuleCtx;

export { t };
