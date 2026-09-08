import {
  EmailStatus,
  WebhookEventStatus,
  spacetimedb,
  t,
  vEmailEvent,
  type EmailEvent,
  type EmailStatusValue,
  type ModuleTimestamp,
  type ReducerModuleCtx,
  type WebhookEventStatusValue,
  type WriteCtx,
} from './schema';
import { upsertEmail } from './email_writes';
import { requireAdmin } from './auth';
import { verifySvixSignature } from '@spacetimedb/crypto';
import {
  SyncResponse,
  type Request,
  type HandlerContext,
} from 'spacetimedb/server';
import {
  assertExhaustive,
  parseWithSchema,
  safeJsonParse,
  summarizeIssues,
  throwSenderError,
} from './validation';
import { parseResendEventType } from './webhook-metadata';

type ResendTags = EmailEvent['data']['tags'];

function toAddressArray(value: string | string[]): string[] {
  return Array.isArray(value) ? value : [value];
}

function tagsToJson(tags: ResendTags): string | undefined {
  if (!tags) return undefined;
  try {
    return JSON.stringify(tags);
  } catch {
    return undefined;
  }
}

function extractTagFields(tags: ResendTags): {
  userId: string | undefined;
  orgId: string | undefined;
} {
  if (!tags) return { userId: undefined, orgId: undefined };
  if (Array.isArray(tags)) {
    let userId: string | undefined;
    let orgId: string | undefined;
    for (const tag of tags) {
      if (tag.name === 'userId') userId = tag.value;
      if (tag.name === 'orgId') orgId = tag.value;
    }
    return { userId, orgId };
  }
  return { userId: tags['userId'], orgId: tags['orgId'] };
}

function recordDeliveryEvent(
  ctx: WriteCtx,
  now: ModuleTimestamp,
  args: {
    eventId: string;
    resendId: string;
    eventType: string;
    createdAtIso: string;
    detailJson: string | undefined;
  }
) {
  if (ctx.db.resendDeliveryEvent.eventId.find(args.eventId)) return;
  ctx.db.resendDeliveryEvent.insert({
    eventId: args.eventId,
    resendId: args.resendId,
    eventType: args.eventType,
    createdAtIso: args.createdAtIso,
    detailJson: args.detailJson,
    insertedAt: now,
  });
}

function updateWebhookStatus(
  ctx: ReducerModuleCtx,
  eventId: string,
  status: WebhookEventStatusValue,
  errorMessage: string | undefined
) {
  const existing = ctx.db.resendWebhookEvent.eventId.find(eventId);
  if (!existing) return;

  const isTerminal = status.tag === 'Processed' || status.tag === 'Failed';
  const updated = {
    ...existing,
    status,
    errorMessage,
    processedAt: isTerminal ? ctx.timestamp : existing.processedAt,
  };

  if (ctx.db.resendWebhookEvent.eventId.update) {
    ctx.db.resendWebhookEvent.eventId.update(updated);
  } else {
    ctx.db.resendWebhookEvent.delete(existing);
    ctx.db.resendWebhookEvent.insert(updated);
  }
}

function makeEmailUpsertArgs(
  event: EmailEvent,
  now: ModuleTimestamp
): Parameters<typeof upsertEmail>[2] {
  const data = event.data;
  const fromAddress = Array.isArray(data.from) ? data.from[0]! : data.from;
  const toAddresses = toAddressArray(data.to);
  const tagFields = extractTagFields(data.tags);
  const tagsJson = tagsToJson(data.tags);
  const subject = data.subject;

  const base = {
    resendId: data.email_id,
    fromAddress,
    toAddressesJson: JSON.stringify(toAddresses),
    subject,
    // undefined preserves whatever send_email recorded.
    html: undefined,
    text: undefined,
    lastError: undefined,
    bouncedAt: undefined,
    bounceJson: undefined,
    failedAt: undefined,
    failureReason: undefined,
    complained: false,
    complainedAt: undefined,
    opened: false,
    openedAt: undefined,
    clicked: false,
    clickedAt: undefined,
    deliveredAt: undefined,
    sentAt: undefined,
    tagsJson,
    userId: tagFields.userId,
    orgId: tagFields.orgId,
    // status undefined = preserve existing or default to queued; branches override.
    status: undefined as EmailStatusValue | undefined,
  } satisfies Parameters<typeof upsertEmail>[2];

  switch (event.type) {
    case 'email.sent':
      return { ...base, status: EmailStatus.Sent, sentAt: now };
    case 'email.delivered':
      return { ...base, status: EmailStatus.Delivered, deliveredAt: now };
    case 'email.delivery_delayed':
      return { ...base, status: EmailStatus.DeliveryDelayed };
    case 'email.bounced':
      return {
        ...base,
        status: EmailStatus.Bounced,
        bouncedAt: now,
        bounceJson: JSON.stringify(event.data.bounce),
        lastError: event.data.bounce.message,
      };
    case 'email.failed':
      return {
        ...base,
        status: EmailStatus.Failed,
        failedAt: now,
        failureReason: event.data.failed.reason,
        lastError: event.data.failed.reason,
      };
    case 'email.complained':
      return { ...base, complained: true, complainedAt: now };
    case 'email.opened':
      return { ...base, opened: true, openedAt: now };
    case 'email.clicked':
      return { ...base, clicked: true, clickedAt: now };
    default:
      return assertExhaustive(event);
  }
}

