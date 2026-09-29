import { Range } from 'spacetimedb/server';
import { Timestamp } from 'spacetimedb';
import {
  DeliverySource,
  OutboxStatus,
  posthogDeliveryLogRow,
  posthogFlushTick,
  posthogOutbox,
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type ViewModuleCtx,
  type WriteCtx,
} from './schema';
import {
  loadConfig,
  loadConfigOrThrowFromProcedure,
  type PostHogConfig,
} from './config';
import {
  featureFlagValue,
  posthogFetch,
  truncateForLog,
  type PostHogHttpResult,
} from './http';
import { isAdmin, requireAdmin } from './auth';
import { errors, parseJsonObject, throwSenderError } from './validation';
import {
  claimHasExpired,
  claimOutboxRow,
  releaseExpiredClaim,
  requeueFailedRow,
  retryDelayMicros,
  settleOutboxClaim,
} from './outbox-state';

const MAX_FLUSH_LIMIT = 100;
const MAX_FLUSH_BATCHES_PER_TICK = 10;
const CLAIM_TTL_MICROS = 5n * 60n * 1_000_000n;
const MAX_EXPIRED_CLAIMS_PER_FLUSH = 1_000;
const MAX_DISTINCT_ID_LENGTH = 256;
const MAX_EVENT_NAME_LENGTH = 200;
const MAX_PROPERTIES_JSON_LENGTH = 64 * 1024;
const MAX_IDEMPOTENCY_KEY_LENGTH = 256;
const HISTORY_RETENTION_MICROS = 30n * 24n * 60n * 60n * 1_000_000n;
const MAX_RETENTION_ROWS_PER_TICK = 1_000;
const MAX_ADMIN_BATCH = 10_000;

function takeRows<T>(rows: Iterable<T>, limit: number): T[] {
  const out: T[] = [];
  for (const row of rows) {
    if (out.length >= limit) break;
    out.push(row);
  }
  return out;
}

type StatsDelta = { pending: bigint; delivered: bigint; failed: bigint };

function emptyDelta(): StatsDelta {
  return { pending: 0n, delivered: 0n, failed: 0n };
}

function statsKey(status: { tag: string }): keyof StatsDelta {
  if (status.tag === 'Delivered') return 'delivered';
  if (status.tag === 'Failed' || status.tag === 'Rejected') return 'failed';
  return 'pending';
}

/** Records one outbox row entering, leaving, or changing state. */
function countStatusChange(
  delta: StatsDelta,
  from: { tag: string } | undefined,
  to: { tag: string } | undefined
): void {
  if (from) delta[statsKey(from)] -= 1n;
  if (to) delta[statsKey(to)] += 1n;
}

function applyStatsDelta(ctx: WriteCtx, delta: StatsDelta): void {
  if (!delta.pending && !delta.delivered && !delta.failed) return;
  const existing = ctx.db.posthogDeliveryStats.singleton.find(true);
  const current = existing ?? {
    singleton: true,
    pending: 0n,
    delivered: 0n,
    failed: 0n,
    updatedAt: ctx.timestamp,
  };
  const adjust = (value: bigint, change: bigint) => {
    const next = value + change;
    return next < 0n ? 0n : next;
  };
  const row = {
    ...current,
    pending: adjust(current.pending, delta.pending),
    delivered: adjust(current.delivered, delta.delivered),
    failed: adjust(current.failed, delta.failed),
    updatedAt: ctx.timestamp,
  };
  if (existing) ctx.db.posthogDeliveryStats.singleton.update(row);
  else ctx.db.posthogDeliveryStats.insert(row);
}

export type EnqueueEventArgs = {
  distinctId: string;
  event: string;
  propertiesJson?: string | undefined;
  idempotencyKey?: string | undefined;
};

export type EnqueueEventResult = {
  outboxId: string;
  inserted: boolean;
  error: string | undefined;
};

type CaptureEventArgs = {
  distinctId: string;
  event: string;
  propertiesJson?: string | undefined;
};

