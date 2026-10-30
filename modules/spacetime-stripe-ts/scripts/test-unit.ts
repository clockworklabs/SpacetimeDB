import * as assert from 'node:assert/strict';
import { retireCheckoutAttempt } from '../example/spacetimedb/src/store/checkout-recovery';
import { Timestamp } from 'spacetimedb';
import { latestSubscription } from '../src/submodule/subscription-order';
import {
  checkoutPaymentStatusRank,
  invoiceStatusRank,
  isStale,
  subscriptionStatusRank,
} from '../src/submodule/event-order';
import { buildStripeHttpRequest } from '../src/submodule/http';
import { checkoutForm, type UserCheckoutArgs } from '../src/submodule/checkout';
import { parseStripeEventMetadata } from '../src/submodule/webhook-metadata';
import {
  validateWebhookRequestBody,
  validateWebhookRequestHeaders,
} from '../src/submodule/webhook-request';

const customerEvent = {
  id: 'evt_1',
  type: 'customer.created',
  livemode: false,
  data: {
    object: {
      id: 'cus_1',
      email: 'customer@example.com',
      name: 'Customer',
      metadata: { userId: 'user-1' },
    },
  },
};

const checkout: UserCheckoutArgs = {
  userId: 'buyer',
  mode: 'subscription',
  items: [{ priceId: 'price_seat', quantity: 2n }],
  successUrl: 'https://store.test/return',
  cancelUrl: 'https://store.test/',
  metadata: { orgId: 'team', note: 'a&b' },
  params: {
    allow_promotion_codes: false,
    automatic_tax: { enabled: true },
    subscription_data: {
      trial_period_days: 7,
      metadata: { userId: 'wrong', extra: 'kept' },
    },
  },
};
const checkoutBody = new URLSearchParams(checkoutForm(checkout, 'cus_buyer'));
assert.equal(checkoutBody.get('customer'), 'cus_buyer');
assert.equal(checkoutBody.get('line_items[0][quantity]'), '2');
assert.equal(checkoutBody.get('allow_promotion_codes'), 'false');
assert.equal(checkoutBody.get('automatic_tax[enabled]'), 'true');
assert.equal(checkoutBody.get('subscription_data[trial_period_days]'), '7');
assert.equal(checkoutBody.get('subscription_data[metadata][userId]'), 'buyer');
assert.equal(checkoutBody.get('subscription_data[metadata][extra]'), 'kept');
assert.equal(checkoutBody.get('metadata[note]'), 'a&b');
assert.throws(
  () => checkoutForm({ ...checkout, items: [] }, 'cus_buyer'),
  /requires_items/
);
assert.throws(
  () =>
    checkoutForm(
      { ...checkout, items: [{ priceId: 'price_seat', quantity: 0n }] },
      'cus_buyer'
    ),
  /quantity/
);
// JavaScript callers can bypass the type boundary; protected fields still cannot be replaced.
assert.throws(
  () =>
    checkoutForm(
      { ...checkout, params: JSON.parse('{"customer":"cus_other"}') },
      'cus_buyer'
    ),
  /protected/
);
const setupBody = new URLSearchParams(
  checkoutForm(
    {
      userId: 'buyer',
      mode: 'setup',
      currency: 'usd',
      uiMode: 'embedded_page',
      returnUrl: 'https://store.test/return',
    },
    'cus_buyer'
  )
);
assert.equal(setupBody.get('mode'), 'setup');
assert.equal(setupBody.get('currency'), 'usd');
assert.equal(setupBody.get('ui_mode'), 'embedded_page');
assert.equal(setupBody.get('return_url'), 'https://store.test/return');
assert.equal(setupBody.has('success_url'), false);
assert.equal(
  [...setupBody.keys()].some(key => key.startsWith('line_items')),
  false
);
assert.equal(setupBody.get('setup_intent_data[metadata][userId]'), 'buyer');

assert.deepEqual(parseStripeEventMetadata(JSON.stringify(customerEvent)), {
  eventId: 'evt_1',
  eventType: 'customer.created',
  livemode: false,
});
assert.equal(parseStripeEventMetadata('{bad json'), undefined);
assert.equal(parseStripeEventMetadata('{"id":"evt_1"}'), undefined);

const request = buildStripeHttpRequest({
  method: 'post',
  path: '/v1/customers',
  secretKey: 'sk_test_secret',
  stripeVersion: '2025-01-01',
  formBody: 'email=user%40example.com',
  idempotencyKey: 'customer-user-1',
});
assert.equal(request.url, 'https://api.stripe.com/v1/customers');
assert.equal(request.method, 'POST');
assert.equal(request.headers.Authorization, 'Bearer sk_test_secret');
assert.throws(
  () =>
    buildStripeHttpRequest({
      method: 'GET',
      path: 'https://attacker.example/collect',
      secretKey: 'sk_test_secret',
      stripeVersion: undefined,
      formBody: undefined,
      idempotencyKey: undefined,
    }),
  /stripe\.request_path_invalid/
);
assert.throws(
  () =>
    buildStripeHttpRequest({
      method: 'GET',
      path: '/v1/customers\u007fblocked',
      secretKey: 'sk_test_secret',
      stripeVersion: undefined,
      formBody: undefined,
      idempotencyKey: undefined,
    }),
  /stripe\.request_path_invalid/
);
assert.throws(
  () =>
    buildStripeHttpRequest({
      method: 'TRACE',
      path: '/v1/customers',
      secretKey: 'sk_test_secret',
      stripeVersion: undefined,
      formBody: undefined,
      idempotencyKey: undefined,
    }),
  /stripe\.request_method_invalid/
);

