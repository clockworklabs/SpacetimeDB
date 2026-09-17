import * as http from 'node:http';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { setTimeout as sleep } from 'node:timers/promises';
import { existsSync } from 'node:fs';

type Options = {
  server: string;
  database: string;
  relayPort: number;
  stripeCliPath: string;
  stripeProjectName?: string;
  stripeApiKey?: string;
  events: string[];
  deliveryWaitSeconds: number;
  listenerWarmupSeconds: number;
  skipBuildPublish: boolean;
  skipCheckoutTrigger: boolean;
  killExistingStripeListeners: boolean;
};

type CmdResult = {
  code: number;
  stdout: string;
  stderr: string;
};

type ProcessedRelayEvent = {
  eventId: string;
  eventType: string;
  objectId?: string;
};

const DEFAULT_EVENTS = [
  'customer.created',
  'customer.subscription.created',
  'payment_intent.succeeded',
  'checkout.session.completed',
  'invoice.paid',
];
const pnpmCommand = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';

function step(name: string) {
  process.stdout.write(`\n==> ${name}\n`);
}

function info(line: string) {
  process.stdout.write(`    ${line}\n`);
}

function tailLines(text: string, count: number) {
  return text.split(/\r?\n/).filter(Boolean).slice(-count);
}

function normalizeFlag(flag: string) {
  return flag.replace(/^-+/, '').toLowerCase();
}

function parseArgs(argv: string[]): Options {
  const opts: Options = {
    server: 'local',
    database: 'stripe-ts-e2e',
    relayPort: 12111,
    stripeCliPath: 'stripe',
    events: [...DEFAULT_EVENTS],
    deliveryWaitSeconds: 20,
    listenerWarmupSeconds: 3,
    skipBuildPublish: false,
    skipCheckoutTrigger: false,
    killExistingStripeListeners: false,
  };

  const takeValue = (i: number, flag: string) => {
    if (i + 1 >= argv.length) {
      throw new Error(`Missing value for --${flag}`);
    }
    return argv[i + 1]!;
  };

  for (let i = 0; i < argv.length; i++) {
    const raw = argv[i]!;
    if (raw === '--') continue;
    if (!raw.startsWith('-')) continue;
    const flag = normalizeFlag(raw);

    if (flag === 'help' || flag === 'h') {
      process.stdout.write(`Usage:
  pnpm run test:stripe:e2e -- [options]

Options:
  --server <name>
  --database <name>
  --relay-port <number>
  --stripe-cli-path <path>
  --stripe-project-name <name>
  --stripe-api-key <key>
  --events <csv>
  --delivery-wait-seconds <n>
  --listener-warmup-seconds <n>
  --skip-build-publish
  --skip-checkout-trigger
  --kill-existing-stripe-listeners
`);
      process.exit(0);
    }

    if (flag === 'skipbuildpublish' || flag === 'skip-build-publish') {
      opts.skipBuildPublish = true;
      continue;
    }
    if (flag === 'skipcheckouttrigger' || flag === 'skip-checkout-trigger') {
      opts.skipCheckoutTrigger = true;
      continue;
    }
    if (
      flag === 'killexistingstripelisteners' ||
      flag === 'kill-existing-stripe-listeners'
    ) {
      opts.killExistingStripeListeners = true;
      continue;
    }

    const value = takeValue(i, flag);
    i++;

    switch (flag) {
      case 'server':
        opts.server = value;
        break;
      case 'database':
        opts.database = value;
        break;
      case 'relayport':
      case 'relay-port':
        opts.relayPort = Number.parseInt(value, 10);
        break;
      case 'stripeclipath':
      case 'stripe-cli-path':
        opts.stripeCliPath = value;
        break;
      case 'stripeprojectname':
      case 'stripe-project-name':
        opts.stripeProjectName = value;
        break;
      case 'stripeapikey':
      case 'stripe-api-key':
        opts.stripeApiKey = value;
        break;
      case 'events':
        opts.events = value
          .split(',')
          .map(v => v.trim())
          .filter(Boolean);
        break;
      case 'deliverywaitseconds':
      case 'delivery-wait-seconds':
        opts.deliveryWaitSeconds = Number.parseInt(value, 10);
        break;
      case 'listenerwarmupseconds':
      case 'listener-warmup-seconds':
        opts.listenerWarmupSeconds = Number.parseInt(value, 10);
        break;
      default:
        throw new Error(`Unknown flag: ${raw}`);
    }
  }

  if (!Number.isFinite(opts.relayPort) || opts.relayPort <= 0) {
    throw new Error('relay port must be a positive integer');
  }
  if (
    !Number.isFinite(opts.deliveryWaitSeconds) ||
    opts.deliveryWaitSeconds <= 0
  ) {
    throw new Error('delivery wait seconds must be a positive integer');
  }
  if (
    !Number.isFinite(opts.listenerWarmupSeconds) ||
    opts.listenerWarmupSeconds < 0
  ) {
    throw new Error('listener warmup seconds must be >= 0');
  }
  if (opts.events.length === 0) {
    throw new Error('at least one event is required');
  }

  return opts;
}