function eventInputError(args: CaptureEventArgs): string | undefined {
  const distinctId = args.distinctId.trim();
  const event = args.event.trim();
  if (!distinctId || distinctId.length > MAX_DISTINCT_ID_LENGTH) {
    return errors.invalidDistinctId;
  }
  if (!event || event.length > MAX_EVENT_NAME_LENGTH) {
    return errors.invalidEvent;
  }
  if (args.propertiesJson !== undefined) {
    if (args.propertiesJson.length > MAX_PROPERTIES_JSON_LENGTH) {
      return errors.propertiesTooLarge;
    }
    if (!parseJsonObject(args.propertiesJson)) {
      return errors.invalidPropertiesJson;
    }
  }
  return undefined;
}

function optionalJsonObject(
  json: string | undefined,
  tooLargeError: string,
  invalidError: string
): object | undefined {
  if (json === undefined) return undefined;
  if (json.length > MAX_PROPERTIES_JSON_LENGTH) throwSenderError(tooLargeError);
  return parseJsonObject(json) ?? throwSenderError(invalidError);
}

type BatchEvent = CaptureEventArgs & { uuid: string; createdAt: Timestamp };

function buildBatchBody(projectApiKey: string, events: BatchEvent[]) {
  return {
    api_key: projectApiKey,
    batch: events.map(event => ({
      uuid: event.uuid,
      timestamp: event.createdAt.toISOString(),
      distinct_id: event.distinctId,
      event: event.event,
      properties:
        (event.propertiesJson === undefined
          ? undefined
          : parseJsonObject(event.propertiesJson)) ?? {},
    })),
  };
}

/**
 * Queues an event in the caller's transaction. Invalid input does not throw,
 * so analytics cannot roll back the host reducer: the event is stored as
 * `Rejected` with the error code in `lastError`, and the code is returned as
 * `error`.
 */
export function enqueueEventInTx(
  ctx: WriteCtx,
  args: EnqueueEventArgs
): EnqueueEventResult {
  const idempotencyKey = args.idempotencyKey?.trim() || undefined;
  const error =
    eventInputError(args) ??
    (idempotencyKey && idempotencyKey.length > MAX_IDEMPOTENCY_KEY_LENGTH
      ? errors.idempotencyKeyTooLong
      : undefined);
  const uuid = ctx.newUuidV7().toString();
  const outboxId =
    idempotencyKey && !error ? `idem:${idempotencyKey}` : `evt:${uuid}`;
  if (ctx.db.posthogOutbox.outboxId.find(outboxId)) {
    return { outboxId, inserted: false, error: undefined };
  }
  const status = error ? OutboxStatus.Rejected : OutboxStatus.Queued;
  ctx.db.posthogOutbox.insert({
    outboxId,
    uuid,
    idempotencyKey: error ? undefined : idempotencyKey,
    distinctId: args.distinctId.trim().slice(0, MAX_DISTINCT_ID_LENGTH),
    event: args.event.trim().slice(0, MAX_EVENT_NAME_LENGTH),
    propertiesJson: error ? undefined : args.propertiesJson,
    status,
    attempts: 0,
    claimId: undefined,
    claimExpiresAtMicros: 0n,
    nextAttemptAt: ctx.timestamp,
    lastStatusCode: undefined,
    lastError: error,
    createdAt: ctx.timestamp,
    updatedAt: ctx.timestamp,
    deliveredAt: undefined,
  });
  const delta = emptyDelta();
  countStatusChange(delta, undefined, status);
  applyStatsDelta(ctx, delta);
  return { outboxId, inserted: true, error };
}

function pruneDeliveryHistory(ctx: WriteCtx): void {
  const expired = new Range(undefined, {
    tag: 'included',
    value: new Timestamp(
      ctx.timestamp.microsSinceUnixEpoch - HISTORY_RETENTION_MICROS
    ),
  });
  const delta = emptyDelta();
  let budget = MAX_RETENTION_ROWS_PER_TICK;
  for (const status of [
    OutboxStatus.Delivered,
    OutboxStatus.Failed,
    OutboxStatus.Rejected,
  ]) {
    const rows = takeRows(
      ctx.db.posthogOutbox.byStatusUpdatedAt.filter([status, expired]),
      budget
    );
    for (const row of rows) {
      ctx.db.posthogOutbox.delete(row);
      countStatusChange(delta, row.status, undefined);
    }
    budget -= rows.length;
  }
  for (const row of takeRows(
    ctx.db.posthogDeliveryLog.byAttemptedAt.filter(expired),
    budget
  )) {
    ctx.db.posthogDeliveryLog.delete(row);
  }
  applyStatsDelta(ctx, delta);
}