assert.deepEqual(validateWebhookRequestHeaders('GET', null, undefined), {
  status: 405,
  error: 'method not allowed',
});
assert.equal(
  validateWebhookRequestHeaders('POST', String(1024 * 1024 + 1), undefined)
    ?.status,
  413
);
assert.equal(
  validateWebhookRequestHeaders('POST', null, 'x'.repeat(8193))?.status,
  431
);
assert.equal(validateWebhookRequestHeaders('POST', null, 'short'), undefined);
assert.equal(validateWebhookRequestBody('')?.status, 400);
assert.equal(
  validateWebhookRequestBody('x'.repeat(1024 * 1024 + 1))?.status,
  413
);
assert.equal(validateWebhookRequestBody('{}'), undefined);

const subscriptions = Array.from({ length: 5001 }, (_, index) => ({
  stripeSubscriptionId: `sub_${index}`,
  insertedAt: new Timestamp(BigInt(index)),
}));
assert.equal(latestSubscription(subscriptions.values()), subscriptions[5000]);
assert.equal(latestSubscription([]), undefined);
assert.equal(
  latestSubscription([
    { stripeSubscriptionId: 'sub_b', insertedAt: new Timestamp(1n) },
    { stripeSubscriptionId: 'sub_a', insertedAt: new Timestamp(1n) },
  ])?.stripeSubscriptionId,
  'sub_b'
);

const state = (createdUnix: bigint, rank: number) => ({ createdUnix, rank });
assert.equal(isStale(undefined, state(1n, 0)), false);
assert.equal(isStale(state(10n, 1), state(9n, 1)), true);
assert.equal(isStale(state(10n, 1), state(10n, 1)), false);
assert.equal(isStale(state(10n, 1), state(11n, 1)), false);
// A newer event cannot move a status backward.
assert.equal(
  isStale(
    state(10n, subscriptionStatusRank('canceled')),
    state(11n, subscriptionStatusRank('active'))
  ),
  true
);
assert.equal(
  isStale(
    state(10n, subscriptionStatusRank('active')),
    state(10n, subscriptionStatusRank('incomplete'))
  ),
  true
);
assert.equal(
  isStale(
    state(10n, subscriptionStatusRank('active')),
    state(11n, subscriptionStatusRank('past_due'))
  ),
  false
);
assert.equal(
  isStale(
    state(10n, invoiceStatusRank('paid')),
    state(10n, invoiceStatusRank('open'))
  ),
  true
);
assert.equal(
  isStale(
    state(10n, invoiceStatusRank('uncollectible')),
    state(11n, invoiceStatusRank('paid'))
  ),
  false
);
assert.equal(
  isStale(
    state(10n, checkoutPaymentStatusRank('paid')),
    state(10n, checkoutPaymentStatusRank('unpaid'))
  ),
  true
);
assert.equal(
  isStale(
    state(10n, checkoutPaymentStatusRank('failed')),
    state(9n, checkoutPaymentStatusRank('unpaid'))
  ),
  true
);
assert.equal(
  isStale(
    state(10n, checkoutPaymentStatusRank('paid')),
    state(11n, checkoutPaymentStatusRank('failed'))
  ),
  true
);

console.log('stripe unit tests passed');

const recoveryArgs = {
  customers: ['cus_buyer'],
  store: 'store',
  attemptId: 'attempt',
};
for (const status of ['complete', 'expired']) {
  let pages = 0;
  const recover = () =>
    retireCheckoutAttempt((method, path) => {
      assert.equal(method, 'GET');
      const query = new URL(`https://api.stripe.com${path}`).searchParams;
      assert.equal(
        query.get('starting_after'),
        pages ? `cs_page_${pages}` : null
      );
      pages++;
      return {
        has_more: pages < 11,
        data: [
          {
            id: `cs_page_${pages}`,
            status,
            metadata:
              pages === 11 ? { store: 'store', storeAttempt: 'attempt' } : {},
          },
        ],
      };
    }, recoveryArgs);
  if (status === 'complete') assert.throws(recover, /checkout is complete/);
  else recover();
  assert.equal(pages, 11, 'Recovery must reach a match beyond page ten');
}
for (const customer of ['cus_buyer', 'cus_other']) {
  const requests: string[] = [];
  const recover = () =>
    retireCheckoutAttempt(
      (method, path) => {
        requests.push(`${method} ${path}`);
        return {
          id: 'cs_saved',
          customer,
          status: method === 'GET' ? 'open' : 'expired',
          metadata: { store: 'store', storeAttempt: 'attempt' },
        };
      },
      { ...recoveryArgs, sessionId: 'cs_saved' }
    );
  if (customer === 'cus_other') {
    assert.throws(recover, /owner_mismatch/);
    assert.equal(requests.length, 1);
  } else {
    recover();
    assert.deepEqual(requests, [
      'GET /v1/checkout/sessions/cs_saved',
      'POST /v1/checkout/sessions/cs_saved/expire',
    ]);
  }
}
console.log(
  'Checkout recovery: pagination and direct session ownership passed.'
);