function spawnCapture(
  file: string,
  args: string[],
  options?: { cwd?: string; env?: NodeJS.ProcessEnv }
): Promise<CmdResult> {
  return new Promise((resolve, reject) => {
    const child = spawn(file, args, {
      cwd: options?.cwd,
      env: options?.env,
      stdio: ['ignore', 'pipe', 'pipe'],
      shell: process.platform === 'win32' && file.endsWith('.cmd'),
      windowsHide: true,
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', chunk => {
      stdout += chunk.toString();
    });
    child.stderr.on('data', chunk => {
      stderr += chunk.toString();
    });
    child.on('error', reject);
    child.on('close', code => {
      resolve({ code: code ?? -1, stdout, stderr });
    });
  });
}

function spawnInherit(
  file: string,
  args: string[],
  options?: { cwd?: string; env?: NodeJS.ProcessEnv }
): Promise<number> {
  return new Promise((resolve, reject) => {
    const child = spawn(file, args, {
      cwd: options?.cwd,
      env: options?.env,
      stdio: 'inherit',
      shell: process.platform === 'win32' && file.endsWith('.cmd'),
      windowsHide: false,
    });
    child.on('error', reject);
    child.on('close', code => resolve(code ?? -1));
  });
}

async function runChecked(
  file: string,
  args: string[],
  options?: { cwd?: string; env?: NodeJS.ProcessEnv }
) {
  info([file, ...args].join(' '));
  const code = await spawnInherit(file, args, options);
  if (code !== 0) {
    throw new Error(`Command failed with exit code ${code}: ${file}`);
  }
}

async function checkCommand(file: string, args: string[]) {
  const out = await spawnCapture(file, args);
  if (out.code !== 0) {
    throw new Error(`Required command not runnable: ${file}`);
  }
}

async function getWebhookEventCount(server: string, database: string) {
  return getSqlCount(
    server,
    database,
    'select count(*) as c from stripe_webhook_event'
  );
}

async function getSqlCount(server: string, database: string, query: string) {
  const result = await spawnCapture('spacetime', [
    'sql',
    '--server',
    server,
    database,
    query,
  ]);
  if (result.code !== 0) {
    throw new Error(
      `Failed SQL query.\n${query}\n${result.stderr || result.stdout}`
    );
  }
  const merged = `${result.stdout}\n${result.stderr}`;
  const lines = merged.split(/\r?\n/);
  const numberLine = lines.find(line => /^\s*\d+\s*$/.test(line));
  if (!numberLine) {
    throw new Error(
      `Could not parse SQL count output.\nQuery: ${query}\n${merged}`
    );
  }
  return Number.parseInt(numberLine.trim(), 10);
}

function sqlStringLiteral(value: string) {
  return `'${value.replace(/'/g, "''")}'`;
}

async function assertCountAtLeast(
  server: string,
  database: string,
  query: string,
  min: number,
  label: string
) {
  const count = await getSqlCount(server, database, query);
  if (count < min) {
    throw new Error(
      `Assertion failed: ${label}. Expected >= ${min}, got ${count}. Query: ${query}`
    );
  }
}

