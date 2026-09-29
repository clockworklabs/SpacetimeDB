import * as v from 'valibot';
import { Timestamp } from 'spacetimedb';
import {
  Range,
  SenderError,
  stripeWebhookPruneTickTable,
  t,
  WebhookEventStatus,
  type WebhookEventStatusValue,
  spacetimedb,
  vStripeEvent,
  vStripeIdResponse,
  extractExpandableId,
  extractExpandableIdOrNull,
  type ParsedStripeEvent,
  type ProcedureModuleCtx,
  type ReducerModuleCtx,
  type TransactionModuleCtx,
  type WriteCtx,
  type JsonRecord,
  type ModuleTimestamp,
} from './schema';
import { verifyStripeSignature } from '@spacetimedb/crypto';
import { adminVerdict, denyIfNotAdmin, requireAdmin } from './auth';
import { parseStripeEventMetadata } from './webhook-metadata';
import {
  checkoutPaymentStatusRank,
  invoiceStatusRank,
  isStale,
  subscriptionStatusRank,
} from './event-order';
import { buildStripeHttpRequest } from './http';
import {
  MAX_WEBHOOK_BODY_LENGTH,
  MAX_WEBHOOK_HEADER_LENGTH,
  MAX_WEBHOOK_METADATA_LENGTH,
  WEBHOOK_EVENT_RETENTION_MICROS,
  WEBHOOK_PRUNE_BATCH,
} from './limits';
import {
  assertExhaustive,
  safeJsonParse,
  summarizeIssues,
  throwSenderError,
} from './validation';
import { errors } from './errors';

export function requireProcedureAdmin(ctx: ProcedureModuleCtx): void {
  const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
  denyIfNotAdmin(verdict);
}

export function withAdminTx<T>(
  ctx: ProcedureModuleCtx,
  read: (tx: TransactionModuleCtx) => T
): T {
  requireProcedureAdmin(ctx);
  return ctx.withTx(read);
}

const MAX_QUERY_ROWS = 1000;

export function takeRows<T>(rows: Iterable<T>, limit = MAX_QUERY_ROWS): T[] {
  const out: T[] = [];
  for (const row of rows) {
    if (out.length >= limit) break;
    out.push(row);
  }
  return out;
}

export function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null;
}

export function maybeString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

export function maybeBoolean(value: unknown): boolean | undefined {
  return typeof value === 'boolean' ? value : undefined;
}

export function maybeNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value)
    ? value
    : undefined;
}

export function maybeInt(value: unknown): number | undefined {
  const parsed = maybeNumber(value);
  return parsed === undefined || !Number.isInteger(parsed) ? undefined : parsed;
}

export function maybeBigIntFromUnknown(value: unknown): bigint | undefined {
  const parsed = maybeInt(value);
  return parsed === undefined ? undefined : BigInt(parsed);
}

export function maybeId(value: unknown): string | undefined {
  if (typeof value === 'string') return value;
  if (!isRecord(value)) return undefined;
  return maybeString(value.id);
}

export function stripeErrorSuffix(body: string): string {
  const parsed = safeJsonParse(body);
  if (isRecord(parsed)) {
    const errorPayload = isRecord(parsed.error) ? parsed.error : undefined;
    if (errorPayload) {
      const type = maybeString(errorPayload.type);
      const code = maybeString(errorPayload.code);
      const message = maybeString(errorPayload.message);
      const requestLogUrl = maybeString(errorPayload.request_log_url);
      const parts: string[] = [];
      if (type) parts.push(`type=${type}`);
      if (code) parts.push(`code=${code}`);
      if (message)
        parts.push(`msg=${message.replace(/\s+/g, ' ').slice(0, 240)}`);
      if (requestLogUrl) parts.push(`log=${requestLogUrl}`);
      if (parts.length > 0) return `:${parts.join('|')}`;
    }
  }

  const compact = body.replace(/\s+/g, ' ').trim();
  if (!compact) return '';
  return `:body=${compact.slice(0, 240)}`;
}

export function toJsonString(value: unknown): string | undefined {
  if (value === undefined) return undefined;
  try {
    return JSON.stringify(value);
  } catch {
    return undefined;
  }
}

export function metadataInfo(metadata: unknown): {
  metadataJson: string | undefined;
  orgId: string | undefined;
  userId: string | undefined;
} {
  if (!isRecord(metadata)) {
    return { metadataJson: undefined, orgId: undefined, userId: undefined };
  }
  return {
    metadataJson: toJsonString(metadata),
    orgId: maybeString(metadata.orgId),
    userId: maybeString(metadata.userId),
  };
}

