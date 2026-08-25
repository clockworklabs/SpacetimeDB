#!/usr/bin/env node

/* global document */

import { spawn, spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const isWindows = process.platform === 'win32';
const pnpmCommand = isWindows ? 'pnpm.cmd' : 'pnpm';
const spacetimeCommand = isWindows ? 'spacetime.exe' : 'spacetime';
const confirmFlag = '--confirm-delete-data';
const browserFlag = '--browser';
const ephemeralFlag = '--ephemeral';

const examples = [
  {
    dir: 'spacetime-stripe-ts/example',
    database: 'spacetime-stripe-example',
    port: 8787,
  },
  {
    dir: 'spacetime-cron-ts/example',
    database: 'spacetime-cron-example',
    port: 8788,
  },
  {
    dir: 'spacetime-agents-ts/example',
    database: 'spacetime-agents-example',
    port: 8789,
  },
  {
    dir: 'spacetime-resend-ts/example',
    database: 'spacetime-resend-example',
    port: 8790,
  },
  {
    dir: 'spacetime-auth-ts/example',
    database: 'spacetime-auth-example',
    port: 8791,
  },
  {
    dir: 'spacetime-rate-limit-ts/example',
    database: 'spacetime-rate-limit-example',
    port: 8792,
  },
  {
    dir: 'spacetime-grid-ts/example',
    database: 'spacetime-grid-example',
    port: 8793,
  },
  {
    dir: 'spacetime-presence-ts/example',
    database: 'spacetime-presence-example',
    port: 8794,
  },
  {
    dir: 'spacetime-posthog-ts/example',
    database: 'spacetime-posthog-example',
    port: 8796,
  },
  {
    dir: 'spacetime-lobby-ts/example',
    database: 'spacetime-lobby-example',
    port: 8797,
  },
  {
    dir: 'spacetime-api-keys-ts/example',
    database: 'spacetime-api-keys-example',
    port: 8798,
  },
  {
    dir: 'spacetime-files-ts/example',
    database: 'spacetime-files-example',
    port: 8799,
  },
];

function selectedExamples() {
  const onlyIndex = process.argv.indexOf('--only');
  if (onlyIndex < 0) return examples;
  const requested = process.argv[onlyIndex + 1];
  if (!requested)
    throw new Error('--only requires an example directory or database name');
  const selected = examples.filter(
    item => item.dir === requested || item.database === requested
  );
  if (selected.length === 0) throw new Error(`unknown example: ${requested}`);
  return selected;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? root,
    env: options.env ?? process.env,
    encoding: 'utf8',
    stdio: options.inherit ? 'inherit' : 'pipe',
    shell: isWindows && command === pnpmCommand,
    windowsHide: true,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const output = `${result.stdout ?? ''}${result.stderr ?? ''}`.trim();
    throw new Error(
      `${command} ${args.join(' ')} exited with ${result.status}${output ? `\n${output}` : ''}`
    );
  }
  return `${result.stdout ?? ''}${result.stderr ?? ''}`;
}

function smokeEnvironment(example) {
  return {
    ...process.env,
    HOST: '127.0.0.1',
    PORT: String(example.port),
    STDB_URI: 'ws://127.0.0.1:3000',
    STDB_HTTP: 'http://127.0.0.1:3000',
    STDB_SERVER: 'http://127.0.0.1:3000',
    STDB_DATABASE: example.database,
    STDB_APP_DATABASE: example.database,
    AUTH_ISSUER_URL: `http://127.0.0.1:${example.port}`,
    AUTH_BASE_URL: `http://127.0.0.1:${example.port}`,
    AUTH_COOKIE_NAME: 'stdb_auth',
    AUTH_SESSION_TTL_SECONDS: '604800',
    AUTH_ES256_PRIVATE_KEY_PEM: '',
    // The generic smoke suite must never spend money or mutate provider accounts.
    OPENROUTER_API_KEY: '',
    OPENAI_API_KEY: '',
    ANTHROPIC_API_KEY: '',
    POSTHOG_PROJECT_API_KEY: '',
    STRIPE_SECRET_KEY: '',
    STRIPE_SYNC_PRICES: '0',
    RESEND_API_KEY: '',
    RESEND_WEBHOOK_SECRET: '',
    GOOGLE_CLIENT_ID: '',
    GOOGLE_CLIENT_SECRET: '',
    GITHUB_CLIENT_ID: '',
    GITHUB_CLIENT_SECRET: '',
  };
}

