// Crash-recovery integration test for @spacetimedb/cron. Runs an isolated
// SpacetimeDB instance so force-stopping it cannot disturb another server. The
// probe procedure remains in flight while the host is killed. The procedure's
// first transaction must preserve the calendar chain before the interruption.
import * as assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const dataDir = await mkdtemp(path.join(os.tmpdir(), 'cron-recovery-'));
const port = await reservePort();
const serverUrl = `http://127.0.0.1:${port}`;
const database = 'cron-recovery';
const serverLogs = [];
let server;

function wait(milliseconds) {
  return new Promise(resolve => setTimeout(resolve, milliseconds));
}

async function reservePort() {
  const listener = net.createServer();
  listener.unref();
  await new Promise((resolve, reject) => {
    listener.once('error', reject);
    listener.listen(0, '127.0.0.1', resolve);
  });
  const address = listener.address();
  assert.ok(address && typeof address === 'object');
  const selected = address.port;
  await new Promise((resolve, reject) =>
    listener.close(error => (error ? reject(error) : resolve()))
  );
  return selected;
}

function run(args) {
  const result = spawnSync('spacetime', args, {
    cwd: root,
    encoding: 'utf8',
    shell: false,
    windowsHide: true,
  });
  assert.ifError(result.error);
  assert.equal(
    result.status,
    0,
    `command failed: spacetime ${args.join(' ')}\n${result.stdout}\n${result.stderr}`
  );
  return result;
}

function call(name, ...args) {
  return run(['call', '--server', serverUrl, database, name, ...args]);
}

function sql(query) {
  const result = run([
    'sql',
    '--server',
    serverUrl,
    '--format',
    'json',
    database,
    query,
  ]);
  return JSON.parse(result.stdout)[0].rows;
}

function statusTag(value) {
  const tags = ['Ok', 'Failed'];
  assert.ok(Array.isArray(value), `unexpected status encoding: ${value}`);
  const tag = tags[value[0]];
  assert.ok(tag, `unknown status variant: ${value[0]}`);
  return tag;
}

function runs() {
  return sql(
    "SELECT invocationId, sequence, status FROM cron_run WHERE jobName = 'recovery_probe'"
  ).map(([invocationId, sequence, status]) => ({
    invocationId,
    sequence: BigInt(sequence),
    status: statusTag(status),
  }));
}

function job() {
  const rows = sql(
    "SELECT enabled, fireCount, consecutiveFailures FROM cron_job WHERE name = 'recovery_probe'"
  );
  assert.equal(rows.length, 1, 'expected the recovery probe job');
  const [enabled, fireCount, consecutiveFailures] = rows[0];
  return {
    enabled,
    fireCount: BigInt(fireCount),
    consecutiveFailures: Number(consecutiveFailures),
  };
}

function blockingInvocationId() {
  const rows = sql(
    'SELECT invocationId FROM recovery_probe_state WHERE singleton = true'
  );
  return rows[0]?.[0];
}

async function poll(description, check, timeoutMilliseconds = 30_000) {
  const deadline = Date.now() + timeoutMilliseconds;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const result = await check();
      if (result) return result;
      lastError = undefined;
    } catch (error) {
      lastError = error;
    }
    await wait(200);
  }
  const detail = lastError instanceof Error ? `: ${lastError.message}` : '';
  throw new Error(`timed out waiting for ${description}${detail}`);
}

function startServer() {
  const child = spawn(
    'spacetime',
    [
      'start',
      '--listen-addr',
      `127.0.0.1:${port}`,
      '--data-dir',
      dataDir,
      '--non-interactive',
    ],
    {
      cwd: root,
      shell: false,
      windowsHide: true,
      stdio: ['ignore', 'pipe', 'pipe'],
    }
  );
  let output = '';
  const capture = chunk => {
    output = `${output}${chunk}`.slice(-20_000);
  };
  child.stdout.on('data', capture);
  child.stderr.on('data', capture);
  serverLogs.push(() => output);
  return child;
}

