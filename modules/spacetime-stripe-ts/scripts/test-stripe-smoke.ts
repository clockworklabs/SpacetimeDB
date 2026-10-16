// Synthetic-payload smoke test; locks in webhook behavior without Stripe CLI/keys.

import { spawn } from 'node:child_process';
import { createHmac } from 'node:crypto';

type Options = {
  server: string;
  database: string;
  skipBuildPublish: boolean;
  httpUrl?: string;
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
    if (flag === 'http-url') opts.httpUrl = argv[++i]!;
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
const EVENT_CREATED = 1735000000;

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

const WEBHOOK_EVENT_STATUS = [
  'Received',
  'Processed',
  'Ignored',
  'Failed',
] as const;

// `spacetime call` prints an option as [0, row] and an enum as [tag, []].
// The stripe_webhook_event status is column 4.
function webhookEventStatus(output: string): string {
  const tag = JSON.parse(output)?.[1]?.[4]?.[0];
  const status = WEBHOOK_EVENT_STATUS[tag];
  if (!status)
    throw new Error(`could not parse webhook event status from: ${output}`);
  return status;
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

  if (opts.httpUrl) {
    step('signed HTTP delivery: failed payloads stay retryable');
    const post = async (payload: object, expectedStatus: number) => {
      const body = JSON.stringify(payload);
      const response = await fetch(
        `${opts.httpUrl}/v1/database/${opts.database}/route/stripe/webhook`,
        {
          method: 'POST',
          headers: {
            'content-type': 'application/json',
            'stripe-signature': stripeSignature(body),
          },
          body,
        }
      );
      if (response.status !== expectedStatus) {
        throw new Error(
          `expected HTTP ${expectedStatus}, got ${response.status}: ${await response.text()}`
        );
      }
    };
    for (const [eventId, eventType, object, expectedStatus] of [
      ['evt_http_invalid', 'customer.created', {}, 400],
      ['evt_http_valid', 'customer.created', { id: 'cus_http_valid' }, 200],
      ['evt_http_ignored', 'unhandled.event', {}, 200],
    ] as const) {
      for (let attempt = 0; attempt < 2; attempt++) {
        await post(
          {
            id: eventId,
            type: eventType,
            created: EVENT_CREATED,
            data: { object },
          },
          expectedStatus
        );
      }
    }
    const paid = {
      id: 'evt_http_paid',
      type: 'invoice.paid',
      created: EVENT_CREATED,
      data: { object: { id: 'in_http_retry', customer: null } },
    };
    await post(paid, 400);
    await ingest(opts, {
      eventId: 'evt_http_invoice',
      eventType: 'invoice.created',
      payload: {
        id: 'evt_http_invoice',
        type: 'invoice.created',
        created: EVENT_CREATED,
        data: {
          object: {
            id: 'in_http_retry',
            customer: 'cus_http_valid',
            status: 'open',
          },
        },
      },
    });
    await post(paid, 200);
    const recovered = await call(opts, 'list_invoices', [q('cus_http_valid')]);
    if (!recovered.includes('"paid"'))
      throw new Error(`failed event was not retried: ${recovered}`);
  }

  step('relay keeps a Failed row for an invalid signed payload');
  const invalidPayload = {
    id: 'evt_relay_invalid',
    type: 'customer.created',
    created: EVENT_CREATED,
    data: { object: {} },
  };
  const readInvalidEvent = async () =>
    webhookEventStatus(
      await call(opts, 'get_webhook_event', [q('evt_relay_invalid')])
    );
  for (let attempt = 0; attempt < 2; attempt++) {
    await ingest(opts, {
      eventId: 'evt_relay_invalid',
      eventType: 'customer.created',
      payload: invalidPayload,
    });
    const status = await readInvalidEvent();
    if (status !== 'Failed')
      throw new Error(`expected Failed webhook row after ingest: ${status}`);
  }
  await call(opts, 'replay_webhook_event', [q('evt_relay_invalid')]);
  const replayed = await readInvalidEvent();
  if (replayed !== 'Failed')
    throw new Error(`expected Failed webhook row after replay: ${replayed}`);

  step('negative: anonymous callers cannot read or mutate Stripe state');
  for (const [name, args] of [
    ['get_customer', [q('cus_smoke_1')]],
    ['get_or_create_customer', [q('u_anonymous'), 'null', 'null']],
    ['replay_webhook_event', [q('evt_smoke_cust_1')]],
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
    created: EVENT_CREATED,
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
      created: EVENT_CREATED,
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
      created: EVENT_CREATED,
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
      created: EVENT_CREATED,
      data: {
        object: {
          id: 'cs_smoke_1',
          status: 'complete',
          payment_status: 'paid',
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
      created: EVENT_CREATED,
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
      created: EVENT_CREATED,
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

  step('late invoice.finalized cannot reopen a paid invoice');
  await ingest(opts, {
    eventId: 'evt_smoke_inv_late',
    eventType: 'invoice.finalized',
    payload: {
      id: 'evt_smoke_inv_late',
      type: 'invoice.finalized',
      created: EVENT_CREATED,
      data: {
        object: { id: 'in_smoke_1', customer: 'cus_smoke_1', status: 'open' },
      },
    },
  });
  invoice = await call(opts, 'list_invoices', [q('cus_smoke_1')]);
  if (!invoice.includes('"paid"')) {
    throw new Error(`late invoice.finalized reopened invoice: ${invoice}`);
  }

  step('late customer.subscription.updated cannot revive a canceled one');
  for (const [eventId, eventType, created, status] of [
    ['evt_smoke_sub_del', 'customer.subscription.deleted', 20, 'canceled'],
    ['evt_smoke_sub_late', 'customer.subscription.updated', 10, 'active'],
  ] as const) {
    await ingest(opts, {
      eventId,
      eventType,
      payload: {
        id: eventId,
        type: eventType,
        created: EVENT_CREATED + created,
        data: {
          object: { id: 'sub_smoke_1', customer: 'cus_smoke_1', status },
        },
      },
    });
  }
  const canceled = await call(opts, 'get_subscription', [q('sub_smoke_1')]);
  if (!canceled.includes('"canceled"')) {
    throw new Error(`late subscription event revived row: ${canceled}`);
  }

  step('ingest payment_intent.succeeded standalone, expect payment row');
  await ingest(opts, {
    eventId: 'evt_smoke_pay_1',
    eventType: 'payment_intent.succeeded',
    payload: {
      id: 'evt_smoke_pay_1',
      type: 'payment_intent.succeeded',
      created: EVENT_CREATED,
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

  step('invoice_payment.paid before its invoice, expect user filled in');
  await ingest(opts, {
    eventId: 'evt_smoke_inpay_early',
    eventType: 'invoice_payment.paid',
    payload: {
      id: 'evt_smoke_inpay_early',
      type: 'invoice_payment.paid',
      created: EVENT_CREATED,
      data: {
        object: {
          invoice: 'in_smoke_early',
          amount_paid: 999,
          currency: 'usd',
          created: EVENT_CREATED,
          payment: { payment_intent: 'pi_smoke_early' },
        },
      },
    },
  });
  await ingest(opts, {
    eventId: 'evt_smoke_inv_early',
    eventType: 'invoice.paid',
    payload: {
      id: 'evt_smoke_inv_early',
      type: 'invoice.paid',
      created: EVENT_CREATED,
      data: {
        object: {
          id: 'in_smoke_early',
          customer: 'cus_smoke_1',
          parent: { subscription_details: { subscription: 'sub_smoke_1' } },
          status: 'paid',
          amount_paid: 999,
        },
      },
    },
  });
  const earlyPayment = await call(opts, 'get_payment', [q('pi_smoke_early')]);
  if (
    !earlyPayment.includes('u_smoke_1') ||
    !earlyPayment.includes('cus_smoke_1')
  ) {
    throw new Error(`invoice did not fill payment row: ${earlyPayment}`);
  }

  step('invoice before its subscription, expect user from invoice metadata');
  await ingest(opts, {
    eventId: 'evt_smoke_inv_nosub',
    eventType: 'invoice.paid',
    payload: {
      id: 'evt_smoke_inv_nosub',
      type: 'invoice.paid',
      created: EVENT_CREATED,
      data: {
        object: {
          id: 'in_smoke_nosub',
          customer: 'cus_smoke_3',
          parent: {
            subscription_details: {
              subscription: 'sub_smoke_later',
              metadata: { userId: 'u_smoke_3' },
            },
          },
          status: 'paid',
        },
      },
    },
  });
  const noSubInvoice = await call(opts, 'list_invoices', [q('cus_smoke_3')]);
  if (!noSubInvoice.includes('u_smoke_3')) {
    throw new Error(`invoice metadata user not recorded: ${noSubInvoice}`);
  }

  step('checkout.session.async_payment_failed, expect paymentStatus=failed');
  for (const [eventId, eventType] of [
    ['evt_smoke_chk_async', 'checkout.session.completed'],
    ['evt_smoke_chk_failed', 'checkout.session.async_payment_failed'],
  ] as const) {
    await ingest(opts, {
      eventId,
      eventType,
      payload: {
        id: eventId,
        type: eventType,
        created: EVENT_CREATED,
        data: {
          object: {
            id: 'cs_smoke_async',
            status: 'complete',
            payment_status: 'unpaid',
            mode: 'payment',
          },
        },
      },
    });
  }
  const failedSession = await call(opts, 'get_checkout_session', [
    q('cs_smoke_async'),
  ]);
  if (!failedSession.includes('"failed"')) {
    throw new Error(`async payment failure not recorded: ${failedSession}`);
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
      created: EVENT_CREATED,
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