function startServer(example) {
  const chunks = [];
  const tsxCli = resolve(
    root,
    example.dir,
    'node_modules',
    'tsx',
    'dist',
    'cli.mjs'
  );
  const child = spawn(process.execPath, [tsxCli, 'server.ts'], {
    cwd: resolve(root, example.dir),
    env: smokeEnvironment(example),
    windowsHide: true,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const capture = chunk => {
    chunks.push(chunk.toString());
    if (chunks.join('').length > 40_000) chunks.shift();
  };
  child.stdout.on('data', capture);
  child.stderr.on('data', capture);
  return { child, output: () => chunks.join('') };
}

async function stopServer(child) {
  if (child.exitCode !== null) return;
  if (isWindows) {
    const killed = spawnSync(
      'taskkill.exe',
      ['/pid', String(child.pid), '/t', '/f'],
      {
        stdio: 'ignore',
        windowsHide: true,
        timeout: 10_000,
      }
    );
    if (killed.error || killed.status !== 0) child.kill();
  } else {
    child.kill('SIGTERM');
  }
  await Promise.race([
    new Promise(resolveExit => child.once('exit', resolveExit)),
    new Promise(resolveTimeout => setTimeout(resolveTimeout, 5_000)),
  ]);
  if (child.exitCode === null) {
    child.kill('SIGKILL');
    await Promise.race([
      new Promise(resolveExit => child.once('exit', resolveExit)),
      new Promise(resolveTimeout => setTimeout(resolveTimeout, 2_000)),
    ]);
    if (child.exitCode === null) {
      throw new Error(`failed to stop example server process ${child.pid}`);
    }
  }
}

async function request(url, options) {
  return fetch(url, { ...options, signal: AbortSignal.timeout(2_000) });
}

async function waitForHealth(example, server) {
  const deadline = Date.now() + 45_000;
  const url = `http://127.0.0.1:${example.port}/api/health`;
  let lastError = 'server did not answer';
  while (Date.now() < deadline) {
    if (server.child.exitCode !== null) {
      throw new Error(
        `server exited with ${server.child.exitCode}\n${server.output()}`
      );
    }
    try {
      const response = await request(url);
      const body = await response.json();
      const reportedDatabase = body.database ?? body.app;
      if (
        !response.ok ||
        body.ok !== true ||
        reportedDatabase !== example.database
      ) {
        throw new Error(
          `unexpected health response ${response.status}: ${JSON.stringify(body)}`
        );
      }
      return;
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
      await new Promise(resolveWait => setTimeout(resolveWait, 250));
    }
  }
  throw new Error(`${lastError}\n${server.output()}`);
}

async function checkHttpSurface(example) {
  const origin = `http://127.0.0.1:${example.port}`;
  const rootResponse = await request(`${origin}/`);
  if (
    !rootResponse.ok ||
    !(rootResponse.headers.get('content-type') ?? '').includes('text/html')
  ) {
    throw new Error(`GET / did not return HTML (${rootResponse.status})`);
  }

  const configResponse = await request(`${origin}/api/config`);
  const config = await configResponse.json();
  const configuredDatabase = config.database ?? config.appDatabase;
  if (!configResponse.ok || configuredDatabase !== example.database) {
    throw new Error(
      `unexpected /api/config response: ${JSON.stringify(config)}`
    );
  }

  if (example.baseDatabase === 'spacetime-posthog-example') {
    const removedRoute = await request(`${origin}/api/admin/identity`);
    if (removedRoute.status !== 404)
      throw new Error('removed PostHog admin route is reachable');
  }
  if (example.baseDatabase === 'spacetime-stripe-example') {
    for (const route of [
      '/api/admin/configure',
      '/api/admin/seed',
      '/api/admin/sync',
    ]) {
      const removedRoute = await request(`${origin}${route}`, {
        method: 'POST',
      });
      if (removedRoute.status !== 404)
        throw new Error(`removed Stripe admin route is reachable: ${route}`);
    }
    const unavailableCheckout = await request(`${origin}/api/checkout`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: '{}',
    });
    if (unavailableCheckout.status !== 503) {
      throw new Error(
        `unconfigured Stripe checkout returned ${unavailableCheckout.status}, expected 503`
      );
    }
  }
  if (example.baseDatabase === 'spacetime-resend-example') {
    const unsigned = await request(`${origin}/webhook/resend`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: '{}',
    });
    if (unsigned.status !== 400) {
      throw new Error(
        `unsigned Resend webhook returned ${unsigned.status}, expected 400`
      );
    }
  }
}