function detailJsonForEvent(event: EmailEvent): string | undefined {
  switch (event.type) {
    case 'email.bounced':
      return JSON.stringify(event.data.bounce);
    case 'email.failed':
      return JSON.stringify(event.data.failed);
    case 'email.clicked':
      return JSON.stringify(event.data.click);
    case 'email.sent':
    case 'email.delivered':
    case 'email.delivery_delayed':
    case 'email.complained':
    case 'email.opened':
      return undefined;
    default:
      return assertExhaustive(event);
  }
}

function applyResendEvent(
  ctx: ReducerModuleCtx,
  eventId: string,
  payloadJson: string
): { status: WebhookEventStatusValue; error: string | undefined } {
  const parsed = safeJsonParse(payloadJson);
  if (parsed === undefined) {
    return { status: WebhookEventStatus.Failed, error: 'invalid JSON payload' };
  }

  const result = parseWithSchema(vEmailEvent, parsed);
  if (result.kind === 'error') {
    return {
      status: WebhookEventStatus.Failed,
      error: `payload validation failed: ${summarizeIssues(result.issues)}`,
    };
  }

  const event = result.data;
  const now = ctx.timestamp;
  upsertEmail(ctx, now, makeEmailUpsertArgs(event, now));
  recordDeliveryEvent(ctx, now, {
    eventId,
    resendId: event.data.email_id,
    eventType: event.type,
    createdAtIso: event.created_at,
    detailJson: detailJsonForEvent(event),
  });
  return { status: WebhookEventStatus.Processed, error: undefined };
}

export interface ResendWebhookIngestArgs {
  eventId: string;
  eventType: string;
  payloadJson: string;
  signatureHeader?: string | undefined;
  timestampHeader?: string | undefined;
}

const MAX_WEBHOOK_BODY_LENGTH = 1024 * 1024;
const MAX_WEBHOOK_HEADER_LENGTH = 8192;
const MAX_WEBHOOK_METADATA_LENGTH = 255;