export function coerceMetadataFromJson(metadataJson: string | undefined) {
  if (!metadataJson) {
    return { metadataJson: undefined, orgId: undefined, userId: undefined };
  }
  const parsed = safeJsonParse(metadataJson);
  const details = metadataInfo(parsed);
  return {
    metadataJson: details.metadataJson ?? metadataJson,
    orgId: details.orgId,
    userId: details.userId,
  };
}

export function deriveCancelAtPeriodEnd(
  cancelAtUnix: bigint | undefined,
  currentPeriodEndUnix: bigint
): boolean {
  if (cancelAtUnix === undefined || currentPeriodEndUnix <= 0n) return false;
  const tolerance = 5n * 60n;
  const delta =
    cancelAtUnix > currentPeriodEndUnix
      ? cancelAtUnix - currentPeriodEndUnix
      : currentPeriodEndUnix - cancelAtUnix;
  return delta <= tolerance;
}

export function formPairsToBody(
  pairs: Array<[string, string | undefined]>
): string {
  const encoded: string[] = [];
  for (const [key, value] of pairs) {
    if (value === undefined) continue;
    encoded.push(`${encodeURIComponent(key)}=${encodeURIComponent(value)}`);
  }
  return encoded.join('&');
}

export function metadataJsonToFormPairs(
  keyPrefix: string,
  metadataJson: string | undefined
) {
  if (!metadataJson) return [] as Array<[string, string]>;
  const parsed = safeJsonParse(metadataJson);
  if (!isRecord(parsed)) return [] as Array<[string, string]>;

  const out: Array<[string, string]> = [];
  for (const [k, raw] of Object.entries(parsed)) {
    if (raw === undefined || raw === null) continue;
    out.push([`${keyPrefix}[${k}]`, String(raw)]);
  }
  return out;
}

export function upsertCustomerRow(
  ctx: WriteCtx,
  now: ModuleTimestamp,
  args: {
    stripeCustomerId: string;
    appUserId: string | undefined;
    email: string | undefined;
    name: string | undefined;
    metadataJson: string | undefined;
    userId: string | undefined;
    eventCreatedUnix: bigint;
  }
) {
  const existing = ctx.db.stripeCustomer.stripeCustomerId.find(
    args.stripeCustomerId
  );
  const row = {
    stripeCustomerId: args.stripeCustomerId,
    appUserId: args.appUserId ?? existing?.appUserId,
    email: args.email ?? existing?.email,
    name: args.name ?? existing?.name,
    metadataJson: args.metadataJson ?? existing?.metadataJson,
    userId: args.userId ?? existing?.userId,
    eventCreatedUnix: args.eventCreatedUnix,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
  };

  if (!existing) {
    ctx.db.stripeCustomer.insert(row);
    return;
  }
  ctx.db.stripeCustomer.stripeCustomerId.update(row);
}

export function upsertSubscriptionRow(
  ctx: WriteCtx,
  now: ModuleTimestamp,
  args: {
    stripeSubscriptionId: string;
    stripeCustomerId: string;
    status: string;
    currentPeriodEndUnix: bigint;
    cancelAtPeriodEnd: boolean;
    cancelAtUnix: bigint | undefined;
    quantity: bigint | undefined;
    priceId: string | undefined;
    metadataJson: string | undefined;
    orgId: string | undefined;
    userId: string | undefined;
    eventCreatedUnix: bigint;
  }
) {
  const existing = ctx.db.stripeSubscription.stripeSubscriptionId.find(
    args.stripeSubscriptionId
  );
  const row = {
    stripeSubscriptionId: args.stripeSubscriptionId,
    stripeCustomerId: args.stripeCustomerId,
    status: args.status,
    currentPeriodEndUnix: args.currentPeriodEndUnix,
    cancelAtPeriodEnd: args.cancelAtPeriodEnd,
    cancelAtUnix: args.cancelAtUnix,
    quantity: args.quantity,
    priceId: args.priceId ?? existing?.priceId,
    metadataJson: args.metadataJson ?? existing?.metadataJson,
    orgId: args.orgId ?? existing?.orgId,
    userId: args.userId ?? existing?.userId,
    eventCreatedUnix: args.eventCreatedUnix,
    insertedAt: existing?.insertedAt ?? now,
    updatedAt: now,
  };

  if (!existing) {
    ctx.db.stripeSubscription.insert(row);
    return;
  }
  ctx.db.stripeSubscription.stripeSubscriptionId.update(row);
}

