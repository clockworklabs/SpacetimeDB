import { spacetimedb, type WriteCtx } from '../schema.js';
import {
  SyncResponse,
  type HandlerContext,
  type Request as StdbRequest,
} from 'spacetimedb/server';
import { loadConfigOrThrow } from '../config.js';
import {
  applyRefreshedSubscription,
  callStripe,
  ingestWebhookPayload,
  isRecord,
  type VerifiedStripeEvent,
} from '../operations.js';
import { safeJsonParse } from '../validation.js';
import { validateWebhookRequestHeaders } from '../webhook-request.js';

// POST $STDB_URI/v1/database/<db>/route/stripe/webhook
function jsonResponse(status: number, body: unknown): SyncResponse {
  return new SyncResponse(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

export function handleStripeWebhook<T>(
  ctx: Pick<HandlerContext, 'timestamp' | 'http'> & {
    withTx<R>(body: (tx: T) => R): R;
  },
  req: StdbRequest,
  options: {
    stripe: (tx: T) => WriteCtx;
    onEvent?: (tx: T, event: VerifiedStripeEvent) => void;
  }
): SyncResponse {
  const signatureHeader = req.headers.get('stripe-signature') ?? undefined;
  const rejection = validateWebhookRequestHeaders(
    req.method,
    req.headers.get('content-length'),
    signatureHeader
  );
  if (rejection) {
    return jsonResponse(rejection.status, { error: rejection.error });
  }
  const payloadJson = req.text();
  const onEvent = (root: T) =>
    options.onEvent &&
    ((event: VerifiedStripeEvent) => options.onEvent!(root, event));

  try {
    const result = ctx.withTx(root =>
      ingestWebhookPayload(
        options.stripe(root),
        payloadJson,
        signatureHeader,
        onEvent(root),
        true
      )
    );
    if ('code' in result)
      return jsonResponse(result.httpStatus, { error: result.code });
    const { event } = result;
    let outcome: Pick<typeof result, 'status' | 'error'> = result;
    if (outcome.status.tag === 'Received') {
      const config = ctx.withTx(root =>
        loadConfigOrThrow(options.stripe(root))
      );
      const response = callStripe(ctx, {
        method: 'GET',
        path: `/v1/subscriptions/${encodeURIComponent(String(event.data.object.id))}`,
        secretKey: config.secretKey,
        stripeVersion: config.stripeVersion,
        formBody: undefined,
        idempotencyKey: undefined,
      });
      const subscription = safeJsonParse(response.body);
      if (
        response.status < 200 ||
        response.status >= 300 ||
        !isRecord(subscription)
      )
        throw new Error(`subscription lookup failed: ${response.status}`);
      outcome = ctx.withTx(root =>
        applyRefreshedSubscription(
          options.stripe(root),
          event,
          subscription,
          onEvent(root)
        )
      );
    }
    const ok = outcome.status.tag !== 'Failed';
    return jsonResponse(ok ? 200 : 400, {
      ok,
      eventId: event.id,
      status: outcome.status,
      error: outcome.error,
    });
  } catch (error) {
    console.error('stripe webhook processing failed:', error);
    return jsonResponse(500, { error: 'webhook processing failed' });
  }
}

export const stripeWebhookHandler: ReturnType<typeof spacetimedb.httpHandler> =
  spacetimedb.httpHandler((ctx, req) =>
    handleStripeWebhook(ctx, req, { stripe: tx => tx })
  );
