import * as assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHmac } from 'node:crypto';
import { mkdtempSync, rmSync } from 'node:fs';
import { build } from 'esbuild';
import { resolve, join } from 'node:path';
import Stripe from 'stripe';
import * as v from 'valibot';

const secretKey = process.env.STRIPE_SECRET_KEY;
if (!secretKey?.startsWith('sk_test_'))
  throw new Error('Set STRIPE_SECRET_KEY to a Stripe test key.');
const stripe = new Stripe(secretKey);
const server = process.argv[2] ?? 'http://127.0.0.1:3000';
const database = `stripe-billing-test-${Date.now()}`;
const signingSecret = 'whsec_billing_fixture';
const run = (...args: string[]) =>
  execFileSync('spacetime', args, { encoding: 'utf8' });
const token = run('login', 'show', '--token').match(
  /eyJ[\w-]+\.[\w-]+\.[\w-]+/
)?.[0];
if (!token) throw new Error('Log in with spacetime before running this test.');
const modulePath = mkdtempSync(resolve('.billing-test-'));
const optionString = v.union([
  v.tuple([v.literal(0), v.string()]),
  v.tuple([v.literal(1), v.tuple([])]),
]);
const sessionResult = v.tuple([v.string(), optionString, optionString]);
let published = false;
let customerId: string | undefined;
let productId: string | undefined;
const prices: string[] = [];
const subscriptions: string[] = [];
const sessions: string[] = [];
const failures: unknown[] = [];