// Verify the Svix signature in-module, then store and apply the event. The
// reducer and native HTTP route share this single ingest path and receive an
function ingestResendWebhook(
  ctx: WriteCtx,
  args: ResendWebhookIngestArgs
): { status: number; code: string } {
  if (
    args.eventId.length === 0 ||
    args.eventId.length > MAX_WEBHOOK_METADATA_LENGTH ||
    args.eventType.length === 0 ||
    args.eventType.length > MAX_WEBHOOK_METADATA_LENGTH
  ) {
    return { status: 400, code: 'resend.webhook_metadata_invalid' };
  }
  if (args.payloadJson.length > MAX_WEBHOOK_BODY_LENGTH) {
    return { status: 413, code: 'resend.webhook_payload_too_large' };
  }
  if (
    (args.signatureHeader?.length ?? 0) > MAX_WEBHOOK_HEADER_LENGTH ||
    (args.timestampHeader?.length ?? 0) > MAX_WEBHOOK_HEADER_LENGTH
  ) {
    return { status: 400, code: 'resend.webhook_header_too_large' };
  }

  const cfg = ctx.db.resendConfig.singleton.find(true);
  if (!cfg?.webhookSigningSecret) {
    return { status: 500, code: 'resend.webhook_secret_not_configured' };
  }
  const nowSeconds = Number(ctx.timestamp.microsSinceUnixEpoch / 1_000_000n);
  const sigOk = verifySvixSignature({
    svixId: args.eventId,
    svixTimestamp: args.timestampHeader ?? '',
    svixSignature: args.signatureHeader ?? '',
    rawBody: args.payloadJson,
    secret: cfg.webhookSigningSecret,
    nowSeconds,
  });
  if (!sigOk) return { status: 401, code: 'resend.webhook_signature_mismatch' };

  const signedEventType = parseResendEventType(args.payloadJson);
  if (!signedEventType || signedEventType !== args.eventType) {
    return { status: 400, code: 'resend.webhook_metadata_mismatch' };
  }

  // Idempotent: svix redelivers, so a known event id is a success no-op.
  if (ctx.db.resendWebhookEvent.eventId.find(args.eventId)) {
    return { status: 200, code: 'ok' };
  }

  ctx.db.resendWebhookEvent.insert({
    eventId: args.eventId,
    eventType: signedEventType,
    payloadJson: args.payloadJson,
    signatureHeader: args.signatureHeader,
    timestampHeader: args.timestampHeader,
    status: WebhookEventStatus.Received,
    errorMessage: undefined,
    receivedAt: ctx.timestamp,
    processedAt: undefined,
  });

  const outcome = applyResendEvent(
    ctx as ReducerModuleCtx,
    args.eventId,
    args.payloadJson
  );
  updateWebhookStatus(
    ctx as ReducerModuleCtx,
    args.eventId,
    outcome.status,
    outcome.error
  );
  return { status: 200, code: 'ok' };
}

export const ingest_resend_webhook = spacetimedb.reducer(
  {
    eventId: t.string(),
    eventType: t.string(),
    payloadJson: t.string(),
    signatureHeader: t.option(t.string()),
    timestampHeader: t.option(t.string()),
  },
  (ctx, args) => {
    const result = ingestResendWebhook(ctx, args);
    if (result.status !== 200) throwSenderError(result.code);
  }
);

function webhookJson(body: unknown, status: number): SyncResponse {
  return new SyncResponse(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

// Native SpacetimeDB HTTP route handler. Host modules register it on a router so
// Resend can post directly to the database.
export function makeResendWebhookHandler() {
  // The host passes the submodule-scoped context, so this handler remains schema-agnostic.
  return function resendWebhook(
    ctx: HandlerContext<typeof spacetimedb.schemaType>,
    req: Request
  ): SyncResponse {
    if (req.method.toUpperCase() !== 'POST') {
      return webhookJson({ error: 'method_not_allowed' }, 405);
    }

    const rawBody = req.text();
    const svixId = req.headers.get('svix-id') ?? '';
    const svixTimestamp = req.headers.get('svix-timestamp') ?? undefined;
    const svixSignature = req.headers.get('svix-signature') ?? undefined;
    if (!svixId) return webhookJson({ error: 'missing_svix_id' }, 400);

    let eventType: string | undefined;
    const parsed = safeJsonParse(rawBody);
    if (
      parsed &&
      typeof parsed === 'object' &&
      typeof (parsed as { type?: unknown }).type === 'string'
    ) {
      eventType = (parsed as { type: string }).type;
    }
    if (!eventType) return webhookJson({ error: 'missing_event_type' }, 400);

    const result = ctx.withTx(tx =>
      ingestResendWebhook(tx as WriteCtx, {
        eventId: svixId,
        eventType,
        payloadJson: rawBody,
        signatureHeader: svixSignature,
        timestampHeader: svixTimestamp,
      })
    );
    return webhookJson(
      { ok: result.status === 200, code: result.code },
      result.status
    );
  };
}

export const replay_webhook_event = spacetimedb.reducer(
  { eventId: t.string() },
  (ctx, { eventId }) => {
    // Administrators may run this operation over stored events.
    requireAdmin(ctx, ctx.sender);
    const event = ctx.db.resendWebhookEvent.eventId.find(eventId);
    if (!event) throwSenderError(`resend.webhook_event_not_found:${eventId}`);
    const outcome = applyResendEvent(ctx, eventId, event.payloadJson);
    updateWebhookStatus(ctx, eventId, outcome.status, outcome.error);
  }
);