export function upsertCheckoutSession(
  ctx: WriteCtx,
  now: ModuleTimestamp,
  args: {
    stripeCheckoutSessionId: string;
    stripeCustomerId: string | undefined;
    status: string;
    paymentStatus: string;
    mode: string;
    metadataJson: string | undefined;
    eventCreatedUnix: bigint;
  }
) {
  const existing = ctx.db.stripeCheckoutSession.stripeCheckoutSessionId.find(
    args.stripeCheckoutSessionId
  );
  const row = {
    stripeCheckoutSessionId: args.stripeCheckoutSessionId,
    stripeCustomerId: args.stripeCustomerId ?? existing?.stripeCustomerId,
    status: args.status,
    paymentStatus: args.paymentStatus,
    mode: args.mode,
    metadataJson: args.metadataJson ?? existing?.metadataJson,
    eventCreatedUnix: args.eventCreatedUnix,
    insertedAt: existing?.insertedAt ?? now,
    updatedAt: now,
  };

  if (!existing) {
    ctx.db.stripeCheckoutSession.insert(row);
    return;
  }
  ctx.db.stripeCheckoutSession.stripeCheckoutSessionId.update(row);
}

export function upsertPayment(
  ctx: WriteCtx,
  now: ModuleTimestamp,
  args: {
    stripePaymentIntentId: string;
    stripeCustomerId: string | undefined;
    stripeInvoiceId: string | undefined;
    amount: bigint;
    currency: string;
    status: string;
    createdUnix: bigint;
    metadataJson: string | undefined;
    orgId: string | undefined;
    userId: string | undefined;
  }
) {
  const existing = ctx.db.stripePayment.stripePaymentIntentId.find(
    args.stripePaymentIntentId
  );
  const row = {
    stripePaymentIntentId: args.stripePaymentIntentId,
    stripeCustomerId: args.stripeCustomerId ?? existing?.stripeCustomerId,
    stripeInvoiceId: args.stripeInvoiceId ?? existing?.stripeInvoiceId,
    amount: args.amount,
    currency: args.currency,
    status: args.status,
    createdUnix: args.createdUnix,
    metadataJson: args.metadataJson ?? existing?.metadataJson,
    orgId: args.orgId ?? existing?.orgId,
    userId: args.userId ?? existing?.userId,
    insertedAt: existing?.insertedAt ?? now,
    updatedAt: now,
  };

  if (!existing) {
    ctx.db.stripePayment.insert(row);
    return;
  }
  ctx.db.stripePayment.stripePaymentIntentId.update(row);
}

export function upsertInvoice(
  ctx: WriteCtx,
  now: ModuleTimestamp,
  args: {
    stripeInvoiceId: string;
    stripeCustomerId: string;
    stripeSubscriptionId: string | undefined;
    status: string;
    amountDue: bigint;
    amountPaid: bigint;
    createdUnix: bigint;
    orgId: string | undefined;
    userId: string | undefined;
    eventCreatedUnix: bigint;
  }
) {
  const existing = ctx.db.stripeInvoice.stripeInvoiceId.find(
    args.stripeInvoiceId
  );
  const row = {
    stripeInvoiceId: args.stripeInvoiceId,
    stripeCustomerId: args.stripeCustomerId,
    stripeSubscriptionId:
      args.stripeSubscriptionId ?? existing?.stripeSubscriptionId,
    status: args.status,
    amountDue: args.amountDue,
    amountPaid: args.amountPaid,
    createdUnix: args.createdUnix,
    orgId: args.orgId ?? existing?.orgId,
    userId: args.userId ?? existing?.userId,
    eventCreatedUnix: args.eventCreatedUnix,
    insertedAt: existing?.insertedAt ?? now,
    updatedAt: now,
  };

  if (!existing) {
    ctx.db.stripeInvoice.insert(row);
    return;
  }
  ctx.db.stripeInvoice.stripeInvoiceId.update(row);
}

export function updateWebhookStatus(
  ctx: ReducerModuleCtx,
  eventId: string,
  status: WebhookEventStatusValue,
  errorMessage: string | undefined
) {
  const existing = ctx.db.stripeWebhookEvent.eventId.find(eventId);
  if (!existing) return;

  const isTerminal =
    status.tag === 'Processed' ||
    status.tag === 'Ignored' ||
    status.tag === 'Failed';
  const updated = {
    ...existing,
    status,
    errorMessage,
    processedAt: isTerminal ? ctx.timestamp : existing.processedAt,
  };

  ctx.db.stripeWebhookEvent.eventId.update(updated);
}

