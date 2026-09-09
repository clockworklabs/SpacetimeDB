// Synthetic-payload smoke test; locks in webhook behavior without Stripe CLI/keys.

import { spawn } from 'node:child_process';
import { createHmac } from 'node:crypto';

type Options = {
  server: string;
  database: string;
  skipBuildPublish: boolean;
};

function parseArgs(argv: string[]): Options {
  const opts: Options = {
    server: 'local',
    // Dedicated database so the smoke test never overwrites dev module config with placeholders.
    database: 'stripe-ts-smoke-test',
    skipBuildPublish: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const raw = argv[i]!;
    const flag = raw.replace(/^-+/, '').toLowerCase();
    if (flag === 'skip-build-publish') opts.skipBuildPublish = true;
    if (flag === 'server') opts.server = argv[++i]!;
    if (flag === 'database') opts.database = argv[++i]!;
  }
  return opts;
}

function step(name: string) {
  process.stdout.write(`\n==> ${name}\n`);
}

function run(
  cmd: string,
  args: string[]
): Promise<{ code: number; stdout: string; stderr: string }> {
  return new Promise(resolve => {
    const child = spawn(cmd, args, { shell: false });
    let stdout = '';
    let stderr = '';
    child.stdout?.on('data', d => (stdout += String(d)));
    child.stderr?.on('data', d => (stderr += String(d)));
    child.on('close', code => resolve({ code: code ?? 1, stdout, stderr }));
    child.on('error', err =>
      resolve({ code: 1, stdout, stderr: stderr + String(err) })
    );
  });
}

async function call(
  opts: Options,
  name: string,
  args: string[]
): Promise<string> {
  const result = await run('spacetime', [
    'call',
    '--server',
    opts.server,
    opts.database,
    name,
    ...args,
  ]);
  if (result.code !== 0) {
    throw new Error(
      `spacetime call ${name} failed: code=${result.code}\nstderr: ${result.stderr}\nstdout: ${result.stdout}`
    );
  }
  return result.stdout;
}

// Expects the call to fail. Returns combined stderr/stdout for assertion.
async function expectCallFails(
  opts: Options,
  name: string,
  args: string[],
  anonymous = false
): Promise<string> {
  const result = await run('spacetime', [
    'call',
    ...(anonymous ? ['--anonymous'] : []),
    '--server',
    opts.server,
    opts.database,
    name,
    ...args,
  ]);
  if (result.code === 0) {
    throw new Error(
      `expected ${name} to fail but it succeeded:\nstdout: ${result.stdout}`
    );
  }
  return result.stderr + result.stdout;
}

const q = (s: string) => JSON.stringify(s);
const some = (s: string) => JSON.stringify({ some: s });
const STRIPE_WEBHOOK_SECRET = 'whsec_smoke_test_secret';

function stripeSignature(rawBody: string): string {
  const ts = Math.floor(Date.now() / 1000);
  const digest = createHmac('sha256', STRIPE_WEBHOOK_SECRET)
    .update(`${ts}.${rawBody}`)
    .digest('hex');
  return `t=${ts},v1=${digest}`;
}

