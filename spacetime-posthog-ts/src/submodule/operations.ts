import { Range } from 'spacetimedb/server';
import { Timestamp } from 'spacetimedb';
import {
  DeliverySource,
  OutboxStatus,
  deliverySource,
  posthogDeliveryLogRow,
  posthogOutbox,
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type ViewModuleCtx,
  type WriteCtx,
} from './schema';
import { loadConfigOrThrowFromProcedure } from './config';
import { posthogFetch, type PostHogHttpResult } from './http';
import { isAdmin, requireAdmin } from './auth';
import { isOkStatus, parseJsonObject, throwSenderError } from './utils';
import {
  claimHasExpired,
  claimOutboxRow,
  releaseExpiredClaim,
  retryDelayMicros,
  settleOutboxClaim,
} from './outbox-state';

const DEFAULT_FLUSH_LIMIT = 25;
const MAX_FLUSH_LIMIT = 100;
const CLAIM_TTL_MICROS = 5n * 60n * 1_000_000n;
const MAX_EXPIRED_CLAIMS_PER_FLUSH = 1_000;
const MAX_DISTINCT_ID_LENGTH = 256;
const MAX_EVENT_NAME_LENGTH = 200;
const MAX_PROPERTIES_JSON_LENGTH = 64 * 1024;
const MAX_IDEMPOTENCY_KEY_LENGTH = 256;
const HISTORY_RETENTION_MICROS = 30n * 24n * 60n * 60n * 1_000_000n;
const MAX_RETENTION_ROWS_PER_CALL = 100;

function takeRows<T>(rows: Iterable<T>, limit: number): T[] {
  const out: T[] = [];
  for (const row of rows) {
    if (out.length >= limit) break;
    out.push(row);
  }
  return out;
}