export function metadataInfoFromRecord(
  metadata: Record<string, string> | null | undefined
): {
  metadataJson: string | undefined;
  orgId: string | undefined;
  userId: string | undefined;
} {
  if (!metadata)
    return { metadataJson: undefined, orgId: undefined, userId: undefined };
  return {
    metadataJson: toJsonString(metadata),
    orgId: metadata.orgId,
    userId: metadata.userId,
  };
}

export function unixSeconds(timestamp: ModuleTimestamp): bigint {
  return timestamp.microsSinceUnixEpoch / 1_000_000n;
}

export function toBigIntOrZero(n: number | undefined | null): bigint {
  return n === undefined || n === null ? 0n : BigInt(n);
}

export function toBigIntOrUndefined(
  n: number | undefined | null
): bigint | undefined {
  return n === undefined || n === null ? undefined : BigInt(n);
}

const HANDLED_EVENT_TYPES: ReadonlySet<string> = new Set(
  vStripeEvent.options.map(option => option.entries.type.literal)
);

export function applyStripeEvent(
  ctx: ReducerModuleCtx,
  payloadJson: string
): { status: WebhookEventStatusValue; error: string | undefined } {
  const parsedJson = safeJsonParse(payloadJson);
  if (parsedJson === undefined) {
    return { status: WebhookEventStatus.Failed, error: 'invalid JSON payload' };
  }

  const result = v.safeParse(vStripeEvent, parsedJson);
  if (!result.success) {
    // Distinguish unhandled type (ignore) from handled-but-malformed (fail).
    const eventTypeRaw =
      typeof parsedJson === 'object' && parsedJson !== null
        ? (parsedJson as Record<string, unknown>).type
        : undefined;
    const isHandledType =
      typeof eventTypeRaw === 'string' && HANDLED_EVENT_TYPES.has(eventTypeRaw);
    if (!isHandledType) {
      return { status: WebhookEventStatus.Ignored, error: undefined };
    }
    return {
      status: WebhookEventStatus.Failed,
      error: `payload validation failed: ${summarizeIssues(result.issues)}`,
    };
  }

  return { status: dispatchEvent(ctx, result.output), error: undefined };
}

type ParsedInvoiceObject = Extract<
  ParsedStripeEvent,
  { type: 'invoice.paid' }
>['data']['object'];

function invoiceSubscriptionId(obj: ParsedInvoiceObject): string | undefined {
  const legacy =
    obj.subscription === undefined
      ? null
      : extractExpandableIdOrNull(obj.subscription);
  const parent = obj.parent?.subscription_details?.subscription;
  return (
    legacy ?? (parent === undefined ? undefined : extractExpandableId(parent))
  );
}

function syncInvoiceEvent(
  ctx: ReducerModuleCtx,
  obj: ParsedInvoiceObject,
  status: string,
  eventCreatedUnix: bigint
): WebhookEventStatusValue {
  const existing = ctx.db.stripeInvoice.stripeInvoiceId.find(obj.id);
  if (
    existing &&
    isStale(
      {
        createdUnix: existing.eventCreatedUnix,
        rank: invoiceStatusRank(existing.status),
      },
      { createdUnix: eventCreatedUnix, rank: invoiceStatusRank(status) }
    )
  )
    return WebhookEventStatus.Ignored;
  const payloadCustomerId = extractExpandableIdOrNull(obj.customer);
  const customerId = existing?.stripeCustomerId ?? payloadCustomerId;
  if (customerId === null) return WebhookEventStatus.Failed;
  const subscriptionId =
    existing?.stripeSubscriptionId ?? invoiceSubscriptionId(obj);
  const subscription = subscriptionId
    ? ctx.db.stripeSubscription.stripeSubscriptionId.find(subscriptionId)
    : undefined;
  upsertInvoice(ctx, ctx.timestamp, {
    stripeInvoiceId: obj.id,
    stripeCustomerId: customerId,
    stripeSubscriptionId: subscriptionId,
    status,
    amountDue: toBigIntOrUndefined(obj.amount_due) ?? existing?.amountDue ?? 0n,
    amountPaid:
      toBigIntOrUndefined(obj.amount_paid) ?? existing?.amountPaid ?? 0n,
    createdUnix:
      toBigIntOrUndefined(obj.created) ?? existing?.createdUnix ?? 0n,
    orgId: existing?.orgId ?? subscription?.orgId,
    userId: existing?.userId ?? subscription?.userId,
    eventCreatedUnix,
  });
  return WebhookEventStatus.Processed;
}

