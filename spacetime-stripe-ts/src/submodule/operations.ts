import {
  SenderError,
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
import { buildStripeHttpRequest } from './http';
import {
  MAX_WEBHOOK_BODY_LENGTH,
  MAX_WEBHOOK_HEADER_LENGTH,
  MAX_WEBHOOK_METADATA_LENGTH,
} from './limits';
import {
  assertExhaustive,
  parseWithSchema,
  safeJsonParse,
  summarizeIssues,
} from './validation';

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

export function maybeJson(value: string): unknown | undefined {
  try {
    return JSON.parse(value);
  } catch {
    return undefined;
  }
}

export function stripeErrorSuffix(body: string): string {
  const parsed = maybeJson(body);
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
  const parsed = maybeJson(metadataJson);
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
  const parsed = maybeJson(metadataJson);
  if (!isRecord(parsed)) return [] as Array<[string, string]>;

  const out: Array<[string, string]> = [];
  for (const [k, raw] of Object.entries(parsed)) {
    if (raw === undefined || raw === null) continue;
    out.push([`${keyPrefix}[${k}]`, String(raw)]);
  }
  return out;
}

export function throwSenderError(message: string): never {
  throw new SenderError(message);
}

export function upsertCustomer(
  ctx: WriteCtx,
  now: ModuleTimestamp,
  args: {
    stripeCustomerId: string;
    appUserId: string | undefined;
    email: string | undefined;
    name: string | undefined;
    metadataJson: string | undefined;
    userId: string | undefined;
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
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
  };

  if (!existing) {
    ctx.db.stripeCustomer.insert(row);
    return;
  }
  if (ctx.db.stripeCustomer.stripeCustomerId.update) {
    ctx.db.stripeCustomer.stripeCustomerId.update(row);
  } else {
    ctx.db.stripeCustomer.delete(existing);
    ctx.db.stripeCustomer.insert(row);
  }
}

export function upsertSubscription(
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
    insertedAt: existing?.insertedAt ?? now,
    updatedAt: now,
  };

  if (!existing) {
    ctx.db.stripeSubscription.insert(row);
    return;
  }
  if (ctx.db.stripeSubscription.stripeSubscriptionId.update) {
    ctx.db.stripeSubscription.stripeSubscriptionId.update(row);
  } else {
    ctx.db.stripeSubscription.delete(existing);
    ctx.db.stripeSubscription.insert(row);
  }
}

export function upsertCheckoutSession(
  ctx: WriteCtx,
  now: ModuleTimestamp,
  args: {
    stripeCheckoutSessionId: string;
    stripeCustomerId: string | undefined;
    status: string;
    mode: string;
    metadataJson: string | undefined;
  }
) {
  const existing = ctx.db.stripeCheckoutSession.stripeCheckoutSessionId.find(
    args.stripeCheckoutSessionId
  );
  const row = {
    stripeCheckoutSessionId: args.stripeCheckoutSessionId,
    stripeCustomerId: args.stripeCustomerId ?? existing?.stripeCustomerId,
    status: args.status,
    mode: args.mode,
    metadataJson: args.metadataJson ?? existing?.metadataJson,
    insertedAt: existing?.insertedAt ?? now,
    updatedAt: now,
  };

  if (!existing) {
    ctx.db.stripeCheckoutSession.insert(row);
    return;
  }
  if (ctx.db.stripeCheckoutSession.stripeCheckoutSessionId.update) {
    ctx.db.stripeCheckoutSession.stripeCheckoutSessionId.update(row);
  } else {
    ctx.db.stripeCheckoutSession.delete(existing);
    ctx.db.stripeCheckoutSession.insert(row);
  }
}