function updateDeliveryStats(
  ctx: WriteCtx,
  delta: { pending?: bigint; delivered?: bigint; failed?: bigint }
): void {
  const existing = ctx.db.posthogDeliveryStats.singleton.find(true);
  const current = existing ?? {
    singleton: true,
    pending: 0n,
    delivered: 0n,
    failed: 0n,
    updatedAt: ctx.timestamp,
  };
  const adjust = (value: bigint, change = 0n) => {
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

export type CaptureEventArgs = {
  distinctId: string;
  event: string;
  propertiesJson?: string | undefined;
};

function validateEventInput(args: CaptureEventArgs): void {
  const distinctId = args.distinctId.trim();
  const event = args.event.trim();
  if (!distinctId || distinctId.length > MAX_DISTINCT_ID_LENGTH) {
    throwSenderError('posthog.invalid_distinct_id');
  }
  if (!event || event.length > MAX_EVENT_NAME_LENGTH) {
    throwSenderError('posthog.invalid_event');
  }
  if ((args.propertiesJson?.length ?? 0) > MAX_PROPERTIES_JSON_LENGTH) {
    throwSenderError('posthog.properties_too_large');
  }
  parseJsonObject(args.propertiesJson, 'properties');
}

function buildBatchBody(projectApiKey: string, events: CaptureEventArgs[]) {
  return {
    api_key: projectApiKey,
    batch: events.map(event => ({
      distinct_id: event.distinctId,
      event: event.event,
      properties: parseJsonObject(event.propertiesJson, 'properties') ?? {},
    })),
  };
}

function outboxIdFor(
  ctx: WriteCtx,
  idempotencyKey: string | undefined
): string {
  if (idempotencyKey !== undefined && idempotencyKey.trim()) {
    return `idem:${idempotencyKey.trim()}`;
  }
  return `evt:${ctx.newUuidV7().toString()}`;
}

export function enqueueEvent(ctx: WriteCtx, args: EnqueueEventArgs) {
  validateEventInput(args);
  if ((args.idempotencyKey?.length ?? 0) > MAX_IDEMPOTENCY_KEY_LENGTH) {
    throwSenderError('posthog.idempotency_key_too_long');
  }
  const outboxId = outboxIdFor(ctx, args.idempotencyKey);
  const existing = ctx.db.posthogOutbox.outboxId.find(outboxId);
  if (existing) {
    return { outboxId, inserted: false };
  }
  ctx.db.posthogOutbox.insert({
    outboxId,
    idempotencyKey: args.idempotencyKey,
    distinctId: args.distinctId,
    event: args.event,
    propertiesJson: args.propertiesJson,
    status: OutboxStatus.Queued,
    attempts: 0,
    claimId: undefined,
    claimExpiresAtMicros: 0n,
    nextAttemptAt: ctx.timestamp,
    lastStatusCode: undefined,
    lastError: undefined,
    createdAt: ctx.timestamp,
    updatedAt: ctx.timestamp,
    deliveredAt: undefined,
  });
  updateDeliveryStats(ctx, { pending: 1n });
  return { outboxId, inserted: true };
}

function pruneDeliveryHistory(
  ctx: WriteCtx,
  maxRows = MAX_RETENTION_ROWS_PER_CALL
): number {
  const cutoff = new Timestamp(
    ctx.timestamp.microsSinceUnixEpoch - HISTORY_RETENTION_MICROS
  );
  let removed = 0;
  for (const status of [OutboxStatus.Delivered, OutboxStatus.Failed]) {
    for (const row of ctx.db.posthogOutbox.byStatusUpdatedAt.filter([
      status,
      new Range(undefined, { tag: 'included', value: cutoff }),
    ])) {
      if (removed >= maxRows) return removed;
      ctx.db.posthogOutbox.delete(row);
      removed++;
    }
  }
  for (const row of ctx.db.posthogDeliveryLog.byAttemptedAt.filter(
    new Range(undefined, { tag: 'included', value: cutoff })
  )) {
    if (removed >= maxRows) break;
    ctx.db.posthogDeliveryLog.delete(row);
    removed++;
  }
  return removed;
}

// Remove queued and delivered outbox entries plus the delivery log for bounded
// demo and test resets. Events received by PostHog remain at the provider.
export function clearAnalytics(
  ctx: WriteCtx,
  maxRows = 1000
): { outbox: number; deliveries: number } {
  if (!Number.isInteger(maxRows) || maxRows <= 0 || maxRows > 10_000) {
    throwSenderError('posthog.invalid_clear_batch');
  }
  let outbox = 0;
  let deliveries = 0;
  let pendingRemoved = 0n;
  let deliveredRemoved = 0n;
  let failedRemoved = 0n;
  for (const row of ctx.db.posthogOutbox.iter()) {
    if (outbox + deliveries >= maxRows) break;
    if (row.status.tag === 'Queued' || row.status.tag === 'Processing')
      pendingRemoved += 1n;
    ctx.db.posthogOutbox.delete(row);
    outbox++;
  }
  for (const row of ctx.db.posthogDeliveryLog.iter()) {
    if (outbox + deliveries >= maxRows) break;
    if (row.ok) deliveredRemoved += 1n;
    else failedRemoved += 1n;
    ctx.db.posthogDeliveryLog.delete(row);
    deliveries++;
  }
  updateDeliveryStats(ctx, {
    pending: -pendingRemoved,
    delivered: -deliveredRemoved,
    failed: -failedRemoved,
  });
  return { outbox, deliveries };
}

function logDelivery(
  ctx: WriteCtx,
  source: (typeof DeliverySource)[keyof typeof DeliverySource],
  outboxId: string | undefined,
  event: CaptureEventArgs,
  result: PostHogHttpResult
) {
  ctx.db.posthogDeliveryLog.insert({
    deliveryId: 0n,
    source,
    outboxId,
    distinctId: event.distinctId,
    event: event.event,
    ok: result.ok,
    statusCode: result.statusCode,
    responseBody: result.responseBody,
    errorMessage: result.ok ? undefined : result.responseBody,
    attemptedAt: ctx.timestamp,
    attemptedAtOrder: -ctx.timestamp.microsSinceUnixEpoch,
  });
  updateDeliveryStats(ctx, result.ok ? { delivered: 1n } : { failed: 1n });
}

export function captureNow(
  ctx: ProcedureModuleCtx,
  args: CaptureEventArgs
): PostHogHttpResult {
  validateEventInput(args);
  const cfg = loadConfigOrThrowFromProcedure(ctx);
  const result = posthogFetch(
    ctx,
    cfg,
    '/batch',
    buildBatchBody(cfg.projectApiKey, [args])
  );
  ctx.withTx(tx => {
    logDelivery(tx, DeliverySource.Direct, undefined, args, result);
    pruneDeliveryHistory(tx);
  });
  return result;
}

function claimQueuedRows(ctx: WriteCtx, limit: number) {
  const nowMicros = ctx.timestamp.microsSinceUnixEpoch;
  let inspected = 0;
  for (const row of ctx.db.posthogOutbox.byStatusClaimExpiresAtMicros.filter([
    OutboxStatus.Processing,
    new Range(undefined, { tag: 'included', value: nowMicros }),
  ])) {
    if (inspected >= MAX_EXPIRED_CLAIMS_PER_FLUSH) break;
    inspected++;
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

export function flushOutbox(
  ctx: ProcedureModuleCtx,
  args: { limit?: number | undefined }
) {
  const rawLimit = args.limit ?? DEFAULT_FLUSH_LIMIT;
  if (
    !Number.isInteger(rawLimit) ||
    rawLimit <= 0 ||
    rawLimit > MAX_FLUSH_LIMIT
  ) {
    throwSenderError('posthog.invalid_flush_limit');
  }
  const cfg = loadConfigOrThrowFromProcedure(ctx);
  const claim = ctx.withTx(tx => {
    requireAdmin(tx, ctx.sender);
    return claimQueuedRows(tx, rawLimit);
  });
  const rows = claim.rows;
  if (rows.length === 0) {
    return { attempted: 0, delivered: 0, failed: 0 };
  }

  const events = rows.map(row => ({
    distinctId: row.distinctId,
    event: row.event,
    propertiesJson: row.propertiesJson,
  }));
  const result = posthogFetch(
    ctx,
    cfg,
    '/batch',
    buildBatchBody(cfg.projectApiKey, events)
  );

  return ctx.withTx(tx => {
    let delivered = 0;
    let failed = 0;
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
      tx.db.posthogOutbox.outboxId.update(settled.row);
      if (settled.terminal) updateDeliveryStats(tx, { pending: -1n });
      if (result.ok) delivered++;
      else failed++;
    }
    pruneDeliveryHistory(tx);
    return { attempted: rows.length, delivered, failed };
  });
}

export const enqueue_event = spacetimedb.reducer(
  {
    distinctId: t.string(),
    event: t.string(),
    propertiesJson: t.option(t.string()),
    idempotencyKey: t.option(t.string()),
  },
  (ctx, args) => {
    requireAdmin(ctx, ctx.sender);
    enqueueEvent(ctx, args);
  }
);

export const capture_now = spacetimedb.procedure(
  {
    distinctId: t.string(),
    event: t.string(),
    propertiesJson: t.option(t.string()),
  },
  t.string(),
  (ctx, args) => {
    ctx.withTx(tx => requireAdmin(tx, ctx.sender));
    return JSON.stringify(captureNow(ctx, args));
  }
);

export const flush_outbox = spacetimedb.procedure(
  { limit: t.u32() },
  t.string(),
  (ctx, args) => JSON.stringify(flushOutbox(ctx, { limit: args.limit }))
);

export const get_feature_flag = spacetimedb.procedure(
  {
    key: t.string(),
    distinctId: t.string(),
    personPropertiesJson: t.option(t.string()),
    groupsJson: t.option(t.string()),
  },
  t.string(),
  (ctx, args) => {
    ctx.withTx(tx => requireAdmin(tx, ctx.sender));
    if (!args.key.trim() || args.key.length > MAX_EVENT_NAME_LENGTH) {
      throwSenderError('posthog.invalid_flag_key');
    }
    if (
      !args.distinctId.trim() ||
      args.distinctId.length > MAX_DISTINCT_ID_LENGTH
    ) {
      throwSenderError('posthog.invalid_distinct_id');
    }
    if ((args.personPropertiesJson?.length ?? 0) > MAX_PROPERTIES_JSON_LENGTH) {
      throwSenderError('posthog.person_properties_too_large');
    }
    if ((args.groupsJson?.length ?? 0) > MAX_PROPERTIES_JSON_LENGTH) {
      throwSenderError('posthog.groups_too_large');
    }
    const personProperties = parseJsonObject(
      args.personPropertiesJson,
      'person_properties'
    );
    const groups = parseJsonObject(args.groupsJson, 'groups');
    const cfg = loadConfigOrThrowFromProcedure(ctx);
    const body: Record<string, unknown> = {
      api_key: cfg.projectApiKey,
      distinct_id: args.distinctId,
    };
    if (personProperties !== undefined)
      body.person_properties = personProperties;
    if (groups !== undefined) body.groups = groups;
    const result = posthogFetch(ctx, cfg, '/flags?v=2', body);
    let valueJson: string | undefined;
    if (isOkStatus(result.statusCode)) {
      try {
        const parsed = JSON.parse(result.responseBody) as Record<
          string,
          unknown
        >;
        const flags = parsed.featureFlags;
        if (flags && typeof flags === 'object' && args.key in flags) {
          valueJson = JSON.stringify(
            (flags as Record<string, unknown>)[args.key]
          );
        }
      } catch {
        valueJson = undefined;
      }
    }
    ctx.withTx(tx => {
      logDelivery(
        tx,
        DeliverySource.FeatureFlag,
        undefined,
        { distinctId: args.distinctId, event: `$feature_flag:${args.key}` },
        result
      );
      pruneDeliveryHistory(tx);
    });
    return JSON.stringify({
      ok: result.ok,
      statusCode: result.statusCode,
      responseBody: result.responseBody,
      valueJson,
    });
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

export { deliverySource };