function dispatchEvent(
  ctx: ReducerModuleCtx,
  event: ParsedStripeEvent
): WebhookEventStatusValue {
  const eventCreatedUnix = BigInt(event.created);
  switch (event.type) {
    case 'customer.created':
    case 'customer.updated': {
      const obj = event.data.object;
      const existing = ctx.db.stripeCustomer.stripeCustomerId.find(obj.id);
      if (existing && eventCreatedUnix < existing.eventCreatedUnix)
        return WebhookEventStatus.Ignored;
      const meta = metadataInfoFromRecord(obj.metadata);
      upsertCustomerRow(ctx, ctx.timestamp, {
        stripeCustomerId: obj.id,
        appUserId: undefined,
        email: obj.email ?? undefined,
        name: obj.name ?? undefined,
        metadataJson: meta.metadataJson,
        userId: meta.userId,
        eventCreatedUnix,
      });
      return WebhookEventStatus.Processed;
    }
    case 'customer.subscription.created':
    case 'customer.subscription.updated':
    case 'customer.subscription.deleted': {
      const obj = event.data.object;
      const status =
        event.type === 'customer.subscription.deleted'
          ? 'canceled'
          : obj.status;
      const existing = ctx.db.stripeSubscription.stripeSubscriptionId.find(
        obj.id
      );
      if (
        existing &&
        isStale(
          {
            createdUnix: existing.eventCreatedUnix,
            rank: subscriptionStatusRank(existing.status),
          },
          {
            createdUnix: eventCreatedUnix,
            rank: subscriptionStatusRank(status),
          }
        )
      )
        return WebhookEventStatus.Ignored;
      const customerId = extractExpandableId(obj.customer);
      const firstItem = obj.items?.data[0];
      const currentPeriodEnd =
        toBigIntOrUndefined(firstItem?.current_period_end) ??
        toBigIntOrUndefined(obj.current_period_end) ??
        0n;
      const cancelAtUnix = toBigIntOrUndefined(obj.cancel_at);
      const cancelAtPeriodEnd =
        obj.cancel_at_period_end ??
        deriveCancelAtPeriodEnd(cancelAtUnix, currentPeriodEnd);
      const meta = metadataInfoFromRecord(obj.metadata);
      upsertSubscriptionRow(ctx, ctx.timestamp, {
        stripeSubscriptionId: obj.id,
        stripeCustomerId: customerId,
        status,
        currentPeriodEndUnix: currentPeriodEnd,
        cancelAtPeriodEnd,
        cancelAtUnix,
        quantity: toBigIntOrUndefined(firstItem?.quantity),
        priceId: firstItem?.price?.id,
        metadataJson: meta.metadataJson,
        orgId: meta.orgId,
        userId: meta.userId,
        eventCreatedUnix,
      });
      return WebhookEventStatus.Processed;
    }
    case 'checkout.session.completed':
    case 'checkout.session.async_payment_succeeded':
    case 'checkout.session.async_payment_failed': {
      const obj = event.data.object;
      const existing =
        ctx.db.stripeCheckoutSession.stripeCheckoutSessionId.find(obj.id);
      if (
        existing &&
        isStale(
          {
            createdUnix: existing.eventCreatedUnix,
            rank: checkoutPaymentStatusRank(existing.paymentStatus),
          },
          {
            createdUnix: eventCreatedUnix,
            rank: checkoutPaymentStatusRank(obj.payment_status),
          }
        )
      )
        return WebhookEventStatus.Ignored;
      const meta = metadataInfoFromRecord(obj.metadata);
      const customerId =
        obj.customer === undefined
          ? undefined
          : (extractExpandableIdOrNull(obj.customer) ?? undefined);
      upsertCheckoutSession(ctx, ctx.timestamp, {
        stripeCheckoutSessionId: obj.id,
        stripeCustomerId: customerId,
        status: obj.status ?? 'complete',
        paymentStatus: obj.payment_status,
        mode: obj.mode ?? 'payment',
        metadataJson: meta.metadataJson,
        eventCreatedUnix,
      });
      return WebhookEventStatus.Processed;
    }
    case 'invoice.created': {
      const obj = event.data.object;
      return syncInvoiceEvent(
        ctx,
        obj,
        obj.status ?? 'draft',
        eventCreatedUnix
      );
    }
    case 'invoice.finalized':
    case 'invoice.payment_failed': {
      const obj = event.data.object;
      return syncInvoiceEvent(ctx, obj, obj.status ?? 'open', eventCreatedUnix);
    }
    case 'invoice.paid':
    case 'invoice.payment_succeeded': {
      const obj = event.data.object;
      return syncInvoiceEvent(ctx, obj, obj.status ?? 'paid', eventCreatedUnix);
    }
    case 'invoice_payment.paid': {
      // API versions from 2025-03-31.basil link invoice payments here instead
      // of `payment_intent.invoice`.
      const obj = event.data.object;
      if (obj.payment.payment_intent === undefined)
        return WebhookEventStatus.Ignored;
      const paymentIntentId = extractExpandableId(obj.payment.payment_intent);
      const invoiceId = extractExpandableId(obj.invoice);
      const existing =
        ctx.db.stripePayment.stripePaymentIntentId.find(paymentIntentId);
      const invoice = ctx.db.stripeInvoice.stripeInvoiceId.find(invoiceId);
      upsertPayment(ctx, ctx.timestamp, {
        stripePaymentIntentId: paymentIntentId,
        stripeCustomerId: invoice?.stripeCustomerId,
        stripeInvoiceId: invoiceId,
        amount: existing?.amount ?? toBigIntOrZero(obj.amount_paid),
        currency: existing?.currency ?? obj.currency,
        status: existing?.status ?? 'succeeded',
        createdUnix: existing?.createdUnix ?? BigInt(obj.created),
        metadataJson: undefined,
        orgId: invoice?.orgId,
        userId: invoice?.userId,
      });
      return WebhookEventStatus.Processed;
    }
    case 'payment_intent.succeeded': {
      const obj = event.data.object;
      const invoiceId =
        obj.invoice === undefined
          ? undefined
          : (extractExpandableIdOrNull(obj.invoice) ?? undefined);
      const customerId =
        obj.customer === undefined
          ? null
          : extractExpandableIdOrNull(obj.customer);
      const meta = metadataInfoFromRecord(obj.metadata);
      upsertPayment(ctx, ctx.timestamp, {
        stripePaymentIntentId: obj.id,
        stripeCustomerId: customerId ?? undefined,
        stripeInvoiceId: invoiceId,
        amount: toBigIntOrZero(obj.amount),
        currency: obj.currency ?? 'unknown',
        status: obj.status ?? 'succeeded',
        createdUnix: toBigIntOrZero(obj.created),
        metadataJson: meta.metadataJson,
        orgId: meta.orgId,
        userId: meta.userId,
      });
      return WebhookEventStatus.Processed;
    }
    default:
      return assertExhaustive(event);
  }
}

