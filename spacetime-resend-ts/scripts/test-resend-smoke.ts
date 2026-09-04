// Smoke test: build, publish, ingest synthetic events, verify state. No API key or real webhooks needed. Usage: pnpm run test:resend:smoke [-- --skip-build-publish]

import { spawn } from 'node:child_process';
import { createHmac } from 'node:crypto';

type Options = {
  server: string;
  database: string;
  skipBuildPublish: boolean;
};

function parseArgs(argv: string[]): Options {
  const opts: Options = {
    server: 'http://127.0.0.1:3000',
    // Dedicated DB so smoke test never overwrites the dev module's real config.
    database: 'resend-ts-smoke-test',
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

async function callReducer(
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

function quote(s: string): string {
  return JSON.stringify(s);
}

function some(s: string): string {
  return JSON.stringify({ some: s });
}

const RESEND_WEBHOOK_SECRET_RAW = 'resend_smoke_test_secret';
const RESEND_WEBHOOK_SECRET = `whsec_${Buffer.from(RESEND_WEBHOOK_SECRET_RAW).toString('base64')}`;

function svixSignature(args: {
  eventId: string;
  timestamp: string;
  payloadJson: string;
}): string {
  const digest = createHmac('sha256', RESEND_WEBHOOK_SECRET_RAW)
    .update(`${args.eventId}.${args.timestamp}.${args.payloadJson}`)
    .digest('base64');
  return `v1,${digest}`;
}

async function ingestWebhook(
  opts: Options,
  eventId: string,
  eventType: string,
  payloadJson: string
) {
  const timestamp = String(Math.floor(Date.now() / 1000));
  await callReducer(opts, 'ingest_resend_webhook', [
    quote(eventId),
    quote(eventType),
    quote(payloadJson),
    some(svixSignature({ eventId, timestamp, payloadJson })),
    some(timestamp),
  ]);
}

function eventPayload(args: {
  type: string;
  emailId: string;
  from?: string;
  to?: string[];
  subject?: string;
  extra?: Record<string, unknown>;
}): string {
  const data: Record<string, unknown> = {
    email_id: args.emailId,
    created_at: '2026-05-04T00:00:00Z',
    from: args.from ?? 'onboarding@resend.dev',
    to: args.to ?? ['delivered@resend.dev'],
    subject: args.subject ?? 'smoke test',
    ...(args.extra ?? {}),
  };
  return JSON.stringify({
    type: args.type,
    created_at: '2026-05-04T00:00:00Z',
    data,
  });
}

const EMAIL_STATUS = [
  'Queued',
  'Sent',
  'Delivered',
  'DeliveryDelayed',
  'Bounced',
  'Failed',
  'Cancelled',
] as const;

function emailStatus(rowText: string): string {
  const parsed = JSON.parse(rowText);
  const row = parsed?.[1];
  const variant = row?.[4];
  const tag = variant?.[0];
  if (typeof tag !== 'number' || tag < 0 || tag >= EMAIL_STATUS.length) {
    throw new Error(`could not parse email status from row: ${rowText}`);
  }
  return EMAIL_STATUS[tag]!;
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  // Unique run id so re-runs don't collide on idempotent webhook IDs.
  const RUN = Date.now().toString(36);
  const evt = (suffix: string) => `evt_${RUN}_${suffix}`;
  const em = (suffix: string) => `em_${RUN}_${suffix}`;

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

  // Negative path: send_email refuses cleanly when config is not set.
  step('negative: send_email before config, expect failure');
  const preBootstrap = await expectCallFails(opts, 'send_email', [
    'null',
    JSON.stringify(['delivered@resend.dev']),
    quote('hello'),
    some('<p>hello</p>'),
    'null',
    'null',
    'null',
    'null',
    'null',
    'null',
    'null',
    'null',
  ]);
  if (!preBootstrap.toLowerCase().includes('config')) {
    throw new Error(
      `expected error to mention config; got: ${preBootstrap.slice(0, 400)}`
    );
  }

  step('set_resend_config');
  await callReducer(opts, 'set_resend_config', [
    quote('re_smoke_placeholder'),
    some(RESEND_WEBHOOK_SECRET),
    some('onboarding@resend.dev'),
  ]);

  step('negative: anonymous callers cannot query email state');
  const unauthorized = await expectCallFails(
    opts,
    'get_email',
    [quote(em('a'))],
    true
  );
  if (!unauthorized.toLowerCase().includes('not_authorized')) {
    throw new Error(
      `expected get_email to reject a non-admin caller: ${unauthorized.slice(0, 400)}`
    );
  }

  step('negative: signed webhook type must match supplied event type');
  const mismatchEventId = evt('metadata_mismatch');
  const mismatchPayload = eventPayload({
    type: 'email.delivered',
    emailId: em('metadata_mismatch'),
  });
  const mismatchTimestamp = String(Math.floor(Date.now() / 1000));
  const mismatch = await expectCallFails(opts, 'ingest_resend_webhook', [
    quote(mismatchEventId),
    quote('email.bounced'),
    quote(mismatchPayload),
    some(
      svixSignature({
        eventId: mismatchEventId,
        timestamp: mismatchTimestamp,
        payloadJson: mismatchPayload,
      })
    ),
    some(mismatchTimestamp),
  ]);
  if (!mismatch.toLowerCase().includes('metadata')) {
    throw new Error(
      `expected signed metadata mismatch failure: ${mismatch.slice(0, 400)}`
    );
  }

  // Email A happy path: queued -> sent -> delivered, then flag overlays must not roll status back.
  step(`ingest email.sent for ${em('a')}`);
  await ingestWebhook(
    opts,
    evt('a'),
    'email.sent',
    eventPayload({ type: 'email.sent', emailId: em('a') })
  );

  step(`ingest email.delivered for ${em('a')}`);
  await ingestWebhook(
    opts,
    evt('b'),
    'email.delivered',
    eventPayload({ type: 'email.delivered', emailId: em('a') })
  );

  step(`verify status=delivered for ${em('a')}`);
  let row = await callReducer(opts, 'get_email', [quote(em('a'))]);
  if (emailStatus(row) !== 'Delivered') {
    throw new Error(`expected delivered for ${em('a')}, got: ${row}`);
  }

  step(`ingest email.complained for ${em('a')} (flag, must NOT change status)`);
  await ingestWebhook(
    opts,
    evt('e'),
    'email.complained',
    eventPayload({ type: 'email.complained', emailId: em('a') })
  );
  row = await callReducer(opts, 'get_email', [quote(em('a'))]);
  if (emailStatus(row) !== 'Delivered') {
    throw new Error(`complained should not flip status: ${row}`);
  }

  step(`ingest email.opened for ${em('a')} (flag)`);
  await ingestWebhook(
    opts,
    evt('g'),
    'email.opened',
    eventPayload({ type: 'email.opened', emailId: em('a') })
  );
  row = await callReducer(opts, 'get_email', [quote(em('a'))]);
  if (emailStatus(row) !== 'Delivered') {
    throw new Error(`opened should not flip status: ${row}`);
  }

  step(`ingest email.clicked for ${em('a')} (flag + detail captured)`);
  await ingestWebhook(
    opts,
    evt('h'),
    'email.clicked',
    eventPayload({
      type: 'email.clicked',
      emailId: em('a'),
      extra: {
        click: {
          ipAddress: '203.0.113.42',
          link: 'https://spacetimedb.com',
          timestamp: '2026-05-04T00:00:00Z',
          userAgent: 'Mozilla/5.0 (smoke-test)',
        },
      },
    })
  );
  const clickEvents = await callReducer(
    opts,
    'list_delivery_events_for_email',
    [quote(em('a'))]
  );
  const clickRows: unknown = JSON.parse(clickEvents);
  const clickRow = Array.isArray(clickRows)
    ? clickRows.find(row => Array.isArray(row) && row[2] === 'email.clicked')
    : undefined;
  if (!clickRow) {
    throw new Error(`expected click event in delivery log: ${clickEvents}`);
  }
  const detailOption = Array.isArray(clickRow) ? clickRow[4] : undefined;
  const detailJson =
    Array.isArray(detailOption) &&
    detailOption[0] === 0 &&
    typeof detailOption[1] === 'string'
      ? detailOption[1]
      : undefined;
  const clickDetail = detailJson ? JSON.parse(detailJson) : undefined;
  if (clickDetail?.link !== 'https://spacetimedb.com') {
    throw new Error(`expected click detail (link) preserved: ${clickEvents}`);
  }

  // Email C bounce path with structured detail.
  step(`ingest email.bounced for ${em('c')}`);
  await ingestWebhook(
    opts,
    evt('c'),
    'email.bounced',
    eventPayload({
      type: 'email.bounced',
      emailId: em('c'),
      to: ['bounced@resend.dev'],
      extra: {
        bounce: {
          message: 'Mailbox does not exist',
          subType: 'NoEmail',
          type: 'Permanent',
        },
      },
    })
  );
  const bouncedRow = await callReducer(opts, 'get_email', [quote(em('c'))]);
  if (emailStatus(bouncedRow) !== 'Bounced') {
    throw new Error(`expected bounced for ${em('c')}: ${bouncedRow}`);
  }
  if (!bouncedRow.includes('Mailbox does not exist')) {
    throw new Error(`expected bounce reason in row: ${bouncedRow}`);
  }

  // Email D delivery_delayed status path.
  step(`ingest email.delivery_delayed for ${em('d')}`);
  await ingestWebhook(
    opts,
    evt('d'),
    'email.delivery_delayed',
    eventPayload({ type: 'email.delivery_delayed', emailId: em('d') })
  );
  const delayed = await callReducer(opts, 'get_email', [quote(em('d'))]);
  if (emailStatus(delayed) !== 'DeliveryDelayed') {
    throw new Error(`expected delivery_delayed for ${em('d')}: ${delayed}`);
  }

  // Email F failed status with reason.
  step(`ingest email.failed for ${em('f')}`);
  await ingestWebhook(
    opts,
    evt('f'),
    'email.failed',
    eventPayload({
      type: 'email.failed',
      emailId: em('f'),
      extra: { failed: { reason: 'rate limited by destination MTA' } },
    })
  );
  const failedRow = await callReducer(opts, 'get_email', [quote(em('f'))]);
  if (emailStatus(failedRow) !== 'Failed') {
    throw new Error(`expected failed for ${em('f')}: ${failedRow}`);
  }
  if (!failedRow.includes('rate limited')) {
    throw new Error(`expected failure reason in row: ${failedRow}`);
  }

  // Idempotency + replay.
  step(
    `verify idempotency: re-ingest ${evt('a')} (already processed, should no-op)`
  );
  await ingestWebhook(
    opts,
    evt('a'),
    'email.sent',
    eventPayload({ type: 'email.sent', emailId: em('a') })
  );
  row = await callReducer(opts, 'get_email', [quote(em('a'))]);
  if (emailStatus(row) !== 'Delivered') {
    throw new Error(`replay broke status: ${row}`);
  }

  step(`replay_webhook_event re-applies ${evt('b')} (status stays delivered)`);
  await callReducer(opts, 'replay_webhook_event', [quote(evt('b'))]);
  row = await callReducer(opts, 'get_email', [quote(em('a'))]);
  if (emailStatus(row) !== 'Delivered') {
    throw new Error(`replay reducer altered state: ${row}`);
  }

  step('negative: replay unknown event_id, expect failure');
  const replayMissing = await expectCallFails(opts, 'replay_webhook_event', [
    quote('evt_does_not_exist_xyz'),
  ]);
  if (!replayMissing.toLowerCase().includes('not_found')) {
    throw new Error(
      `expected not_found error; got: ${replayMissing.slice(0, 400)}`
    );
  }

  step(
    'done: smoke test passed (8 event types + idempotency + replay + 2 negative)'
  );
}

main().catch(err => {
  process.stderr.write(
    `\nSMOKE TEST FAILED: ${err instanceof Error ? err.message : String(err)}\n`
  );
  process.exit(1);
});