async function waitForEnabled(page, selector) {
  await page.locator(selector).waitFor({ state: 'visible' });
  await page.waitForFunction(
    target => !document.querySelector(target)?.hasAttribute('disabled'),
    selector
  );
}

async function checkExampleInteraction(example, page) {
  switch (example.baseDatabase) {
    case 'spacetime-stripe-example':
      await page.locator('#btnCart').click();
      await page.waitForFunction(
        () => !document.querySelector('#cartPopout')?.hasAttribute('hidden')
      );
      await page.locator('#btnCartClose').click();
      return;

    case 'spacetime-cron-example':
      await page.waitForFunction(
        () =>
          document.querySelector('#connection')?.dataset.state === 'connected'
      );
      await page.waitForFunction(
        () => Number(document.querySelector('#stat-jobs')?.textContent) === 2
      );
      await page.locator('#open-scheduler').click();
      await page.locator('#job-name').waitFor({ state: 'visible' });
      return;

    case 'spacetime-agents-example':
      await page.locator('#toggle-link').click();
      await page.waitForFunction(
        () =>
          document.querySelector('#auth-title')?.textContent ===
          'Create an account'
      );
      return;

    case 'spacetime-resend-example':
      await page.locator('#subject-input').fill('Browser smoke');
      await page.locator('#message-input').fill('Rendered preview');
      await page.locator('.msg-tab[data-tab="preview"]').click();
      await page.waitForFunction(
        () =>
          !document.querySelector('#message-preview')?.hasAttribute('hidden')
      );
      return;

    case 'spacetime-auth-example':
      await page.locator('#toggle-link').click();
      await page.waitForFunction(
        () =>
          document.querySelector('#auth-title')?.textContent ===
          'Create account'
      );
      return;

    case 'spacetime-rate-limit-example': {
      try {
        await waitForEnabled(page, '#tapBtn');
      } catch (error) {
        const diagnostics = await page.evaluate(() => ({
          connection: globalThis.__reactorConnectionState,
          tapVisible: document.querySelector('#tapBtn') != null,
          tapDisabled:
            document.querySelector('#tapBtn')?.hasAttribute('disabled') ?? null,
          hasActionApi: typeof globalThis.reactor?.tap === 'function',
        }));
        throw new Error(
          `Rate Limit did not become ready: ${JSON.stringify(diagnostics)}`,
          { cause: error }
        );
      }
      if (
        await page.evaluate(
          () => globalThis.__reactorConnectedBeforeReady === true
        )
      ) {
        throw new Error('Rate Limit reported connected before its action API');
      }
      const before = await page.locator('#energy').textContent();
      await page.locator('#tapBtn').click();
      try {
        await page.waitForFunction(
          previous =>
            document.querySelector('#energy')?.textContent !== previous,
          before
        );
      } catch (error) {
        const diagnostics = await page.evaluate(() => ({
          connectedBeforeReady:
            globalThis.__reactorConnectedBeforeReady === true,
          energy: document.querySelector('#energy')?.textContent ?? null,
          tapDisabled:
            document.querySelector('#tapBtn')?.hasAttribute('disabled') ?? null,
          hasActionApi: typeof globalThis.reactor?.tap === 'function',
        }));
        throw new Error(
          `Rate Limit tap did not update energy: ${JSON.stringify(diagnostics)}`,
          { cause: error }
        );
      }
      return;
    }

    case 'spacetime-grid-example':
      await page.locator('#toggle-link').click();
      await page.waitForFunction(
        () =>
          document.querySelector('#auth-title')?.textContent ===
          'Create an account'
      );
      return;

    case 'spacetime-presence-example':
      await page.locator('#toggleLink').click();
      await page.waitForFunction(
        () =>
          document.querySelector('#landingAuthTitle')?.textContent ===
          'Create an account'
      );
      return;

    case 'spacetime-posthog-example':
      await waitForEnabled(page, '#tickOnce');
      await page.locator('#tickOnce').click();
      return;

    case 'spacetime-lobby-example':
      await waitForEnabled(page, '#findDuel');
      await page.locator('#displayName').fill('Browser Smoke');
      await page.locator('#findDuel').click();
      await page.waitForFunction(
        () =>
          document
            .querySelector('#waitingScreen')
            ?.classList.contains('active') ||
          document.querySelector('#duelScreen')?.classList.contains('active')
      );
      return;

    case 'spacetime-api-keys-example':
      await page.waitForFunction(
        () => document.querySelector('#connChip')?.dataset.state === 'connected'
      );
      await page.locator('#shareBtn').click();
      await page.locator('#keyNameInput').fill('Browser smoke');
      await page.locator('#createKeyBtn').click();
      await page.waitForFunction(
        () =>
          !document.querySelector('#linkBox')?.hasAttribute('hidden') &&
          document.querySelectorAll('#keyList [data-key]').length === 1
      );
      {
        const firstLink = await page
          .locator('#linkBox .link-code')
          .textContent();
        await page.locator('#keyList [data-rotate]').click();
        await page.waitForFunction(
          previous =>
            document.querySelector('#linkBox .link-code')?.textContent !==
            previous,
          firstLink
        );
      }
      await page.locator('#keyList [data-revoke]').click();
      await page.waitForFunction(
        () => document.querySelectorAll('#keyList [data-key]').length === 0
      );
      return;

    case 'spacetime-files-example':
      await waitForEnabled(page, '#new-folder');
      await page.locator('#new-folder').click();
      await page.waitForFunction(() =>
        document.querySelector('#dialog')?.classList.contains('open')
      );
      await page.locator('#dialog-cancel').click();
      await page.locator('#file-input').setInputFiles([
        { name: 'a.txt', mimeType: 'text/plain', buffer: Buffer.from('a') },
        { name: 'b.txt', mimeType: 'text/plain', buffer: Buffer.from('b') },
        { name: 'c.txt', mimeType: 'text/plain', buffer: Buffer.from('c') },
      ]);
      await page.waitForFunction(
        () => document.querySelectorAll('[data-file]').length === 3
      );
      await page.locator('[data-file="/a.txt"]').click();
      await page
        .locator('[data-file="/c.txt"]')
        .click({ modifiers: ['Shift'] });
      await page.waitForFunction(
        () =>
          document.querySelectorAll('[data-file].selected').length === 3 &&
          document.querySelector('#bulk-count')?.textContent === '3 selected'
      );
      await page.locator('#bulk-public').click();
      await page.waitForFunction(
        () =>
          document.querySelectorAll(
            '[data-file] .vis-dot.public, [data-file] .badge.public'
          ).length === 3
      );
      await page.locator('#bulk-delete').click();
      await page.waitForFunction(() =>
        document.querySelector('#dialog')?.classList.contains('open')
      );
      await page.locator('#dialog-ok').click();
      await page.waitForFunction(
        () => document.querySelectorAll('[data-file]').length === 0
      );
      return;

    default:
      throw new Error(
        `missing browser interaction for ${example.baseDatabase}`
      );
  }
}

