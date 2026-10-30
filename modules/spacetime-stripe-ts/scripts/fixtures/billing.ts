import { schema, t } from 'spacetimedb/server';
import * as stripe from '../../src/submodule';

// Deployed only by integration tests. All calls use the test caller's identity.
const db = schema({ stripe });
export default db;
export const init = db.init(ctx => stripe.install(ctx.as.stripe));

// Fail the local customer write after Stripe accepts the request.
export const failCustomerWrite = db.procedure({}, t.unit(), ctx => {
  const userId = ctx.sender.toHexString();
  const withTx: typeof ctx.as.stripe.withTx = body =>
    ctx.as.stripe.withTx(tx => {
      const result = body(tx);
      const customer = [...tx.db.stripeCustomer.byUserId.filter(userId)][0];
      if (customer)
        throw new Error(`test.local_write_failed:${customer.stripeCustomerId}`);
      return result;
    });
  const failing = new Proxy(ctx.as.stripe, {
    get: (target, key) =>
      key === 'withTx' ? withTx : Reflect.get(target, key, target),
  });
  stripe.getOrCreateUserCustomer(failing, { userId });
  return {};
});
const checkoutResult = t.object('FixtureCheckout', {
  sessionId: t.string(),
  url: t.option(t.string()),
  clientSecret: t.option(t.string()),
});

export const checkout = db.procedure(
  {
    setup: t.bool(),
    embedded: t.bool(),
    priceId: t.string(),
    returnUrl: t.string(),
    requestId: t.string(),
  },
  checkoutResult,
  (ctx, args) => {
    const mode = args.setup
      ? { mode: 'setup' as const, currency: 'usd' }
      : {
          mode: 'payment' as const,
          items: [{ priceId: args.priceId, quantity: 1n }],
        };
    const ui = args.embedded
      ? { uiMode: 'embedded_page' as const, returnUrl: args.returnUrl }
      : { successUrl: args.returnUrl, cancelUrl: args.returnUrl };
    const result = stripe.createUserCheckoutSession(ctx.as.stripe, {
      ...mode,
      ...ui,
      userId: ctx.sender.toHexString(),
      idempotencyKey: args.requestId,
      params: {
        payment_method_types: ['card'],
        billing_address_collection: 'required',
      },
    });
    return { ...result, clientSecret: result.clientSecret };
  }
);

export const changeSubscription = db.procedure(
  {
    subscriptionId: t.string(),
    itemId: t.option(t.string()),
    operation: t.enum('SubscriptionOperation', {
      cancel: t.unit(),
      reactivate: t.unit(),
      quantity: t.u32(),
    }),
    requestId: t.string(),
  },
  t.unit(),
  (ctx, args) => {
    const common = {
      userId: ctx.sender.toHexString(),
      stripeSubscriptionId: args.subscriptionId,
      idempotencyKey: args.requestId,
    };
    switch (args.operation.tag) {
      case 'cancel':
        stripe.cancelUserSubscription(ctx.as.stripe, common);
        break;
      case 'reactivate':
        stripe.reactivateUserSubscription(ctx.as.stripe, common);
        break;
      case 'quantity':
        stripe.updateUserSubscriptionQuantity(ctx.as.stripe, {
          ...common,
          subscriptionItemId: args.itemId,
          quantity: BigInt(args.operation.value),
        });
        break;
    }
    return {};
  }
);
