import { spacetimedb, type WriteCtx } from '../schema.js';
import {
  SyncResponse,
  type HandlerContext,
  type Request as StdbRequest,
} from 'spacetimedb/server';
import {
  ingestWebhookPayload,
  type VerifiedStripeEvent,
} from '../operations.js';
import { validateWebhookRequestHeaders } from '../webhook-request.js';

// POST $STDB_URI/v1/database/<db>/route/stripe/webhook
function jsonResponse(status: number, body: unknown): SyncResponse {
  return new SyncResponse(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

export function handleStripeWebhook<T>(
  ctx: Pick<HandlerContext, 'timestamp'> & {
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

  try {
    const result = ctx.withTx(root =>
      ingestWebhookPayload(
        options.stripe(root),
        payloadJson,
        signatureHeader,
        options.onEvent && (event => options.onEvent!(root, event))
      )
    );
    if ('code' in result)
      return jsonResponse(result.httpStatus, { error: result.code });
    const ok = result.status.tag !== 'Failed';
    return jsonResponse(ok ? 200 : 400, {
      ok,
      eventId: result.event.id,
      status: result.status,
      error: result.error,
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