export function callStripe(
  ctx: ProcedureModuleCtx,
  args: {
    method: string;
    path: string;
    secretKey: string;
    stripeVersion: string | undefined;
    formBody: string | undefined;
    idempotencyKey: string | undefined;
  }
) {
  let request;
  try {
    request = buildStripeHttpRequest(args);
  } catch (error) {
    throw new SenderError(
      error instanceof Error ? error.message : errors.requestInvalid
    );
  }
  const response = ctx.http.fetch(request.url, {
    method: request.method,
    headers: request.headers,
    body: request.body,
  });
  return {
    status: response.status,
    body: response.text(),
  };
}

export function createCustomerInStripeAndSync(
  ctx: ProcedureModuleCtx,
  args: {
    secretKey: string;
    stripeVersion: string | undefined;
    email: string | undefined;
    name: string | undefined;
    metadataJson: string | undefined;
    idempotencyKey: string | undefined;
  }
): string {
  const formPairs: Array<[string, string | undefined]> = [
    ['email', args.email],
    ['name', args.name],
  ];
  for (const [k, v] of metadataJsonToFormPairs('metadata', args.metadataJson)) {
    formPairs.push([k, v]);
  }

  const result = callStripe(ctx, {
    method: 'POST',
    path: '/v1/customers',
    secretKey: args.secretKey,
    stripeVersion: args.stripeVersion,
    idempotencyKey: args.idempotencyKey
      ? `create_customer_${args.idempotencyKey}`
      : undefined,
    formBody: formPairsToBody(formPairs),
  });
  if (result.status < 200 || result.status >= 300) {
    throwSenderError(
      `${errors.createCustomerFailed}:${result.status}${stripeErrorSuffix(result.body)}`
    );
  }

  const parsedBody = safeJsonParse(result.body);
  const idResult = v.safeParse(vStripeIdResponse, parsedBody);
  if (!idResult.success) {
    throwSenderError(
      `${errors.createCustomerInvalidResponse}:${summarizeIssues(idResult.issues)}`
    );
  }
  const customerId = idResult.output.id;

  const details = coerceMetadataFromJson(args.metadataJson);
  ctx.withTx(tx => {
    upsertCustomerRow(tx, ctx.timestamp, {
      stripeCustomerId: customerId,
      appUserId: undefined,
      email: args.email,
      name: args.name,
      metadataJson: details.metadataJson,
      userId: details.userId,
      eventCreatedUnix: unixSeconds(ctx.timestamp),
    });
  });
  return customerId;
}