async function assertEventWorkflowState(
  server: string,
  database: string,
  event: ProcessedRelayEvent
) {
  const eventIdLiteral = sqlStringLiteral(event.eventId);
  await assertCountAtLeast(
    server,
    database,
    `select count(*) as c from stripe_webhook_event where event_id = ${eventIdLiteral}`,
    1,
    `webhook event row exists for ${event.eventId}`
  );

  if (!event.objectId) return;
  const objectIdLiteral = sqlStringLiteral(event.objectId);
  switch (event.eventType) {
    case 'customer.created':
    case 'customer.updated':
      await assertCountAtLeast(
        server,
        database,
        `select count(*) as c from stripe_customer where stripe_customer_id = ${objectIdLiteral}`,
        1,
        `customer row exists for ${event.objectId}`
      );
      return;
    case 'customer.subscription.created':
    case 'customer.subscription.updated':
    case 'customer.subscription.deleted':
      await assertCountAtLeast(
        server,
        database,
        `select count(*) as c from stripe_subscription where stripe_subscription_id = ${objectIdLiteral}`,
        1,
        `subscription row exists for ${event.objectId}`
      );
      return;
    case 'checkout.session.completed':
      await assertCountAtLeast(
        server,
        database,
        `select count(*) as c from stripe_checkout_session where stripe_checkout_session_id = ${objectIdLiteral}`,
        1,
        `checkout session row exists for ${event.objectId}`
      );
      return;
    case 'payment_intent.succeeded':
      // Invoice-attached and recent-subscription payment intents are
      // ignored to avoid duplicate or orphan payment rows. The
      // aggregate assertion in main verifies that at least one standalone
      // trigger produced a payment row.
      return;
    case 'invoice.created':
    case 'invoice.finalized':
    case 'invoice.paid':
    case 'invoice.payment_succeeded':
    case 'invoice.payment_failed':
      await assertCountAtLeast(
        server,
        database,
        `select count(*) as c from stripe_invoice where stripe_invoice_id = ${objectIdLiteral}`,
        1,
        `invoice row exists for ${event.objectId}`
      );
      return;
    default:
      return;
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const moduleRoot = process.cwd();
  const relayOutput: string[] = [];
  let stripeListen: ReturnType<typeof spawn> | undefined;
  let stripeStdout = '';
  let stripeStderr = '';
  let relayServer: http.Server | undefined;
  let relayHits = 0;
  const processedEvents: ProcessedRelayEvent[] = [];
  let triggeredEvents: string[] = [];

  try {
    step('Check required CLI tools');
    await checkCommand('spacetime', ['--help']);
    if (!options.skipBuildPublish) {
      await checkCommand(pnpmCommand, ['--version']);
    }
    if (options.stripeCliPath === 'stripe') {
      await checkCommand('stripe', ['version']);
    } else if (!existsSync(options.stripeCliPath)) {
      throw new Error(`Stripe CLI not found at path: ${options.stripeCliPath}`);
    }

    step('Clean stale Stripe listeners');
    if (options.killExistingStripeListeners) {
      const kill = await spawnCapture('taskkill', [
        '/F',
        '/IM',
        'stripe.exe',
        '/T',
      ]);
      if (kill.code === 0) {
        info('stopped existing stripe.exe processes.');
      } else {
        info('no existing stripe.exe processes to stop (or no permission).');
      }
    } else {
      info('skipped (pass --kill-existing-stripe-listeners to force cleanup).');
    }

    if (!options.skipBuildPublish) {
      step('Build module');
      await runChecked(pnpmCommand, ['run', 'build'], { cwd: moduleRoot });

      step('Publish module locally');
      await runChecked(
        'spacetime',
        ['publish', '--server', options.server, '--yes', options.database],
        { cwd: moduleRoot }
      );
    }

    step('Read current webhook event count');
    const beforeCount = await getWebhookEventCount(
      options.server,
      options.database
    );
    const beforePaymentCount = await getSqlCount(
      options.server,
      options.database,
      'select count(*) as c from stripe_payment'
    );
    info(`Current webhook rows: ${beforeCount}`);

    step('Start local relay endpoint');
    relayServer = http.createServer(
      async (req: http.IncomingMessage, res: http.ServerResponse) => {
        if (req.method !== 'POST' || req.url !== '/stripe-webhook/') {
          res.statusCode = 404;
          res.end('not found');
          return;
        }

        const chunks: Buffer[] = [];
        const signatureHeader = req.headers['stripe-signature'];
        req.on('data', (chunk: Buffer) => chunks.push(Buffer.from(chunk)));
        await once(req, 'end');
        relayHits++;

        let event: Record<string, unknown>;
        let rawBody: string;
        try {
          rawBody = Buffer.concat(chunks).toString('utf8');
          const parsed: unknown = JSON.parse(rawBody);
          if (typeof parsed !== 'object' || parsed === null) {
            throw new Error('event must be an object');
          }
          event = parsed as Record<string, unknown>;
        } catch (err) {
          const msg = `relay error status=400 msg=invalid-json: ${String(err)}`;
          relayOutput.push(msg);
          res.statusCode = 400;
          res.end('invalid json');
          return;
        }

        if (typeof event.id !== 'string' || typeof event.type !== 'string') {
          const msg = 'relay error status=400 msg=missing-event-id-or-type';
          relayOutput.push(msg);
          res.statusCode = 400;
          res.end('invalid event');
          return;
        }
        if (typeof signatureHeader !== 'string' || !signatureHeader) {
          relayOutput.push(
            `relay event id=${event.id} type=${event.type} status=400 missing-signature`
          );
          res.statusCode = 400;
          res.end('missing signature');
          return;
        }

        const callArgs = [
          'call',
          '--server',
          options.server,
          options.database,
          'ingest_stripe_webhook',
          JSON.stringify(String(event.id)),
          JSON.stringify(String(event.type)),
          event.livemode === true ? 'true' : 'false',
          JSON.stringify(rawBody),
          JSON.stringify({ some: signatureHeader }),
        ];

        const ingest = await spawnCapture('spacetime', callArgs);
        if (ingest.code !== 0) {
          const msg = `relay event id=${event.id} type=${event.type} status=500 ingest-failed`;
          relayOutput.push(msg);
          relayOutput.push(
            tailLines(`${ingest.stdout}\n${ingest.stderr}`, 20).join('\n')
          );
          res.statusCode = 500;
          res.end('ingest failed');
          return;
        }

        processedEvents.push({
          eventId: String(event.id),
          eventType: String(event.type),
          objectId: (() => {
            const data =
              typeof event.data === 'object' && event.data !== null
                ? (event.data as Record<string, unknown>)
                : undefined;
            const object =
              typeof data?.object === 'object' && data.object !== null
                ? (data.object as Record<string, unknown>)
                : undefined;
            return typeof object?.id === 'string' ? object.id : undefined;
          })(),
        });
        const msg = `relay event id=${event.id} type=${event.type} status=200`;
        relayOutput.push(msg);
        res.statusCode = 200;
        res.end('ok');
      }
    );

    await new Promise<void>((resolve, reject) => {
      relayServer!.once('error', reject);
      relayServer!.listen(options.relayPort, '127.0.0.1', () => resolve());
    });
    info(
      `Relay listening on http://127.0.0.1:${options.relayPort}/stripe-webhook/`
    );

    step('Start stripe listener process');
    const eventsCsv = options.events.join(',');
    const forwardUrl = `http://127.0.0.1:${options.relayPort}/stripe-webhook/`;
    const listenArgs = [
      'listen',
      '--events',
      eventsCsv,
      '--forward-to',
      forwardUrl,
    ];
    if (options.stripeProjectName) {
      listenArgs.push('--project-name', options.stripeProjectName);
    }
    if (options.stripeApiKey) {
      listenArgs.push('--api-key', options.stripeApiKey);
    }

    stripeListen = spawn(options.stripeCliPath, listenArgs, {
      stdio: ['ignore', 'pipe', 'pipe'],
      shell: false,
      windowsHide: true,
    });
    if (!stripeListen.stdout || !stripeListen.stderr) {
      throw new Error('Failed to capture stripe listen stdout/stderr streams.');
    }
    stripeListen.stdout.on('data', chunk => {
      stripeStdout += chunk.toString();
    });
    stripeListen.stderr.on('data', chunk => {
      stripeStderr += chunk.toString();
    });

    const readyDeadline = Date.now() + 30_000;
    while (Date.now() < readyDeadline) {
      if (stripeListen.exitCode !== null) {
        throw new Error(
          `stripe listen exited early.\nSTDOUT:\n${stripeStdout}\nSTDERR:\n${stripeStderr}`
        );
      }
      if (
        stripeStdout.includes('Ready!') ||
        stripeStdout.includes('webhook signing secret') ||
        stripeStderr.includes('Ready!') ||
        stripeStderr.includes('webhook signing secret')
      ) {
        break;
      }
      await sleep(250);
    }
    if (
      !stripeStdout.includes('Ready!') &&
      !stripeStderr.includes('Ready!') &&
      !stripeStdout.includes('webhook signing secret') &&
      !stripeStderr.includes('webhook signing secret')
    ) {
      throw new Error(
        `stripe listen did not become ready.\nSTDOUT:\n${stripeStdout}\nSTDERR:\n${stripeStderr}`
      );
    }

    info(`stripe listen PID: ${stripeListen.pid ?? 'unknown'}`);
    info(`forwarding to: ${forwardUrl}`);
    info(`relay port: ${options.relayPort}`);
    const listenerOutput = `${stripeStdout}\n${stripeStderr}`;
    const signingSecret = listenerOutput.match(/whsec_[A-Za-z0-9]+/)?.[0];
    if (!signingSecret) {
      throw new Error(
        'stripe listen became ready without reporting a webhook signing secret'
      );
    }
    step('Configure the listener webhook secret in the module');
    let configureSecret = await spawnCapture('spacetime', [
      'call',
      '--server',
      options.server,
      options.database,
      'set_stripe_webhook_signing_secret',
      JSON.stringify(signingSecret),
    ]);
    const configureOutput = `${configureSecret.stdout}\n${configureSecret.stderr}`;
    if (
      configureSecret.code !== 0 &&
      configureOutput.includes('config_not_set')
    ) {
      configureSecret = await spawnCapture('spacetime', [
        'call',
        '--server',
        options.server,
        options.database,
        'set_stripe_config',
        JSON.stringify('sk_test_e2e_webhook_only'),
        'null',
        JSON.stringify({ some: signingSecret }),
      ]);
    }
    if (configureSecret.code !== 0) {
      throw new Error(
        `could not configure the Stripe listener secret: ${tailLines(`${configureSecret.stdout}\n${configureSecret.stderr}`, 10).join('\n')}`
      );
    }
    info('listener signing secret configured (value redacted).');
    if (options.listenerWarmupSeconds > 0) {
      info(`warming listener for ${options.listenerWarmupSeconds}s...`);
      await sleep(options.listenerWarmupSeconds * 1000);
    }

    const preflight = await fetch(forwardUrl, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: '{}',
    });
    info(`relay preflight HTTP status: ${preflight.status}`);

    step('Trigger test events through Stripe CLI');
    const eventsToRun = options.skipCheckoutTrigger
      ? options.events.filter(
          eventName => eventName !== 'checkout.session.completed'
        )
      : options.events;
    triggeredEvents = [...eventsToRun];
    for (const eventName of eventsToRun) {
      const triggerArgs = ['trigger', eventName];
      if (options.stripeProjectName) {
        triggerArgs.push('--project-name', options.stripeProjectName);
      }
      if (options.stripeApiKey) {
        triggerArgs.push('--api-key', options.stripeApiKey);
      }
      await runChecked(options.stripeCliPath, triggerArgs);
      await sleep(1000);
    }

    step('Validate webhook rows increased');
    if (relayOutput.length > 0) {
      process.stdout.write('\n');
      info('relay output (tail):');
      for (const line of relayOutput.slice(-60)) {
        info(`  ${line}`);
      }
    } else {
      info('relay output: no inbound webhook requests observed.');
    }

    const waitDeadline = Date.now() + options.deliveryWaitSeconds * 1000;
    let afterCount = beforeCount;
    while (Date.now() < waitDeadline) {
      await sleep(1000);
      afterCount = await getWebhookEventCount(options.server, options.database);
      if (afterCount > beforeCount) break;
    }
    const delta = afterCount - beforeCount;
    info(`Before: ${beforeCount}`);
    info(`After:  ${afterCount}`);
    info(`Delta:  ${delta}`);

    if (delta <= 0) {
      process.stdout.write('\n');
      info('stripe listen stdout (tail):');
      for (const line of tailLines(stripeStdout, 80)) info(`  ${line}`);
      process.stdout.write('\n');
      info('stripe listen stderr (tail):');
      for (const line of tailLines(stripeStderr, 80)) info(`  ${line}`);
      process.stdout.write('\n');
      info(`relay request count: ${relayHits}`);
      throw new Error(
        'No new webhook events were ingested. Expected delta > 0.'
      );
    }

    for (const eventType of triggeredEvents) {
      if (!processedEvents.some(event => event.eventType === eventType)) {
        throw new Error(
          `Assertion failed: did not observe forwarded event type ${eventType} in relay output.`
        );
      }
    }
    for (const event of processedEvents) {
      await assertEventWorkflowState(options.server, options.database, event);
    }
    if (triggeredEvents.includes('payment_intent.succeeded')) {
      const afterPaymentCount = await getSqlCount(
        options.server,
        options.database,
        'select count(*) as c from stripe_payment'
      );
      if (afterPaymentCount <= beforePaymentCount) {
        throw new Error(
          'Assertion failed: no standalone payment_intent.succeeded event produced a payment row.'
        );
      }
    }
    info(
      `workflow assertions passed for ${processedEvents.length} forwarded event(s).`
    );

    process.stdout.write('\nStripe E2E test passed.\n');
  } finally {
    if (stripeListen && stripeListen.exitCode === null && stripeListen.pid) {
      try {
        await spawnCapture('taskkill', [
          '/PID',
          String(stripeListen.pid),
          '/T',
          '/F',
        ]);
      } catch {
        // Ignore cleanup failures.
      }
    }
    if (relayServer) {
      await new Promise<void>(resolve => relayServer!.close(() => resolve()));
    }
  }
}

main().catch(err => {
  process.stderr.write(`${err instanceof Error ? err.message : String(err)}\n`);
  process.exit(1);
});
