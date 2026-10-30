import * as assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { Timestamp } from 'spacetimedb';
import { ingestStripeWebhook } from '../src/submodule/operations';
import {
  createUserCheckoutSession,
  createUserCustomerPortalSession,
  cancelUserSubscription,
  reactivateUserSubscription,
  updateUserSubscriptionQuantity,
} from '../src/submodule/operations/billing';
import type { ProcedureModuleCtx } from '../src/submodule/schema';

// Signed and handled, but fails payload validation.
const webhookSecret = 'whsec_unit';
const nowSeconds = 1_800_000_000;
const invalidEventJson = JSON.stringify({
  id: 'evt_invalid',
  type: 'customer.created',
  livemode: false,
  created: nowSeconds,
  data: { object: {} },
});
const invalidEventSignature = `t=${nowSeconds},v1=${createHmac('sha256', webhookSecret).update(`${nowSeconds}.${invalidEventJson}`).digest('hex')}`;

// The mock context implements only the tables the reducer reads and writes.
type EventRow = { eventId: string; status: { tag: string } };
const webhookEvents = new Map<string, EventRow>();
const tx = {
  timestamp: new Timestamp(BigInt(nowSeconds) * 1_000_000n),
  db: {
    stripeConfig: {
      singleton: { find: () => ({ webhookSigningSecret: webhookSecret }) },
    },
    stripeWebhookEvent: {
      eventId: {
        find: (id: string) => webhookEvents.get(id),
        update: (row: EventRow) => webhookEvents.set(row.eventId, row),
      },
      insert: (row: EventRow) => webhookEvents.set(row.eventId, row),
    },
  },
};
const ingest = (signatureHeader: string) =>
  ingestStripeWebhook(tx as never, {
    eventId: 'evt_invalid',
    eventType: 'customer.created',
    livemode: false,
    payloadJson: invalidEventJson,
    signatureHeader,
  });

assert.throws(
  () => ingest(`t=${nowSeconds},v1=${'0'.repeat(64)}`),
  /stripe\.webhook_signature_mismatch/
);
assert.equal(webhookEvents.size, 0);
// The reducer returns so the Failed row commits.
ingest(invalidEventSignature);
assert.equal(webhookEvents.get('evt_invalid')?.status.tag, 'Failed');

process.stdout.write('stripe webhook tests passed\n');

const customer = {
  stripeCustomerId: 'cus_buyer',
  userId: 'buyer',
  deleted: false,
};
const subscription = {
  stripeSubscriptionId: 'sub_buyer',
  stripeCustomerId: customer.stripeCustomerId,
  userId: 'buyer',
  status: 'active',
  eventCreatedUnix: 0n,
};
const remoteItems = [
  { id: 'si_first', quantity: 1, price: { id: 'price_first' } },
];
const calls: Array<{
  url: string;
  body?: string;
  headers: Record<string, string>;
}> = [];
const billingTx = {
  timestamp: tx.timestamp,
  db: {
    stripeConfig: {
      singleton: {
        find: () => ({ secretKey: 'sk_test', stripeVersion: undefined }),
      },
    },
    stripeCustomer: {
      byUserId: { filter: (id: string) => (id === 'buyer' ? [customer] : []) },
      stripeCustomerId: { find: () => customer },
    },
    stripeSubscription: {
      stripeSubscriptionId: {
        find: (id: string) =>
          id === subscription.stripeSubscriptionId ? subscription : undefined,
        update: () => {},
      },
      byUserId: { filter: () => [] },
    },
    stripePayment: { byUserId: { filter: () => [] } },
  },
};
// Only the HTTP and table operations used by billing are supplied here.
const billing = {
  timestamp: tx.timestamp,
  withTx: (fn: (ctx: typeof billingTx) => unknown) => fn(billingTx),
  http: {
    fetch: (
      url: string,
      init: { body?: string; headers: Record<string, string> }
    ) => {
      calls.push({ url, ...init });
      const body = url.endsWith('/checkout/sessions')
        ? {
            id: 'cs_buyer',
            url: 'https://checkout.stripe.com/test',
            client_secret: 'cs_secret',
          }
        : url.endsWith('/billing_portal/sessions')
          ? { url: 'https://billing.stripe.com/test' }
          : {
              id: 'sub_buyer',
              customer: 'cus_buyer',
              status: 'active',
              items: { data: remoteItems },
            };
      return { status: 200, text: () => JSON.stringify(body) };
    },
  },
} as unknown as ProcedureModuleCtx;

const session = createUserCheckoutSession(billing, {
  userId: 'buyer',
  mode: 'setup',
  currency: 'usd',
  uiMode: 'embedded_page',
  returnUrl: 'https://store.test/return',
  idempotencyKey: 'attempt-1',
});
assert.equal(session.clientSecret, 'cs_secret');
assert.ok(calls.at(-1)?.headers['Idempotency-Key']);
assert.equal(
  new URLSearchParams(calls.at(-1)?.body).get(
    'setup_intent_data[metadata][userId]'
  ),
  'buyer'
);
assert.equal(
  createUserCustomerPortalSession(billing, {
    userId: 'buyer',
    returnUrl: 'https://store.test/',
  }).url,
  'https://billing.stripe.com/test'
);
const beforeDenied = calls.length;
assert.throws(
  () =>
    cancelUserSubscription(billing, {
      userId: 'other',
      stripeSubscriptionId: 'sub_buyer',
    }),
  /not_authorized/
);
assert.throws(
  () =>
    createUserCustomerPortalSession(billing, {
      userId: 'other',
      returnUrl: 'https://store.test/',
    }),
  /customer_not_found/
);
assert.equal(calls.length, beforeDenied);
cancelUserSubscription(billing, {
  userId: 'buyer',
  stripeSubscriptionId: 'sub_buyer',
});
assert.equal(
  new URLSearchParams(calls.at(-1)?.body).get('cancel_at_period_end'),
  'true'
);
reactivateUserSubscription(billing, {
  userId: 'buyer',
  stripeSubscriptionId: 'sub_buyer',
});
assert.equal(
  new URLSearchParams(calls.at(-1)?.body).get('cancel_at_period_end'),
  'false'
);
remoteItems.push({
  id: 'si_second',
  quantity: 1,
  price: { id: 'price_second' },
});
assert.throws(
  () =>
    updateUserSubscriptionQuantity(billing, {
      userId: 'buyer',
      stripeSubscriptionId: 'sub_buyer',
      quantity: 3n,
    }),
  /item_required/
);
assert.throws(
  () =>
    updateUserSubscriptionQuantity(billing, {
      userId: 'buyer',
      stripeSubscriptionId: 'sub_buyer',
      subscriptionItemId: 'si_other',
      quantity: 3n,
    }),
  /item_not_found/
);
assert.throws(
  () =>
    updateUserSubscriptionQuantity(billing, {
      userId: 'buyer',
      stripeSubscriptionId: 'sub_buyer',
      subscriptionItemId: 'si_second',
      quantity: 3n,
    }),
  /item_not_stored/
);
assert.equal(
  calls.some(call => call.url.endsWith('/subscription_items/si_second')),
  false
);
updateUserSubscriptionQuantity(billing, {
  userId: 'buyer',
  stripeSubscriptionId: 'sub_buyer',
  subscriptionItemId: 'si_first',
  quantity: 3n,
});
const change = calls.find(call =>
  call.url.endsWith('/subscription_items/si_first')
);
assert.equal(new URLSearchParams(change?.body).get('quantity'), '3');
process.stdout.write('stripe billing tests passed\n');
