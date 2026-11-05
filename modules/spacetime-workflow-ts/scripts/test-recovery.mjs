// Integration test for @spacetimedb/workflow. Runs an isolated SpacetimeDB
// instance so force-stopping it cannot disturb another server. Covers rollback,
// retries, sleep, events, cancellation, and a host killed in the middle of a step.
import * as assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const dataDir = await mkdtemp(path.join(os.tmpdir(), 'workflow-recovery-'));
const port = await reservePort();
const serverUrl = `http://127.0.0.1:${port}`;
const database = 'workflow-recovery';
const STATUSES = ['Running', 'Waiting', 'Completed', 'Failed', 'Cancelled'];
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

function latestRun() {
  const rows = sql(
    'SELECT id, status, lastStep, attempt, error FROM workflow_run'
  );
  const [id, status, lastStep, attempt, error] = rows.sort((a, b) =>
    Number(BigInt(b[0]) - BigInt(a[0]))
  )[0];
  return {
    id: BigInt(id),
    status: STATUSES[status[0]],
    lastStep: lastStep[0] === 0 ? lastStep[1] : undefined,
    attempt: Number(attempt),
    error: error[0] === 0 ? error[1] : undefined,
  };
}

function stepLog(runId) {
  return sql(
    `SELECT step, attempt, note FROM step_log WHERE runId = ${runId}`
  ).map(([step, attempt, note]) => ({
    step,
    attempt: Number(attempt),
    note,
  }));
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

const pollRun = (runId, status) =>
  poll(`run ${runId} to be ${status}`, () => {
    const latest = latestRun();
    return latest.id === runId && latest.status === status ? latest : undefined;
  });

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

// A process killed by a signal reports signalCode and leaves exitCode null.
function hasExited(child) {
  return child.exitCode !== null || child.signalCode !== null;
}

async function waitForServer(child) {
  await poll(
    'isolated SpacetimeDB server',
    () =>
      new Promise(resolve => {
        if (hasExited(child)) {
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
  if (hasExited(child)) {
    throw new Error(`isolated server exited early\n${serverLogs.at(-1)?.()}`);
  }
}

async function stopServer(child) {
  if (!child || hasExited(child)) return;
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
  if (!hasExited(child)) {
    throw new Error(`failed to stop isolated server process ${child.pid}`);
  }
}

let failed = false;
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

  // Two failures roll back their writes, then the run sleeps and waits. The
  // last step reads a result saved before the sleep.
  call('start_demo', JSON.stringify('demo'), '2');
  const waiting = await poll('the demo run to wait for its event', () => {
    const latest = latestRun();
    return latest.status === 'Waiting' ? latest : undefined;
  });
  assert.equal(waiting.lastStep, 'sleep');
  assert.deepEqual(
    stepLog(waiting.id).map(row => [row.step, row.attempt]),
    [['flaky', 3]],
    'failed attempts must leave no writes behind'
  );
  call('signal_run', String(waiting.id), JSON.stringify('go'));
  await pollRun(waiting.id, 'Completed');
  assert.deepEqual(
    stepLog(waiting.id)
      .map(row => [row.step, row.note])
      .at(-1),
    ['finish', 'saved:8']
  );
  assert.equal(
    sql(`SELECT * FROM workflow_journal WHERE runId = ${waiting.id}`).length,
    0
  );

  // A function can catch a step that gave up.
  call('start_caught', JSON.stringify('caught'));
  const caught = await poll('the caught run to complete', () => {
    const latest = latestRun();
    return latest.id > waiting.id && latest.status === 'Completed'
      ? latest
      : undefined;
  });
  assert.deepEqual(
    stepLog(caught.id).map(row => [row.step, row.note]),
    [['compensate', 'boom']]
  );

  // A step that gave up fails the run if nothing catches it.
  call('start_uncaught', JSON.stringify('uncaught'));
  const failedRun = await poll('the uncaught run to fail', () => {
    const latest = latestRun();
    return latest.id > caught.id && latest.status === 'Failed'
      ? latest
      : undefined;
  });
  assert.equal(failedRun.error, 'boom');

  // Parallel items run at the same time, and their results come back in order.
  call('start_overlap', JSON.stringify('overlap'));
  const overlap = await poll('the parallel run to complete', () => {
    const latest = latestRun();
    return latest.id > failedRun.id && latest.status === 'Completed'
      ? latest
      : undefined;
  });
  assert.deepEqual(
    stepLog(overlap.id).find(row => row.step === 'after')?.note,
    '0,1,2'
  );

  // Cancelling stops a parked run.
  call('start_parked', JSON.stringify('parked'));
  const parked = await poll('the parked run to wait', () => {
    const latest = latestRun();
    return latest.status === 'Waiting' && latest.id > overlap.id
      ? latest
      : undefined;
  });
  call('cancel_workflow_run', String(parked.id));
  await pollRun(parked.id, 'Cancelled');
  assert.equal(sql('SELECT * FROM workflow_fire').length, 0);

  // The first attempt is still running when the host is killed. The restarted
  // host retries it and the run finishes.
  call('start_crash', JSON.stringify('crash'));
  const crashing = await poll('the crash step to start', () => {
    const latest = latestRun();
    if (latest.id <= parked.id) return undefined;
    return stepLog(latest.id).length > 0 ? latest : undefined;
  });
  await stopServer(server);
  // The killed host's database lock takes a moment to release.
  await wait(5_000);
  server = startServer();
  await waitForServer(server);
  await pollRun(crashing.id, 'Completed');
  const attempts = stepLog(crashing.id);
  assert.ok(attempts.length >= 2, 'the step must run again after the restart');
  assert.equal(new Set(attempts.map(row => row.note)).size, 1);
  const lost = sql(
    `SELECT step, error FROM workflow_history WHERE runId = ${crashing.id}`
  ).filter(([, error]) => error[0] === 0 && error[1] === 'workflow.step_lost');
  assert.ok(lost.length >= 1, 'the killed attempt must be recorded as lost');

  console.log('workflow recovery test passed');
} catch (error) {
  failed = true;
  console.error(error);
  for (const logs of serverLogs) console.error(logs());
} finally {
  await stopServer(server).catch(error => {
    failed = true;
    console.error(error);
  });
  await rm(dataDir, { recursive: true, force: true }).catch(() => {});
}
process.exit(failed ? 1 : 0);