function logDelivery(
  ctx: WriteCtx,
  source: (typeof DeliverySource)[keyof typeof DeliverySource],
  outboxId: string | undefined,
  event: CaptureEventArgs,
  result: PostHogHttpResult
) {
  const responseBody = truncateForLog(result.responseBody);
  ctx.db.posthogDeliveryLog.insert({
    deliveryId: 0n,
    source,
    outboxId,
    distinctId: event.distinctId,
    event: event.event,
    ok: result.ok,
    statusCode: result.statusCode,
    responseBody,
    errorMessage: result.ok ? undefined : responseBody,
    attemptedAt: ctx.timestamp,
    attemptedAtOrder: -ctx.timestamp.microsSinceUnixEpoch,
  });
}

function claimQueuedRows(ctx: WriteCtx, limit: number) {
  const nowMicros = ctx.timestamp.microsSinceUnixEpoch;
  const expiredClaims = takeRows(
    ctx.db.posthogOutbox.byStatusClaimExpiresAtMicros.filter([
      OutboxStatus.Processing,
      new Range(undefined, { tag: 'included', value: nowMicros }),
    ]),
    MAX_EXPIRED_CLAIMS_PER_FLUSH
  );
  for (const row of expiredClaims) {
    if (!claimHasExpired(row, nowMicros)) continue;
    ctx.db.posthogOutbox.outboxId.update(
      releaseExpiredClaim(row, ctx.timestamp)
    );
  }

  const rows = takeRows(
    ctx.db.posthogOutbox.byStatusNextAttemptAt.filter([
      OutboxStatus.Queued,
      new Range(undefined, { tag: 'included', value: ctx.timestamp }),
    ]),
    limit
  );
  const claimId = ctx.newUuidV7().toString();
  const claimed = rows.map(row =>
    claimOutboxRow(row, claimId, nowMicros + CLAIM_TTL_MICROS, ctx.timestamp)
  );
  for (const row of claimed) ctx.db.posthogOutbox.outboxId.update(row);
  return { claimId, rows: claimed };
}

const flushResult = t.object('PostHogFlushResult', {
  attempted: t.u32(),
  delivered: t.u32(),
  failed: t.u32(),
});

function deliverOutbox(
  ctx: ProcedureModuleCtx,
  cfg: PostHogConfig,
  limit: number
) {
  const claim = ctx.withTx(tx => claimQueuedRows(tx, limit));
  const rows = claim.rows;
  if (rows.length === 0) {
    return { attempted: 0, delivered: 0, failed: 0 };
  }

  const result = posthogFetch(
    ctx,
    cfg,
    '/batch',
    buildBatchBody(cfg.projectApiKey, rows)
  );

  return ctx.withTx(tx => {
    let delivered = 0;
    let failed = 0;
    const delta = emptyDelta();
    for (const row of rows) {
      const current = tx.db.posthogOutbox.outboxId.find(row.outboxId);
      if (
        !current ||
        current.status.tag !== 'Processing' ||
        current.claimId !== claim.claimId
      )
        continue;
      logDelivery(tx, DeliverySource.Flush, row.outboxId, row, result);
      const retryAt = new Timestamp(
        ctx.timestamp.microsSinceUnixEpoch +
          retryDelayMicros(current.attempts + 1)
      );
      const settled = settleOutboxClaim(
        current,
        result,
        ctx.timestamp,
        retryAt
      );
      tx.db.posthogOutbox.outboxId.update(settled);
      countStatusChange(delta, current.status, settled.status);
      if (result.ok) delivered++;
      else failed++;
    }
    applyStatsDelta(tx, delta);
    return { attempted: rows.length, delivered, failed };
  });
}

