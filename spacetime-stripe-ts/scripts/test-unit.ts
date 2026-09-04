import * as assert from 'node:assert/strict';
import { buildStripeHttpRequest } from '../src/submodule/http.ts';
import { parseStripeEventMetadata } from '../src/submodule/webhook-metadata.ts';
import {
  validateWebhookRequestBody,
  validateWebhookRequestHeaders,
} from '../src/submodule/webhook-request.ts';

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

console.log('stripe unit tests passed');