export const upsertCustomer = spacetimedb.reducer(
  {
    stripeCustomerId: t.string(),
    appUserId: t.option(t.string()),
    email: t.option(t.string()),
    name: t.option(t.string()),
    metadataJson: t.option(t.string()),
    userId: t.option(t.string()),
  },
  (ctx, args) => {
    requireAdmin(ctx, ctx.sender);
    upsertCustomerRow(ctx, ctx.timestamp, {
      stripeCustomerId: args.stripeCustomerId,
      appUserId: args.appUserId,
      email: args.email,
      name: args.name,
      metadataJson: args.metadataJson,
      userId: args.userId,
      eventCreatedUnix: unixSeconds(ctx.timestamp),
    });
  }
);

export const upsertSubscription = spacetimedb.reducer(
  {
    stripeSubscriptionId: t.string(),
    stripeCustomerId: t.string(),
    status: t.string(),
    currentPeriodEndUnix: t.i64(),
    cancelAtPeriodEnd: t.bool(),
    cancelAtUnix: t.option(t.i64()),
    quantity: t.option(t.i64()),
    priceId: t.option(t.string()),
    metadataJson: t.option(t.string()),
    orgId: t.option(t.string()),
    userId: t.option(t.string()),
  },
  (ctx, args) => {
    requireAdmin(ctx, ctx.sender);
    upsertSubscriptionRow(ctx, ctx.timestamp, {
      stripeSubscriptionId: args.stripeSubscriptionId,
      stripeCustomerId: args.stripeCustomerId,
      status: args.status,
      currentPeriodEndUnix: args.currentPeriodEndUnix,
      cancelAtPeriodEnd: args.cancelAtPeriodEnd,
      cancelAtUnix: args.cancelAtUnix,
      quantity: args.quantity,
      priceId: args.priceId,
      metadataJson: args.metadataJson,
      orgId: args.orgId,
      userId: args.userId,
      eventCreatedUnix: unixSeconds(ctx.timestamp),
    });
  }
);

export const updatePaymentCustomer = spacetimedb.reducer(
  {
    stripePaymentIntentId: t.string(),
    stripeCustomerId: t.string(),
  },
  (ctx, { stripePaymentIntentId, stripeCustomerId }) => {
    requireAdmin(ctx, ctx.sender);
    const existing = ctx.db.stripePayment.stripePaymentIntentId.find(
      stripePaymentIntentId
    );
    if (!existing || existing.stripeCustomerId) return;
    upsertPayment(ctx, ctx.timestamp, {
      stripePaymentIntentId: existing.stripePaymentIntentId,
      stripeCustomerId,
      stripeInvoiceId: existing.stripeInvoiceId,
      amount: existing.amount,
      currency: existing.currency,
      status: existing.status,
      createdUnix: existing.createdUnix,
      metadataJson: existing.metadataJson,
      orgId: existing.orgId,
      userId: existing.userId,
    });
  }
);

export const updateSubscriptionQuantityInternal = spacetimedb.reducer(
  {
    stripeSubscriptionId: t.string(),
    quantity: t.i64(),
  },
  (ctx, { stripeSubscriptionId, quantity }) => {
    requireAdmin(ctx, ctx.sender);
    const existing =
      ctx.db.stripeSubscription.stripeSubscriptionId.find(stripeSubscriptionId);
    if (!existing) return;
    upsertSubscriptionRow(ctx, ctx.timestamp, {
      stripeSubscriptionId: existing.stripeSubscriptionId,
      stripeCustomerId: existing.stripeCustomerId,
      status: existing.status,
      currentPeriodEndUnix: existing.currentPeriodEndUnix,
      cancelAtPeriodEnd: existing.cancelAtPeriodEnd,
      cancelAtUnix: existing.cancelAtUnix,
      quantity,
      priceId: existing.priceId,
      metadataJson: existing.metadataJson,
      orgId: existing.orgId,
      userId: existing.userId,
      eventCreatedUnix: existing.eventCreatedUnix,
    });
  }
);