/** Runs on the interval started by `install`: prunes history and delivers queued events. */
export const scheduledFlush = spacetimedb.procedure(
  { onSchedule: posthogFlushTick },
  { tick: posthogFlushTick.rowType },
  t.unit(),
  ctx => {
    if (!ctx.sender.isEqual(ctx.identity)) {
      throwSenderError(errors.notAuthorized);
    }
    const cfg = ctx.withTx(tx => {
      pruneDeliveryHistory(tx);
      return loadConfig(tx);
    });
    if (!cfg) return {};
    // Continue only while full batches succeed, so an outage costs one request per run.
    for (let batch = 0; batch < MAX_FLUSH_BATCHES_PER_TICK; batch++) {
      if (deliverOutbox(ctx, cfg, MAX_FLUSH_LIMIT).delivered < MAX_FLUSH_LIMIT)
        break;
    }
    return {};
  }
);

export const enqueueEvent = spacetimedb.reducer(
  {
    distinctId: t.string(),
    event: t.string(),
    propertiesJson: t.option(t.string()),
    idempotencyKey: t.option(t.string()),
  },
  (ctx, args) => {
    requireAdmin(ctx, ctx.sender);
    const { error } = enqueueEventInTx(ctx, args);
    if (error) throwSenderError(error);
  }
);

export const captureNow = spacetimedb.procedure(
  {
    distinctId: t.string(),
    event: t.string(),
    propertiesJson: t.option(t.string()),
  },
  t.object('PostHogCaptureResult', {
    ok: t.bool(),
    statusCode: t.u16(),
    error: t.option(t.string()),
  }),
  (ctx, args) => {
    ctx.withTx(tx => requireAdmin(tx, ctx.sender));
    const error = eventInputError(args);
    if (error) throwSenderError(error);
    const cfg = loadConfigOrThrowFromProcedure(ctx);
    const result = posthogFetch(
      ctx,
      cfg,
      '/batch',
      buildBatchBody(cfg.projectApiKey, [
        { ...args, uuid: ctx.newUuidV7().toString(), createdAt: ctx.timestamp },
      ])
    );
    ctx.withTx(tx =>
      logDelivery(tx, DeliverySource.Direct, undefined, args, result)
    );
    return {
      ok: result.ok,
      statusCode: result.statusCode,
      error: result.ok ? undefined : truncateForLog(result.responseBody),
    };
  }
);

export const flushOutbox = spacetimedb.procedure(
  { limit: t.u32() },
  flushResult,
  (ctx, { limit }) => {
    ctx.withTx(tx => requireAdmin(tx, ctx.sender));
    if (limit <= 0 || limit > MAX_FLUSH_LIMIT) {
      throwSenderError(errors.invalidFlushLimit);
    }
    return deliverOutbox(ctx, loadConfigOrThrowFromProcedure(ctx), limit);
  }
);

/** Moves up to `limit` Failed events back to Queued with a fresh retry budget. */
export const requeueFailedEvents = spacetimedb.reducer(
  { limit: t.u32() },
  (ctx, { limit }) => {
    requireAdmin(ctx, ctx.sender);
    if (limit <= 0 || limit > MAX_ADMIN_BATCH) {
      throwSenderError(errors.invalidRequeueLimit);
    }
    const delta = emptyDelta();
    for (const row of takeRows(
      ctx.db.posthogOutbox.byStatus.filter(OutboxStatus.Failed),
      limit
    )) {
      const requeued = requeueFailedRow(row, ctx.timestamp);
      ctx.db.posthogOutbox.outboxId.update(requeued);
      countStatusChange(delta, row.status, requeued.status);
    }
    applyStatsDelta(ctx, delta);
  }
);

