import {
  SenderError,
  schema,
  table,
  t,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
  type ViewCtx,
} from 'spacetimedb/server';
import { installPostHog } from './install';

export const outboxStatus = t.enum('PostHogOutboxStatus', [
  'Queued',
  'Processing',
  'Delivered',
  'Failed',
]);
export const OutboxStatus = {
  Queued: { tag: 'Queued' as const },
  Processing: { tag: 'Processing' as const },
  Delivered: { tag: 'Delivered' as const },
  Failed: { tag: 'Failed' as const },
};

export const deliverySource = t.enum('PostHogDeliverySource', [
  'Direct',
  'Flush',
  'FeatureFlag',
]);
export const DeliverySource = {
  Direct: { tag: 'Direct' as const },
  Flush: { tag: 'Flush' as const },
  FeatureFlag: { tag: 'FeatureFlag' as const },
};

export const posthogConfig = table(
  { name: 'posthog_config', public: false },
  {
    singleton: t.bool().primaryKey(),
    host: t.string(),
    projectApiKey: t.string(),
    updatedAt: t.timestamp(),
  }
);

export const posthogAdminIdentity = table(
  { name: 'posthog_admin_identity', public: false },
  {
    identity: t.identity().primaryKey(),
    addedAtMicros: t.i64(),
  }
);

export const posthogOutbox = table(
  {
    name: 'posthog_outbox',
    public: false,
    indexes: [
      { accessor: 'byStatus', algorithm: 'btree', columns: ['status'] },
      { accessor: 'byCreatedAt', algorithm: 'btree', columns: ['createdAt'] },
      {
        accessor: 'byStatusCreatedAt',
        algorithm: 'btree',
        columns: ['status', 'createdAt'],
      },
      {
        accessor: 'byStatusNextAttemptAt',
        algorithm: 'btree',
        columns: ['status', 'nextAttemptAt'],
      },
      {
        accessor: 'byStatusClaimExpiresAtMicros',
        algorithm: 'btree',
        columns: ['status', 'claimExpiresAtMicros'],
      },
      {
        accessor: 'byStatusUpdatedAt',
        algorithm: 'btree',
        columns: ['status', 'updatedAt'],
      },
    ],
  },
  {
    outboxId: t.string().primaryKey(),
    idempotencyKey: t.option(t.string()),
    distinctId: t.string(),
    event: t.string(),
    propertiesJson: t.option(t.string()),
    status: outboxStatus,
    attempts: t.u32(),
    claimId: t.option(t.string()),
    claimExpiresAtMicros: t.i64(),
    nextAttemptAt: t.timestamp(),
    lastStatusCode: t.option(t.u16()),
    lastError: t.option(t.string()),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
    deliveredAt: t.option(t.timestamp()),
  }
);

export const posthogDeliveryStats = table(
  { name: 'posthog_delivery_stats', public: false },
  {
    singleton: t.bool().primaryKey(),
    pending: t.u64(),
    delivered: t.u64(),
    failed: t.u64(),
    updatedAt: t.timestamp(),
  }
);

export const posthogDeliveryLog = table(
  {
    name: 'posthog_delivery_log',
    public: false,
    indexes: [
      {
        accessor: 'byAttemptedAt',
        algorithm: 'btree',
        columns: ['attemptedAt'],
      },
      {
        accessor: 'byAttemptedAtOrder',
        algorithm: 'btree',
        columns: ['attemptedAtOrder'],
      },
      { accessor: 'byOk', algorithm: 'btree', columns: ['ok'] },
    ],
  },
  {
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
  }
);

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

export const spacetimedb = schema({
  posthogConfig,
  posthogAdminIdentity,
  posthogOutbox,
  posthogDeliveryLog,
  posthogDeliveryStats,
});

export const init = spacetimedb.init(ctx => {
  installPostHog(ctx);
});

export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
export type ReducerModuleCtx = ReducerCtx<Schema>;
export type ProcedureModuleCtx = ProcedureCtx<Schema>;
export type TransactionModuleCtx = TransactionCtx<Schema>;
export type ViewModuleCtx = ViewCtx<Schema>;
export type WriteCtx = ReducerModuleCtx | TransactionModuleCtx;

export { SenderError, t };
