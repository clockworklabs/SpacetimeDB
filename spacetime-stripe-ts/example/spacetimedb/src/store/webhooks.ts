// HTTP handlers for the store module.

import { Router, SyncResponse } from 'spacetimedb/server';
import { handle_stripe_webhook } from '@spacetimedb/stripe/submodule';
import { spacetimedb } from './schema';

export const stripe_webhook_handler = spacetimedb.httpHandler((ctx, req) =>
  handle_stripe_webhook(ctx.as.stripe, req)
);

export const health = spacetimedb.httpHandler((ctx, _req) => {
  const count = ctx.withTx(tx => tx.db.storeProduct.count());
  return new SyncResponse(
    JSON.stringify({
      ok: true,
      catalogRows: Number(count),
      at: ctx.timestamp.microsSinceUnixEpoch.toString(),
    }),
    { status: 200, headers: { 'content-type': 'application/json' } }
  );
});

export const echo = spacetimedb.httpHandler((_ctx, req) => {
  const body = req.text();
  return new SyncResponse(
    JSON.stringify({
      method: req.method,
      uri: req.uri,
      bodyLength: body.length,
      bodyPreview: body.slice(0, 200),
    }),
    { status: 200, headers: { 'content-type': 'application/json' } }
  );
});

export const router = spacetimedb.httpRouter(
  new Router()
    .get('/health', health)
    .post('/echo', echo)
    .post('/stripe/webhook', stripe_webhook_handler)
);