async function checkBrowserSurface(example, browser) {
  const origin = `http://127.0.0.1:${example.port}`;
  const context = await browser.newContext();
  const page = await context.newPage();
  const errors = [];

  if (example.baseDatabase === 'spacetime-rate-limit-example') {
    await page.addInitScript(() => {
      globalThis.__reactorConnectedBeforeReady = false;
      globalThis.__reactorConnectionState = null;
      globalThis.addEventListener('reactor:connState', event => {
        globalThis.__reactorConnectionState = event.detail ?? null;
        if (
          event.detail?.state === 'connected' &&
          typeof globalThis.reactor?.tap !== 'function'
        ) {
          globalThis.__reactorConnectedBeforeReady = true;
        }
      });
    });
  }

  page.on('pageerror', error => errors.push(`page error: ${error.message}`));
  page.on('console', message => {
    if (message.type() !== 'error') return;
    if (message.text().startsWith('Failed to load resource:')) return;
    errors.push(`console: ${message.text()}`);
  });
  page.on('response', response => {
    if (response.status() < 400) return;
    const url = new URL(response.url());
    if (
      url.origin === origin &&
      response.status() === 401 &&
      url.pathname === '/auth/session/refresh'
    ) {
      return;
    }
    errors.push(
      `HTTP ${response.status()}: ${response.request().method()} ${url.href}`
    );
  });
  page.on('requestfailed', request => {
    const url = new URL(request.url());
    if (
      !['document', 'script', 'stylesheet', 'xhr', 'fetch'].includes(
        request.resourceType()
      )
    ) {
      return;
    }
    errors.push(
      `request failed: ${request.method()} ${url.href} (${request.failure()?.errorText ?? 'unknown'})`
    );
  });

  try {
    const response = await page.goto(origin, { waitUntil: 'load' });
    if (!response?.ok()) {
      throw new Error(
        `browser GET / returned ${response?.status() ?? 'no response'}`
      );
    }
    await page.waitForFunction(() =>
      [...document.styleSheets].some(sheet =>
        sheet.href?.endsWith('/styles.css')
      )
    );
    await checkExampleInteraction(example, page);
    await page.waitForTimeout(250);
    if (errors.length > 0) throw new Error(errors.join('\n'));
  } finally {
    await context.close();
  }
}

