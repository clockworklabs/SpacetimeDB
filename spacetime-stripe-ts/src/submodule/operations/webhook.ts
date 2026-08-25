import { WebhookEventStatus, spacetimedb } from '../schema';
import {
  SyncResponse,
  type HandlerContext,
  type Request as StdbRequest,
} from 'spacetimedb/server';
import { verifyStripeSignature } from '@spacetimedb/crypto';
import { parseStripeEventMetadata } from '../webhook-metadata';
import { MAX_WEBHOOK_METADATA_LENGTH } from '../limits';
import { applyStripeEvent, updateWebhookStatus } from '../operations';
import {
  validateWebhookRequestBody,
  validateWebhookRequestHeaders,
} from '../webhook-request';

// POST $STDB_URI/v1/database/<db>/route/stripe/webhook
function jsonResponse(status: number, body: unknown): SyncResponse {
  return new SyncResponse(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

export function handle_stripe_webhook(
  ctx: HandlerContext<typeof spacetimedb.schemaType>,
  req: StdbRequest
): SyncResponse {
  const signatureHeader = req.headers.get('stripe-signature') ?? undefined;
  const headerRejection = validateWebhookRequestHeaders(
    req.method,
    req.headers.get('content-length'),
    signatureHeader
  );
  if (headerRejection) {
    return jsonResponse(headerRejection.status, {
      error: headerRejection.error,
    });
  }

  const payloadJson = req.text();
  const bodyRejection = validateWebhookRequestBody(payloadJson);
  if (bodyRejection) {
    return jsonResponse(bodyRejection.status, { error: bodyRejection.error });
  }

  // Webhook processing requires a configured signing secret.
  const webhookSecret = ctx.withTx(tx => {
    const cfg = tx.db.stripeConfig.singleton.find(true);
    return cfg?.webhookSigningSecret ?? undefined;
  });
  if (!webhookSecret) {
    return jsonResponse(503, {
      error: 'webhook signing secret not configured',
    });
  }
  const nowSeconds = Number(ctx.timestamp.microsSinceUnixEpoch / 1_000_000n);
  const sigOk = verifyStripeSignature({
    rawBody: payloadJson,
    signatureHeader: signatureHeader ?? '',
    secret: webhookSecret,
    nowSeconds,
  });
  if (!sigOk) {
    return jsonResponse(401, { error: 'signature mismatch' });
  }

  const metadata = parseStripeEventMetadata(payloadJson);
  if (!metadata)
    return jsonResponse(400, { error: 'missing or invalid event metadata' });
  const { eventId, eventType, livemode } = metadata;
  if (
    eventId.length === 0 ||
    eventId.length > MAX_WEBHOOK_METADATA_LENGTH ||
    eventType.length === 0 ||
    eventType.length > MAX_WEBHOOK_METADATA_LENGTH
  ) {
    return jsonResponse(400, { error: 'invalid event metadata' });
  }

  try {
    const outcome = ctx.withTx(tx => {
      const existing = tx.db.stripeWebhookEvent.eventId.find(eventId);
      if (existing) return { kind: 'duplicate', status: existing.status };

      tx.db.stripeWebhookEvent.insert({
        eventId,
        eventType,
        livemode,
        signatureHeader,
        payloadJson,
        status: WebhookEventStatus.Received,
        errorMessage: undefined,
        receivedAt: tx.timestamp,
        processedAt: undefined,
      });

      const result = applyStripeEvent(tx, payloadJson);
      updateWebhookStatus(tx, eventId, result.status, result.error);
      return { kind: 'applied', status: result.status, error: result.error };
    });

    return jsonResponse(200, { ok: true, eventId, ...outcome });
  } catch (error) {
    console.error(`stripe webhook processing failed for ${eventId}:`, error);
    return jsonResponse(500, { error: 'webhook processing failed' });
  }
}

export const stripe_webhook_handler = spacetimedb.httpHandler(
  handle_stripe_webhook
);