async function call(
  name: string,
  args: unknown[],
  authorized = true
): Promise<unknown> {
  const response = await fetch(
    `${server}/v1/database/${database}/call/${name}`,
    {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        ...(authorized ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify(args),
    }
  );
  if (!response.ok)
    throw new Error(`${name}: ${response.status} ${await response.text()}`);
  const text = await response.text();
  return text ? JSON.parse(text) : undefined;
}

try {
  await build({
    entryPoints: ['scripts/fixtures/billing.ts'],
    bundle: true,
    format: 'esm',
    platform: 'browser',
    target: 'esnext',
    external: ['spacetime:sys*'],
    outfile: join(modulePath, 'bundle.js'),
  });
  run(
    'publish',
    '--server',
    server,
    '--js-path',
    join(modulePath, 'bundle.js'),
    '--yes',
    database
  );
  published = true;
  await call('stripe.set_stripe_config', [
    secretKey,
    null,
    { some: signingSecret },
  ]);
  const product = await stripe.products.create({
    name: `STDB billing test ${database}`,
  });
  productId = product.id;
  const price = await stripe.prices.create({
    product: product.id,
    unit_amount: 100,
    currency: 'usd',
  });
  prices.push(price.id);
  const recurring = await stripe.prices.create({
    product: product.id,
    unit_amount: 100,
    currency: 'usd',
    recurring: { interval: 'month' },
  });
  prices.push(recurring.id);
  let customerBeforeRetry: string | undefined;
  await assert.rejects(call('fail_customer_write', []), error => {
    if (!(error instanceof Error)) return false;
    customerBeforeRetry = error.message.match(
      /test\.local_write_failed:(cus_[A-Za-z0-9]+)/
    )?.[1];
    customerId = customerBeforeRetry;
    return customerBeforeRetry !== undefined;
  });
  for (const setup of [false, true]) {
    for (const embedded of [false, true]) {
      const args = [
        setup,
        embedded,
        price.id,
        'http://localhost:8817/',
        `${setup}-${embedded}`,
      ];
      const session = v.parse(sessionResult, await call('checkout', args));
      sessions.push(session[0]);
      assert.deepEqual(
        await call('checkout', args),
        session,
        'same intent must return the same session'
      );
      assert.equal(
        session[embedded ? 2 : 1][0],
        0,
        'expected URL or client secret'
      );
      const remote = await stripe.checkout.sessions.retrieve(session[0]);
      assert.equal(remote.mode, setup ? 'setup' : 'payment');
      assert.equal(remote.ui_mode, embedded ? 'embedded_page' : 'hosted_page');
      assert.equal(remote.billing_address_collection, 'required');
      assert.equal(typeof remote.customer, 'string');
      if (typeof remote.customer !== 'string')
        throw new Error('Expected customer ID');
      assert.equal(
        remote.customer,
        customerBeforeRetry,
        'retry must recover the customer after a failed local write'
      );
      customerId = remote.customer;
    }
  }
  await assert.rejects(
    call('checkout', [
      false,
      false,
      recurring.id,
      'http://localhost:8817/',
      'false-false',
    ]),
    /idempotent/i
  );
  assert.ok(customerId);
  const customer = await stripe.customers.retrieve(customerId);
  if (customer.deleted) throw new Error('Customer unexpectedly deleted');
  const sub = await stripe.subscriptions.create({
    customer: customerId,
    items: [{ price: recurring.id }],
    trial_period_days: 7,
    metadata: customer.metadata,
  });
  subscriptions.push(sub.id);
  const now = Math.floor(Date.now() / 1000);
  const payload = JSON.stringify({
    id: 'evt_billing_sub',
    type: 'customer.subscription.created',
    created: now,
    livemode: false,
    data: { object: sub },
  });
  const signature = `t=${now},v1=${createHmac('sha256', signingSecret).update(`${now}.${payload}`).digest('hex')}`;
  await call('stripe.ingest_stripe_webhook', [
    'evt_billing_sub',
    'customer.subscription.created',
    false,
    payload,
    { some: signature },
  ]);
  await assert.rejects(
    call(
      'change_subscription',
      [sub.id, null, { cancel: [] }, `${database}:unauthorized`],
      false
    ),
    /not_authorized/
  );
  await call('change_subscription', [
    sub.id,
    null,
    { cancel: [] },
    `${database}:cancel`,
  ]);
  assert.equal(
    (await stripe.subscriptions.retrieve(sub.id)).cancel_at_period_end,
    true
  );
  await call('change_subscription', [
    sub.id,
    null,
    { reactivate: [] },
    `${database}:reactivate`,
  ]);
  assert.equal(
    (await stripe.subscriptions.retrieve(sub.id)).cancel_at_period_end,
    false
  );
  await stripe.subscriptions.update(sub.id, {
    cancel_at: sub.items.data[0].current_period_end,
  });
  await call('change_subscription', [
    sub.id,
    null,
    { reactivate: [] },
    `${database}:reactivate-explicit-date`,
  ]);
  assert.equal(
    (await stripe.subscriptions.retrieve(sub.id)).cancel_at,
    null,
    'reactivation must clear a portal cancellation date'
  );
  await call('change_subscription', [
    sub.id,
    { some: sub.items.data[0].id },
    { quantity: 3 },
    `${database}:quantity`,
  ]);
  assert.equal(
    (await stripe.subscriptions.retrieve(sub.id)).items.data[0].quantity,
    3
  );
  console.log(
    'Live Stripe billing: hosted/embedded payment and setup, request retries, ownership, cancellation, reactivation, and quantity passed.'
  );
} catch (error) {
  failures.push(error);
} finally {
  const cleanup = [
    ...sessions.map(id => () => stripe.checkout.sessions.expire(id)),
    ...subscriptions.map(id => () => stripe.subscriptions.cancel(id)),
    () => (customerId ? stripe.customers.del(customerId) : undefined),
    ...prices.map(id => () => stripe.prices.update(id, { active: false })),
    () =>
      productId
        ? stripe.products.update(productId, { active: false })
        : undefined,
    () => {
      if (published) run('delete', '--server', server, '--yes', database);
    },
    () => {
      assert.ok(modulePath.startsWith(resolve('.billing-test-')));
      rmSync(modulePath, { recursive: true, force: true });
    },
  ];
  for (const action of cleanup) {
    try {
      await action();
    } catch (error) {
      failures.push(error);
    }
  }
}
if (failures.length)
  throw new AggregateError(failures, 'Stripe billing test or cleanup failed.');