export function upsertPayment(
  ctx: WriteCtx,
  now: ModuleTimestamp,
  args: {
    stripePaymentIntentId: string;
    stripeCustomerId: string | undefined;
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
  if (ctx.db.stripePayment.stripePaymentIntentId.update) {
    ctx.db.stripePayment.stripePaymentIntentId.update(row);
  } else {
    ctx.db.stripePayment.delete(existing);
    ctx.db.stripePayment.insert(row);
  }
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
    insertedAt: existing?.insertedAt ?? now,
    updatedAt: now,
  };

  if (!existing) {
    ctx.db.stripeInvoice.insert(row);
    return;
  }
  if (ctx.db.stripeInvoice.stripeInvoiceId.update) {
    ctx.db.stripeInvoice.stripeInvoiceId.update(row);
  } else {
    ctx.db.stripeInvoice.delete(existing);
    ctx.db.stripeInvoice.insert(row);
  }
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

  if (ctx.db.stripeWebhookEvent.eventId.update) {
    ctx.db.stripeWebhookEvent.eventId.update(updated);
  } else {
    ctx.db.stripeWebhookEvent.delete(existing);
    ctx.db.stripeWebhookEvent.insert(updated);
  }
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

export function toBigIntOrZero(n: number | undefined | null): bigint {
  return n === undefined || n === null ? 0n : BigInt(n);
}

export function toBigIntOrUndefined(
  n: number | undefined | null
): bigint | undefined {
  return n === undefined || n === null ? undefined : BigInt(n);
}

const HANDLED_EVENT_TYPES: ReadonlySet<string> = new Set([
  'customer.created',
  'customer.updated',
  'customer.subscription.created',
  'customer.subscription.updated',
  'customer.subscription.deleted',
  'checkout.session.completed',
  'invoice.created',
  'invoice.finalized',
  'invoice.paid',
  'invoice.payment_succeeded',
  'invoice.payment_failed',
  'payment_intent.succeeded',
]);

export function applyStripeEvent(
  ctx: ReducerModuleCtx,
  payloadJson: string
): { status: WebhookEventStatusValue; error: string | undefined } {
  const parsedJson = safeJsonParse(payloadJson);
  if (parsedJson === undefined) {
    return { status: WebhookEventStatus.Failed, error: 'invalid JSON payload' };
  }

  const result = parseWithSchema(vStripeEvent, parsedJson);
  if (result.kind === 'error') {
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

  return { status: dispatchEvent(ctx, result.data), error: undefined };
}

type ParsedInvoiceObject = Extract<
  ParsedStripeEvent,
  { type: 'invoice.paid' }
>['data']['object'];

function syncInvoiceEvent(
  ctx: ReducerModuleCtx,
  obj: ParsedInvoiceObject,
  status: string
): WebhookEventStatusValue {
  const existing = ctx.db.stripeInvoice.stripeInvoiceId.find(obj.id);
  const payloadCustomerId = extractExpandableIdOrNull(obj.customer);
  const customerId = existing?.stripeCustomerId ?? payloadCustomerId;
  if (customerId === null) return WebhookEventStatus.Failed;
  const payloadSubscriptionId =
    obj.subscription === undefined
      ? undefined
      : (extractExpandableIdOrNull(obj.subscription) ?? undefined);
  const subscriptionId =
    existing?.stripeSubscriptionId ?? payloadSubscriptionId;
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
  });
  return WebhookEventStatus.Processed;
}