export const ingestStripeWebhook = spacetimedb.reducer(
  {
    eventId: t.string(),
    eventType: t.string(),
    livemode: t.bool(),
    payloadJson: t.string(),
    signatureHeader: t.option(t.string()),
  },
  (ctx, { eventId, eventType, livemode, payloadJson, signatureHeader }) => {
    if (
      eventId.length === 0 ||
      eventId.length > MAX_WEBHOOK_METADATA_LENGTH ||
      eventType.length === 0 ||
      eventType.length > MAX_WEBHOOK_METADATA_LENGTH
    ) {
      throwSenderError(errors.webhookMetadataInvalid);
    }
    if (payloadJson.length > MAX_WEBHOOK_BODY_LENGTH) {
      throwSenderError(errors.webhookPayloadTooLarge);
    }
    if ((signatureHeader?.length ?? 0) > MAX_WEBHOOK_HEADER_LENGTH) {
      throwSenderError(errors.webhookSignatureTooLarge);
    }

    const cfg = ctx.db.stripeConfig.singleton.find(true);
    if (!cfg?.webhookSigningSecret) {
      throwSenderError(errors.webhookSecretNotConfigured);
    }
    const nowSeconds = Number(ctx.timestamp.microsSinceUnixEpoch / 1_000_000n);
    const sigOk = verifyStripeSignature({
      rawBody: payloadJson,
      signatureHeader: signatureHeader ?? '',
      secret: cfg.webhookSigningSecret,
      nowSeconds,
    });
    if (!sigOk) throwSenderError(errors.webhookSignatureMismatch);

    const signedMetadata = parseStripeEventMetadata(payloadJson);
    if (!signedMetadata) throwSenderError(errors.webhookPayloadMissingMetadata);
    if (
      eventId !== signedMetadata.eventId ||
      eventType !== signedMetadata.eventType ||
      livemode !== signedMetadata.livemode
    ) {
      throwSenderError(errors.webhookMetadataMismatch);
    }

    const existing = ctx.db.stripeWebhookEvent.eventId.find(
      signedMetadata.eventId
    );
    if (
      existing &&
      (existing.status.tag === 'Processed' || existing.status.tag === 'Ignored')
    )
      return;

    if (!existing)
      ctx.db.stripeWebhookEvent.insert({
        eventId: signedMetadata.eventId,
        eventType: signedMetadata.eventType,
        livemode: signedMetadata.livemode,
        payloadJson,
        status: WebhookEventStatus.Received,
        errorMessage: undefined,
        receivedAt: ctx.timestamp,
        processedAt: undefined,
      });

    const outcome = applyStripeEvent(ctx, existing?.payloadJson ?? payloadJson);
    if (outcome.status.tag === 'Failed')
      throwSenderError(errors.webhookPayloadInvalid);
    updateWebhookStatus(
      ctx,
      signedMetadata.eventId,
      outcome.status,
      outcome.error
    );
  }
);

export const replayWebhookEvent = spacetimedb.reducer(
  { eventId: t.string() },
  (ctx, { eventId }) => {
    // Administrators may run this operation over stored events.
    requireAdmin(ctx, ctx.sender);
    const event = ctx.db.stripeWebhookEvent.eventId.find(eventId);
    if (!event) throwSenderError(errors.webhookEventNotFound);
    const outcome = applyStripeEvent(ctx, event.payloadJson);
    updateWebhookStatus(ctx, eventId, outcome.status, outcome.error);
  }
);

export const pruneWebhookEvents = spacetimedb.reducer(
  { onSchedule: stripeWebhookPruneTickTable },
  { arg: stripeWebhookPruneTickTable.rowType },
  ctx => {
    const cutoff = new Timestamp(
      ctx.timestamp.microsSinceUnixEpoch - WEBHOOK_EVENT_RETENTION_MICROS
    );
    const expired = takeRows(
      ctx.db.stripeWebhookEvent.byReceivedAt.filter(
        new Range(undefined, { tag: 'excluded', value: cutoff })
      ),
      WEBHOOK_PRUNE_BATCH
    );
    for (const row of expired) ctx.db.stripeWebhookEvent.delete(row);
  }
);