/** Deletes up to `maxRows` outbox and delivery log rows. Events already received by PostHog remain there. */
export const clearAnalytics = spacetimedb.reducer(
  { maxRows: t.u32() },
  (ctx, { maxRows }) => {
    requireAdmin(ctx, ctx.sender);
    if (maxRows <= 0 || maxRows > MAX_ADMIN_BATCH) {
      throwSenderError(errors.invalidClearBatch);
    }
    const delta = emptyDelta();
    const outboxRows = takeRows(ctx.db.posthogOutbox.iter(), maxRows);
    for (const row of outboxRows) {
      ctx.db.posthogOutbox.delete(row);
      countStatusChange(delta, row.status, undefined);
    }
    for (const row of takeRows(
      ctx.db.posthogDeliveryLog.iter(),
      maxRows - outboxRows.length
    )) {
      ctx.db.posthogDeliveryLog.delete(row);
    }
    applyStatsDelta(ctx, delta);
  }
);

export const getFeatureFlag = spacetimedb.procedure(
  {
    key: t.string(),
    distinctId: t.string(),
    personPropertiesJson: t.option(t.string()),
    groupsJson: t.option(t.string()),
  },
  t.object('PostHogFeatureFlagResult', {
    ok: t.bool(),
    statusCode: t.u16(),
    enabled: t.option(t.bool()),
    variant: t.option(t.string()),
    error: t.option(t.string()),
  }),
  (ctx, args) => {
    ctx.withTx(tx => requireAdmin(tx, ctx.sender));
    if (!args.key.trim() || args.key.length > MAX_EVENT_NAME_LENGTH) {
      throwSenderError(errors.invalidFlagKey);
    }
    if (
      !args.distinctId.trim() ||
      args.distinctId.length > MAX_DISTINCT_ID_LENGTH
    ) {
      throwSenderError(errors.invalidDistinctId);
    }
    const personProperties = optionalJsonObject(
      args.personPropertiesJson,
      errors.personPropertiesTooLarge,
      errors.invalidPersonPropertiesJson
    );
    const groups = optionalJsonObject(
      args.groupsJson,
      errors.groupsTooLarge,
      errors.invalidGroupsJson
    );
    const cfg = loadConfigOrThrowFromProcedure(ctx);
    const result = posthogFetch(ctx, cfg, '/flags?v=2', {
      api_key: cfg.projectApiKey,
      distinct_id: args.distinctId,
      person_properties: personProperties,
      groups,
    });
    const value = result.ok
      ? featureFlagValue(result.responseBody, args.key)
      : undefined;
    ctx.withTx(tx => {
      logDelivery(
        tx,
        DeliverySource.FeatureFlag,
        undefined,
        { distinctId: args.distinctId, event: `$feature_flag:${args.key}` },
        result
      );
    });
    return {
      ok: result.ok,
      statusCode: result.statusCode,
      enabled: value === undefined ? undefined : value !== false,
      variant: typeof value === 'string' ? value : undefined,
      error: result.ok ? undefined : truncateForLog(result.responseBody),
    };
  }
);

function viewIsAdmin(ctx: ViewModuleCtx): boolean {
  return isAdmin(ctx, ctx.sender);
}

export const posthogOutboxAdmin = spacetimedb.view(
  { name: 'posthog_outbox_admin', public: true },
  t.array(posthogOutbox.rowType),
  ctx => {
    if (!viewIsAdmin(ctx)) return [];
    const rows = takeRows(
      ctx.db.posthogOutbox.byStatus.filter(OutboxStatus.Processing),
      500
    );
    if (rows.length < 500) {
      rows.push(
        ...takeRows(
          ctx.db.posthogOutbox.byStatus.filter(OutboxStatus.Queued),
          500 - rows.length
        )
      );
    }
    return rows;
  }
);

export const posthogDeliveryLogAdmin = spacetimedb.view(
  { name: 'posthog_delivery_log_admin', public: true },
  t.array(posthogDeliveryLogRow),
  ctx => {
    if (!viewIsAdmin(ctx)) return [];
    return takeRows(
      ctx.db.posthogDeliveryLog.byAttemptedAtOrder.filter(new Range()),
      50
    ).map(row => ({
      deliveryId: row.deliveryId,
      source: row.source,
      outboxId: row.outboxId,
      distinctId: row.distinctId,
      event: row.event,
      ok: row.ok,
      statusCode: row.statusCode,
      responseBody: row.responseBody,
      errorMessage: row.errorMessage,
      attemptedAt: row.attemptedAt,
    }));
  }
);
