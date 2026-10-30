import * as assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHmac } from 'node:crypto';
import * as v from 'valibot';
import { DbConnection } from '../example/src/module_bindings/app';

const server = process.argv[2] ?? 'http://127.0.0.1:3000';
const database = `stripe-example-test-${Date.now()}`;
let published = false;
const connections: DbConnection[] = [];
const secret = 'whsec_example_test';
const run = (...args: string[]) =>
  execFileSync('spacetime', args, { encoding: 'utf8' });

function connect(): Promise<DbConnection> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error('connection timeout')),
      10000
    );
    const connection = DbConnection.builder()
      .withUri(server)
      .withDatabaseName(database)
      .withCompression('none')
      .onConnect(conn => {
        clearTimeout(timer);
        resolve(conn);
      })
      .onConnectError((_ctx, error) => {
        clearTimeout(timer);
        reject(error);
      })
      .build();
    connections.push(connection);
  });
}

async function deliver(
  id: string,
  type: string,
  object: object,
  valid = true
): Promise<number> {
  const now = Math.floor(Date.now() / 1000);
  const body = JSON.stringify({
    id,
    type,
    created: now,
    livemode: false,
    data: { object },
  });
  const digest = createHmac('sha256', secret)
    .update(`${now}.${body}`)
    .digest('hex');
  const response = await fetch(
    `${server}/v1/database/${database}/route/stripe/webhook`,
    {
      method: 'POST',
      headers: {
        'stripe-signature': `t=${now},v1=${valid ? digest : '0'.repeat(64)}`,
      },
      body,
    }
  );
  return response.status;
}

try {
  run(
    'publish',
    '--server',
    server,
    '--module-path',
    'example/spacetimedb',
    '--yes',
    database
  );
  published = true;
  run(
    'call',
    '--server',
    server,
    database,
    'stripe.set_stripe_config',
    JSON.stringify('sk_test_fixture'),
    'null',
    JSON.stringify({ some: secret })
  );
  const info = v.parse(
    v.object({ database_identity: v.object({ __identity__: v.string() }) }),
    await (await fetch(`${server}/v1/database/${database}`)).json()
  );
  const store = info.database_identity.__identity__.replace(/^0x/, '');

  const buyer = await connect();
  const other = await connect();
  assert.ok(buyer.identity);
  const userId = buyer.identity.toHexString();
  const customer = { id: 'cus_example', metadata: { userId } };
  assert.equal(
    await deliver('evt_customer', 'customer.created', customer, false),
    401
  );
  assert.equal(
    (await buyer.procedures.getStoreBilling({})).customerId,
    undefined
  );
  assert.equal(
    await deliver('evt_customer', 'customer.created', customer),
    200
  );
  const session = {
    id: 'cs_example',
    customer: customer.id,
    mode: 'payment',
    status: 'complete',
    payment_status: 'paid',
    metadata: { store, userId },
  };
  // The application callback rejects a missing owner. Both the event and session must roll back.
  assert.equal(
    await deliver('evt_checkout', 'checkout.session.completed', {
      ...session,
      metadata: { store },
    }),
    500
  );
  let billing = await buyer.procedures.getStoreBilling({});
  assert.equal(billing.purchases, 0);
  assert.deepEqual(billing.checkouts, []);
  assert.equal(
    await deliver('evt_checkout', 'checkout.session.completed', session),
    200
  );
  assert.equal(
    await deliver('evt_checkout', 'checkout.session.completed', session),
    200
  );
  assert.equal(
    await deliver('evt_checkout_again', 'checkout.session.completed', session),
    200
  );
  billing = await buyer.procedures.getStoreBilling({});
  assert.equal(billing.purchases, 1);
  assert.equal(billing.checkouts[0].paymentStatus, 'paid');
  assert.equal(
    await deliver('evt_setup', 'checkout.session.completed', {
      ...session,
      id: 'cs_setup',
      mode: 'setup',
      payment_status: 'no_payment_required',
    }),
    200
  );
  assert.equal((await buyer.procedures.getStoreBilling({})).purchases, 1);
  assert.deepEqual(await other.procedures.getStoreBilling({}), {
    customerId: undefined,
    purchases: 0,
    subscriptions: [],
    checkouts: [],
  });
  await assert.rejects(
    other.procedures.stripe.getCustomer({ stripeCustomerId: customer.id }),
    /not_authorized/
  );
  await assert.rejects(
    other.procedures.createStoreCheckoutSession({
      items: [],
      attemptId: 'test-attempt-12345',
    }),
    /invalid_items/
  );
  console.log(
    'Stripe example: ownership, signatures, atomic fulfillment, duplicates, and setup passed.'
  );
} finally {
  for (const connection of connections) connection.disconnect();
  if (published) run('delete', '--server', server, '--yes', database);
}