async function smoke(example, browser) {
  const startedAt = Date.now();
  console.log(`\n[smoke] ${example.dir}: fresh publish as ${example.database}`);
  if (process.argv.includes(ephemeralFlag)) {
    run(
      spacetimeCommand,
      [
        'publish',
        '--server',
        'local',
        '--yes',
        '--module-path',
        resolve(root, example.dir, 'spacetimedb'),
        example.database,
      ],
      { inherit: true }
    );
    run(pnpmCommand, ['--dir', example.dir, 'run', 'build:codegen'], {
      inherit: true,
    });
    run(pnpmCommand, ['--dir', example.dir, 'run', 'build:app'], {
      inherit: true,
    });
  } else {
    try {
      run(pnpmCommand, ['--dir', example.dir, 'run', 'build:module:fresh'], {
        inherit: true,
      });
    } catch (firstError) {
      console.warn(
        `[smoke] ${example.dir}: fresh publish failed; retrying once`
      );
      try {
        run(pnpmCommand, ['--dir', example.dir, 'run', 'build:module:fresh'], {
          inherit: true,
        });
      } catch (retryError) {
        throw new Error(
          `${retryError instanceof Error ? retryError.message : String(retryError)}\nFirst attempt: ${firstError instanceof Error ? firstError.message : String(firstError)}`
        );
      }
    }
  }

  console.log(
    `[smoke] ${example.dir}: start and probe http://127.0.0.1:${example.port}`
  );
  const server = startServer(example);
  try {
    await waitForHealth(example, server);
    await checkHttpSurface(example);
    if (browser) await checkBrowserSurface(example, browser);
  } catch (error) {
    const output = server.output().trim();
    throw new Error(
      `${error instanceof Error ? error.message : String(error)}${output ? `\nServer output:\n${output}` : ''}`
    );
  } finally {
    await stopServer(server.child);
  }
  console.log(
    `[smoke] ${example.dir}: passed (${((Date.now() - startedAt) / 1000).toFixed(1)}s)`
  );
}

async function main() {
  const ephemeral = process.argv.includes(ephemeralFlag);
  if (!process.argv.includes(confirmFlag) && !ephemeral) {
    console.error(
      `Refusing to replace local example databases without ${confirmFlag}.`
    );
    console.error(
      'This suite runs each build:module:fresh script with --delete-data=always.'
    );
    process.exit(2);
  }

  run(process.execPath, [resolve(root, 'tools/check-spacetime-release.mjs')], {
    inherit: true,
  });
  run(spacetimeCommand, ['server', 'ping', 'local']);
  run(spacetimeCommand, ['login', 'show']);

  const suffix = `smoke-${process.pid}-${Date.now()}`;
  const selected = selectedExamples().map(example => ({
    ...example,
    baseDatabase: example.database,
    database: ephemeral ? `${example.database}-${suffix}` : example.database,
  }));
  const failures = [];
  const browser = process.argv.includes(browserFlag)
    ? await chromium.launch({ headless: true })
    : undefined;
  try {
    for (const example of selected) {
      try {
        await smoke(example, browser);
      } catch (error) {
        failures.push({
          example: example.dir,
          error: error instanceof Error ? error.message : String(error),
        });
        console.error(
          `[smoke] ${example.dir}: FAILED\n${failures.at(-1).error}`
        );
      } finally {
        if (ephemeral) {
          try {
            run(spacetimeCommand, [
              'delete',
              '--server',
              'local',
              '--yes',
              example.database,
            ]);
          } catch (error) {
            failures.push({
              example: example.dir,
              error: `ephemeral cleanup failed: ${error instanceof Error ? error.message : String(error)}`,
            });
          }
        }
      }
    }
  } finally {
    await browser?.close();
  }

  if (failures.length > 0) {
    console.error(
      `\nExample smoke failures (${failures.length}/${selected.length}):`
    );
    for (const failure of failures)
      console.error(`- ${failure.example}: ${failure.error.split('\n')[0]}`);
    process.exit(1);
  }
  console.log(
    `\nFresh-database smoke passed for ${selected.length} example app(s).`
  );
}

main().catch(error => {
  console.error(error instanceof Error ? error.stack : String(error));
  process.exit(1);
});