function dispatchEvent(
  ctx: ReducerModuleCtx,
  event: ParsedStripeEvent
): WebhookEventStatusValue {
  switch (event.type) {
    case 'customer.created':
    case 'customer.updated': {
      const obj = event.data.object;
      const meta = metadataInfoFromRecord(obj.metadata);
      upsertCustomer(ctx, ctx.timestamp, {
        stripeCustomerId: obj.id,
        appUserId: undefined,
        email: obj.email ?? undefined,
        name: obj.name ?? undefined,
        metadataJson: meta.metadataJson,
        userId: meta.userId,
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
      upsertSubscription(ctx, ctx.timestamp, {
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
      });
      return WebhookEventStatus.Processed;
    }
    case 'checkout.session.completed': {
      const obj = event.data.object;
      const meta = metadataInfoFromRecord(obj.metadata);
      const customerId =
        obj.customer === undefined
          ? undefined
          : (extractExpandableIdOrNull(obj.customer) ?? undefined);
      upsertCheckoutSession(ctx, ctx.timestamp, {
        stripeCheckoutSessionId: obj.id,
        stripeCustomerId: customerId,
        status: 'complete',
        mode: obj.mode ?? 'payment',
        metadataJson: meta.metadataJson,
      });
      return WebhookEventStatus.Processed;
    }
    case 'invoice.created':
    case 'invoice.finalized': {
      const obj = event.data.object;
      return syncInvoiceEvent(ctx, obj, obj.status ?? 'open');
    }
    case 'invoice.paid':
    case 'invoice.payment_succeeded': {
      return syncInvoiceEvent(ctx, event.data.object, 'paid');
    }
    case 'invoice.payment_failed': {
      return syncInvoiceEvent(ctx, event.data.object, 'open');
    }
    case 'payment_intent.succeeded': {
      const obj = event.data.object;
      // Invoice events own invoice-attached payment state.
      const invoiceId =
        obj.invoice === undefined
          ? null
          : extractExpandableIdOrNull(obj.invoice);
      if (invoiceId !== null && invoiceId !== undefined)
        return WebhookEventStatus.Ignored;

      const customerId =
        obj.customer === undefined
          ? null
          : extractExpandableIdOrNull(obj.customer);
      const meta = metadataInfoFromRecord(obj.metadata);
      upsertPayment(ctx, ctx.timestamp, {
        stripePaymentIntentId: obj.id,
        stripeCustomerId: customerId ?? undefined,
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
      error instanceof Error ? error.message : 'stripe.request_invalid'
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
      `stripe.create_customer_failed:${result.status}${stripeErrorSuffix(result.body)}`
    );
  }

  const parsedBody = safeJsonParse(result.body);
  const idResult = parseWithSchema(vStripeIdResponse, parsedBody);
  if (idResult.kind === 'error') {
    throwSenderError(
      `stripe.create_customer_invalid_response:${summarizeIssues(idResult.issues)}`
    );
  }
  const customerId = idResult.data.id;

  const details = coerceMetadataFromJson(args.metadataJson);
  ctx.withTx(tx => {
    upsertCustomer(tx, ctx.timestamp, {
      stripeCustomerId: customerId,
      appUserId: undefined,
      email: args.email,
      name: args.name,
      metadataJson: details.metadataJson,
      userId: details.userId,
    });
  });
  return customerId;
}

export const upsert_customer = spacetimedb.reducer(
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
    upsertCustomer(ctx, ctx.timestamp, {
      stripeCustomerId: args.stripeCustomerId,
      appUserId: args.appUserId,
      email: args.email,
      name: args.name,
      metadataJson: args.metadataJson,
      userId: args.userId,
    });
  }
);

export const upsert_subscription = spacetimedb.reducer(
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
    upsertSubscription(ctx, ctx.timestamp, {
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
    });
  }
);

export const update_payment_customer = spacetimedb.reducer(
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

export const update_subscription_quantity_internal = spacetimedb.reducer(
  {
    stripeSubscriptionId: t.string(),
    quantity: t.i64(),
  },
  (ctx, { stripeSubscriptionId, quantity }) => {
    requireAdmin(ctx, ctx.sender);
    const existing =
      ctx.db.stripeSubscription.stripeSubscriptionId.find(stripeSubscriptionId);
    if (!existing) return;
    upsertSubscription(ctx, ctx.timestamp, {
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
    });
  }
);

export const ingest_stripe_webhook = spacetimedb.reducer(
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
      throwSenderError('stripe.webhook_metadata_invalid');
    }
    if (payloadJson.length > MAX_WEBHOOK_BODY_LENGTH) {
      throwSenderError('stripe.webhook_payload_too_large');
    }
    if ((signatureHeader?.length ?? 0) > MAX_WEBHOOK_HEADER_LENGTH) {
      throwSenderError('stripe.webhook_signature_too_large');
    }

    const cfg = ctx.db.stripeConfig.singleton.find(true);
    if (!cfg?.webhookSigningSecret) {
      throwSenderError('stripe.webhook_secret_not_configured');
    }
    const nowSeconds = Number(ctx.timestamp.microsSinceUnixEpoch / 1_000_000n);
    const sigOk = verifyStripeSignature({
      rawBody: payloadJson,
      signatureHeader: signatureHeader ?? '',
      secret: cfg.webhookSigningSecret,
      nowSeconds,
    });
    if (!sigOk) throwSenderError('stripe.webhook_signature_mismatch');

    const signedMetadata = parseStripeEventMetadata(payloadJson);
    if (!signedMetadata)
      throwSenderError('stripe.webhook_payload_missing_metadata');
    if (
      eventId !== signedMetadata.eventId ||
      eventType !== signedMetadata.eventType ||
      livemode !== signedMetadata.livemode
    ) {
      throwSenderError('stripe.webhook_metadata_mismatch');
    }

    const existing = ctx.db.stripeWebhookEvent.eventId.find(
      signedMetadata.eventId
    );
    if (existing) return;

    ctx.db.stripeWebhookEvent.insert({
      eventId: signedMetadata.eventId,
      eventType: signedMetadata.eventType,
      livemode: signedMetadata.livemode,
      signatureHeader,
      payloadJson,
      status: WebhookEventStatus.Received,
      errorMessage: undefined,
      receivedAt: ctx.timestamp,
      processedAt: undefined,
    });

    const outcome = applyStripeEvent(ctx, payloadJson);
    updateWebhookStatus(
      ctx,
      signedMetadata.eventId,
      outcome.status,
      outcome.error
    );
  }
);

export const replay_webhook_event = spacetimedb.reducer(
  { eventId: t.string() },
  (ctx, { eventId }) => {
    // Administrators may run this operation over stored events.
    requireAdmin(ctx, ctx.sender);
    const event = ctx.db.stripeWebhookEvent.eventId.find(eventId);
    if (!event) throwSenderError(`stripe.webhook_event_not_found:${eventId}`);
    const outcome = applyStripeEvent(ctx, event.payloadJson);
    updateWebhookStatus(ctx, eventId, outcome.status, outcome.error);
  }
);

// Validate a Stripe price ID by hitting GET /v1/prices/:id with the module's stored secret.