async function ingest(
  opts: Options,
  args: {
    eventId: string;
    eventType: string;
    livemode?: boolean;
    payload: object;
  }
) {
  const payloadJson = JSON.stringify(args.payload);
  await call(opts, 'ingest_stripe_webhook', [
    q(args.eventId),
    q(args.eventType),
    String(args.livemode ?? false),
    q(payloadJson),
    some(stripeSignature(payloadJson)),
  ]);
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));

  if (!opts.skipBuildPublish) {
    step('spacetime build');
    const build = await run('spacetime', ['build']);
    if (build.code !== 0) {
      process.stderr.write(build.stderr);
      throw new Error('build failed');
    }

    step(`spacetime publish --server ${opts.server} ${opts.database}`);
    const publish = await run('spacetime', [
      'publish',
      '--server',
      opts.server,
      '--yes',
      '--delete-data',
      opts.database,
    ]);
    if (publish.code !== 0) {
      process.stderr.write(publish.stderr);
      throw new Error('publish failed');
    }
  }

  // Procedures needing Stripe secret should refuse cleanly before config.
  step('negative: get_or_create_customer before config, expect failure');
  const preBootstrap = await expectCallFails(opts, 'get_or_create_customer', [
    q('u_no_config_yet'),
    'null',
    'null',
  ]);
  if (!preBootstrap.toLowerCase().includes('config')) {
    throw new Error(
      `expected error to mention config; got: ${preBootstrap.slice(0, 400)}`
    );
  }

  step('set_stripe_config');
  await call(opts, 'set_stripe_config', [
    q('sk_test_smoke_placeholder'),
    'null',
    some(STRIPE_WEBHOOK_SECRET),
  ]);

  step('negative: anonymous callers cannot read or mutate Stripe state');
  for (const [name, args] of [
    ['get_customer', [q('cus_smoke_1')]],
    ['create_customer', ['null', 'null', 'null', 'null']],
    [
      'upsert_customer',
      [q('cus_anonymous'), 'null', 'null', 'null', 'null', 'null'],
    ],
  ] as const) {
    const unauthorized = await expectCallFails(opts, name, [...args], true);
    if (!unauthorized.toLowerCase().includes('not_authorized')) {
      throw new Error(
        `expected ${name} to reject a non-admin caller: ${unauthorized.slice(0, 400)}`
      );
    }
  }

  step('negative: signed webhook metadata must match supplied metadata');
  const mismatchPayload = JSON.stringify({
    id: 'evt_smoke_signed_id',
    type: 'customer.created',
    data: { object: { id: 'cus_should_not_exist' } },
  });
  const mismatch = await expectCallFails(opts, 'ingest_stripe_webhook', [
    q('evt_smoke_forged_id'),
    q('customer.deleted'),
    'false',
    q(mismatchPayload),
    some(stripeSignature(mismatchPayload)),
  ]);
  if (!mismatch.toLowerCase().includes('metadata')) {
    throw new Error(
      `expected signed metadata mismatch failure: ${mismatch.slice(0, 400)}`
    );
  }

  step('ingest customer.created, expect customer row');
  await ingest(opts, {
    eventId: 'evt_smoke_cust_1',
    eventType: 'customer.created',
    payload: {
      id: 'evt_smoke_cust_1',
      type: 'customer.created',
      data: {
        object: {
          id: 'cus_smoke_1',
          email: 'smoke@example.com',
          name: 'Smoke Test',
          metadata: { userId: 'u_smoke_1' },
        },
      },
    },
  });
  const customer = await call(opts, 'get_customer', [q('cus_smoke_1')]);
  if (!customer.includes('smoke@example.com')) {
    throw new Error(
      `customer.created did not produce expected row: ${customer}`
    );
  }

  step(
    'ingest customer.subscription.created, expect subscription row + metadata'
  );
  await ingest(opts, {
    eventId: 'evt_smoke_sub_1',
    eventType: 'customer.subscription.created',
    payload: {
      id: 'evt_smoke_sub_1',
      type: 'customer.subscription.created',
      data: {
        object: {
          id: 'sub_smoke_1',
          customer: 'cus_smoke_1',
          status: 'active',
          current_period_end: 1735000000,
          cancel_at_period_end: false,
          items: {
            data: [
              {
                current_period_end: 1735000000,
                quantity: 1,
                price: { id: 'price_smoke_1' },
              },
            ],
          },
          metadata: { userId: 'u_smoke_1', orgId: 'o_smoke_1' },
        },
      },
    },
  });
  const sub = await call(opts, 'get_subscription', [q('sub_smoke_1')]);
  if (!sub.includes('"active"') || !sub.includes('o_smoke_1')) {
    throw new Error(
      `subscription.created did not produce expected row: ${sub}`
    );
  }

  step('ingest checkout.session.completed, expect session row');
  await ingest(opts, {
    eventId: 'evt_smoke_chk_1',
    eventType: 'checkout.session.completed',
    payload: {
      id: 'evt_smoke_chk_1',
      type: 'checkout.session.completed',
      data: {
        object: {
          id: 'cs_smoke_1',
          mode: 'subscription',
          customer: 'cus_smoke_1',
          metadata: { userId: 'u_smoke_1' },
        },
      },
    },
  });
  const session = await call(opts, 'get_checkout_session', [q('cs_smoke_1')]);
  if (!session.includes('"complete"') || !session.includes('subscription')) {
    throw new Error(
      `checkout.session.completed did not produce expected row: ${session}`
    );
  }

  step('ingest invoice.created, expect invoice row');
  await ingest(opts, {
    eventId: 'evt_smoke_inv_1',
    eventType: 'invoice.created',
    payload: {
      id: 'evt_smoke_inv_1',
      type: 'invoice.created',
      data: {
        object: {
          id: 'in_smoke_1',
          customer: 'cus_smoke_1',
          subscription: 'sub_smoke_1',
          status: 'open',
          amount_due: 999,
          amount_paid: 0,
          created: 1735000000,
        },
      },
    },
  });
  let invoice = await call(opts, 'list_invoices', [q('cus_smoke_1')]);
  if (!invoice.includes('in_smoke_1')) {
    throw new Error(`invoice.created did not produce row: ${invoice}`);
  }

  step('ingest invoice.paid, expect status=paid + carry-over fields');
  await ingest(opts, {
    eventId: 'evt_smoke_inv_2',
    eventType: 'invoice.paid',
    payload: {
      id: 'evt_smoke_inv_2',
      type: 'invoice.paid',
      data: {
        object: {
          id: 'in_smoke_1',
          customer: 'cus_smoke_1',
          status: 'paid',
          amount_paid: 999,
        },
      },
    },
  });
  invoice = await call(opts, 'list_invoices', [q('cus_smoke_1')]);
  if (!invoice.includes('"paid"')) {
    throw new Error(`invoice.paid did not flip status: ${invoice}`);
  }

  step('ingest payment_intent.succeeded standalone, expect payment row');
  await ingest(opts, {
    eventId: 'evt_smoke_pay_1',
    eventType: 'payment_intent.succeeded',
    payload: {
      id: 'evt_smoke_pay_1',
      type: 'payment_intent.succeeded',
      data: {
        object: {
          id: 'pi_smoke_1',
          customer: 'cus_smoke_2',
          amount: 1999,
          currency: 'usd',
          status: 'succeeded',
          created: 1735000100,
          metadata: { userId: 'u_smoke_2' },
        },
      },
    },
  });
  const payment = await call(opts, 'get_payment', [q('pi_smoke_1')]);
  if (!payment.includes('1999') || !payment.includes('"usd"')) {
    throw new Error(`payment_intent.succeeded did not produce row: ${payment}`);
  }

  step('negative: replay unknown event_id, expect failure');
  const replayMissing = await expectCallFails(opts, 'replay_webhook_event', [
    q('evt_does_not_exist_xyz'),
  ]);
  if (!replayMissing.toLowerCase().includes('not_found')) {
    throw new Error(
      `expected not_found error; got: ${replayMissing.slice(0, 400)}`
    );
  }

  step('verify idempotency: re-ingest evt_smoke_cust_1, row unchanged');
  await ingest(opts, {
    eventId: 'evt_smoke_cust_1',
    eventType: 'customer.created',
    payload: {
      id: 'evt_smoke_cust_1',
      type: 'customer.created',
      data: { object: { id: 'cus_smoke_1', email: 'CHANGED@example.com' } },
    },
  });
  const customerAfter = await call(opts, 'get_customer', [q('cus_smoke_1')]);
  if (!customerAfter.includes('smoke@example.com')) {
    throw new Error(`replay broke customer row: ${customerAfter}`);
  }

  step('done: stripe smoke test passed');
}

main().catch(err => {
  process.stderr.write(
    `\nSMOKE TEST FAILED: ${err instanceof Error ? err.message : String(err)}\n`
  );
  process.exit(1);
});
