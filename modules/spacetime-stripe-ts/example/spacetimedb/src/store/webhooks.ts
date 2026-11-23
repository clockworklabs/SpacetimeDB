import { Router, SyncResponse } from 'spacetimedb/server';
import { handleStripeWebhook } from '@spacetimedb/stripe/submodule';
import { spacetimedb } from './schema';

export const stripeWebhookHandler = spacetimedb.httpHandler((ctx, req) =>
  handleStripeWebhook(ctx, req, {
    stripe: tx => tx.as.stripe,
    onEvent: (tx, event) => {
      if (!event.type.startsWith('checkout.session.')) return;
      const id = event.data.object.id;
      if (typeof id !== 'string') return;
      const session =
        tx.as.stripe.db.stripeCheckoutSession.stripeCheckoutSessionId.find(id);
      if (
        !session ||
        session.mode === 'setup' ||
        session.status !== 'complete' ||
        !['paid', 'no_payment_required'].includes(session.paymentStatus)
      )
        return;
      const metadata: unknown = JSON.parse(session.metadataJson ?? '{}');
      if (
        !metadata ||
        typeof metadata !== 'object' ||
        !('store' in metadata) ||
        metadata.store !== ctx.identity.toHexString()
      )
        return;
      if (
        !('userId' in metadata) ||
        typeof metadata.userId !== 'string' ||
        !metadata.userId
      ) {
        throw new Error('store.purchase_owner_missing');
      }
      // A second Stripe event for this session must not fulfill the purchase twice.
      if (!tx.db.storePurchase.sessionId.find(id))
        tx.db.storePurchase.insert({
          sessionId: id,
          userId: metadata.userId,
          confirmedAt: tx.timestamp,
        });
    },
  })
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

export const router = spacetimedb.httpRouter(
  new Router()
    .get('/health', health)
    .post('/stripe/webhook', stripeWebhookHandler)
);