async function waitForServer(child) {
  await poll(
    'isolated SpacetimeDB server',
    () =>
      new Promise(resolve => {
        if (child.exitCode !== null) {
          resolve(false);
          return;
        }
        const socket = net.createConnection({ host: '127.0.0.1', port });
        socket.once('connect', () => {
          socket.destroy();
          resolve(true);
        });
        socket.once('error', () => resolve(false));
        socket.setTimeout(500, () => {
          socket.destroy();
          resolve(false);
        });
      })
  );
  if (child.exitCode !== null) {
    throw new Error(`isolated server exited early\n${serverLogs.at(-1)?.()}`);
  }
}

async function stopServer(child) {
  if (!child || child.exitCode !== null) return;
  const exited = once(child, 'exit');
  if (process.platform === 'win32') {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], {
      encoding: 'utf8',
      windowsHide: true,
    });
  } else {
    child.kill('SIGKILL');
  }
  await Promise.race([exited, wait(5_000)]);
  if (child.exitCode === null) {
    throw new Error(`failed to stop isolated server process ${child.pid}`);
  }
}

try {
  server = startServer();
  await waitForServer(server);
  run([
    'publish',
    '--server',
    serverUrl,
    '--yes',
    '--module-path',
    'spacetimedb',
    database,
  ]);

  call(
    'schedule_cron',
    JSON.stringify('recovery_probe'),
    JSON.stringify('*/1 * * * * *'),
    JSON.stringify('UTC'),
    '0'
  );
  const blockedInvocation = await poll(
    'the recovery probe to enter its blocking section',
    blockingInvocationId
  );
  assert.match(blockedInvocation, /^recovery_probe:[0-9]+:1$/);
  assert.equal(
    job().fireCount,
    1n,
    'the first transaction must reserve exactly one invocation before blocking'
  );
  assert.equal(
    runs().some(run => run.sequence === 1n),
    false,
    'the blocked invocation must not have a completed outcome'
  );
  assert.equal(
    Number(
      sql(
        "SELECT COUNT(*) AS count FROM tick_log WHERE jobName = 'recovery_probe'"
      )[0][0]
    ),
    0,
    'the blocked handler must not record completed application work'
  );
  assert.equal(
    Number(
      sql(
        "SELECT COUNT(*) AS count FROM recovery_probe_fire WHERE jobName = 'recovery_probe'"
      )[0][0]
    ),
    1,
    'the first transaction must persist the successor before blocking'
  );

  await stopServer(server);
  server = undefined;

  // Leave several calendar occurrences behind us. Recovery should execute at
  // most one overdue occurrence before resuming from the current time.
  await wait(5_000);

  server = startServer();
  await waitForServer(server);
  await poll('a successful post-restart recovery probe', () => {
    const observed = runs();
    const recovered = observed.some(
      run => run.sequence > 1n && run.status === 'Ok'
    );
    const state = job();
    return (
      recovered &&
      state.enabled &&
      state.fireCount >= 2n &&
      state.consecutiveFailures === 0
    );
  });
  await wait(500);
  assert.ok(
    job().fireCount <= 3n,
    'calendar recovery must not replay every occurrence missed during downtime'
  );
  assert.equal(
    Number(
      sql(
        "SELECT COUNT(*) AS count FROM recovery_probe_fire WHERE jobName = 'recovery_probe'"
      )[0][0]
    ),
    1,
    'the recovered calendar job must retain one successor trigger'
  );

  console.log('cron crash-recovery test passed');
} catch (error) {
  const logs = serverLogs.map(
    (read, index) => `server ${index + 1}:\n${read()}`
  );
  if (logs.length > 0) console.error(logs.join('\n'));
  throw error;
} finally {
  await stopServer(server);
  await rm(dataDir, {
    recursive: true,
    force: true,
    maxRetries: 5,
    retryDelay: 200,
  });
}
