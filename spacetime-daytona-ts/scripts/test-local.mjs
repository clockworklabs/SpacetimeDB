import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as wait } from 'node:timers/promises';

const cli = process.env.SPACETIME_BIN ?? 'spacetime';
const listener = net.createServer();
await new Promise(resolve => listener.listen(0, '127.0.0.1', resolve));
const port = listener.address().port;
await new Promise(resolve => listener.close(resolve));
const server = `http://127.0.0.1:${port}`;
const dataDir = await mkdtemp(path.join(os.tmpdir(), 'daytona-test-'));
const database = `daytona-test-${Date.now()}`;
let host;

async function until(description, check) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (await check()) return;
    await wait(100);
  }
  throw new Error(`Timed out: ${description}`);
}
async function startHost() {
  host = spawn(
    cli,
    [
      'start',
      '--listen-addr',
      `127.0.0.1:${port}`,
      '--data-dir',
      dataDir,
      '--non-interactive',
    ],
    { stdio: 'inherit' }
  );
  await once(host, 'spawn');
  await until(
    'server startup',
    () =>
      new Promise(resolve => {
        assert.equal(host.exitCode, null, 'test server exited');
        const socket = net.createConnection({ host: '127.0.0.1', port });
        socket.once('connect', () => {
          socket.destroy();
          resolve(true);
        });
        socket.once('error', () => resolve(false));
      })
  );
}
async function stopHost() {
  if (!host?.pid || host.exitCode !== null || host.signalCode !== null) return;
  const stopped = once(host, 'exit');
  if (process.platform === 'win32') {
    const result = spawnSync('taskkill', [
      '/PID',
      String(host.pid),
      '/T',
      '/F',
    ]);
    assert.equal(result.status, 0, 'failed to stop owned test server');
  } else host.kill('SIGKILL');
  await stopped;
  host = undefined;
}
function backgroundPoll() {
  const child = spawn(cli, ['call', '--server', server, database, 'poll'], {
    stdio: 'ignore',
    timeout: 30_000,
    killSignal: 'SIGKILL',
  });
  return once(child, 'exit');
}
function run(args, fail = false) {
  const result = spawnSync(cli, args, {
    encoding: 'utf8',
    cwd: new URL('..', import.meta.url),
    timeout: 60_000,
  });
  assert.ifError(result.error);
  if (fail) assert.notEqual(result.status, 0, 'Expected rejection');
  else assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
function call(name, args = [], fail = false) {
  return run(
    ['call', '--server', server, database, name, ...args.map(JSON.stringify)],
    fail
  );
}
function rows(query) {
  return JSON.parse(
    run(['sql', '--server', server, '--format', 'json', database, query])
  )[0].rows;
}
function poll(n = 1) {
  for (let i = 0; i < n; i++) call('poll');
}

let published = false;
try {
  await startHost();
  run([
    'publish',
    '--server',
    server,
    '--yes',
    '--module-path',
    'scripts/fixture',
    database,
  ]);
  published = true;
  call('create', ['one']);
  call('create', ['one']);
  assert.equal(
    rows('SELECT * FROM daytona.sandbox').length,
    1,
    'create request deduplicates'
  );
  poll();
  call('create', ['over-quota'], true);
  assert.equal(
    rows('SELECT creates FROM remote')[0][0],
    1,
    'one provider create'
  );
  call('run', ['first', 'exit 0']);
  call('run', ['first', 'exit 0']);
  call('run', ['first', 'exit 1'], true);
  call('run', ['second', 'exit 0'], true);
  call('other_owner', [], true);
  call('reconcile', [], true);
  poll(2);
  assert.equal(rows('SELECT exitCode FROM daytona.execution')[0][0][1], 0);
  assert.equal(rows('SELECT submits FROM remote')[0][0], 1);

  call('mode', ['lost_reply']);
  call('run', ['lost', 'exit 7']);
  poll(3);
  assert.equal(
    rows('SELECT submits FROM remote')[0][0],
    2,
    'lost response does not resubmit'
  );
  assert.equal(
    rows(
      "SELECT exitCode FROM daytona.execution WHERE command = 'exit 7'"
    )[0][0][1],
    7
  );

  call('mode', ['before_send']);
  call('run', ['uncertain', 'exit 9']);
  poll(3);
  assert.equal(
    rows('SELECT submits FROM remote')[0][0],
    2,
    'unknown request is not retried'
  );
  call('run', ['blocked', 'exit 0'], true);
  call('remove');
  poll(3);
  assert.equal(rows('SELECT removed FROM remote')[0][0], true);
  call('mode', ['lost_create']);
  call('create', ['lost-create']);
  poll(2);
  assert.equal(
    rows('SELECT creates FROM remote')[0][0],
    2,
    'lost create response does not create twice'
  );
  call('run', ['recovered-create', 'exit 0']);
  poll(2);
  call('mode', ['lost_delete']);
  call('remove');
  poll(2);
  call('mode', ['late_delete']);
  call('create', ['late-delete']);
  poll(3);
  assert.equal(
    rows('SELECT removed FROM remote')[0][0],
    true,
    'late create is deleted after cancellation'
  );
  assert.equal(rows('SELECT creates FROM remote')[0][0], 3);

  call('mode', ['normal']);
  call('create', ['overlap']);
  poll();
  const beforeOverlap = rows('SELECT submits FROM remote')[0][0];
  call('mode', ['hold_reply']);
  call('run', ['overlap', 'exit 0']);
  const firstWorker = backgroundPoll();
  await until(
    'first worker accepted command',
    () => rows('SELECT submits FROM remote')[0][0] === beforeOverlap + 1
  );
  // poll advances the lease, so a second worker takes over while the first
  // response remains blocked. It must recover, not submit the command again.
  poll();
  call('mode', ['normal']);
  assert.equal((await firstWorker)[0], 0);
  assert.equal(rows('SELECT submits FROM remote')[0][0], beforeOverlap + 1);
  const latest = () =>
    rows('SELECT id, state, exitCode FROM daytona.execution')
      .reduce((latest, row) =>
        BigInt(row[0]) > BigInt(latest[0]) ? row : latest
      )
      .slice(1);
  assert.equal(latest()[0][0], 3, 'stale worker must not overwrite Succeeded');

  const beforeRestart = rows('SELECT submits FROM remote')[0][0];
  call('mode', ['hold_reply']);
  call('run', ['restart', 'exit 7']);
  const interruptedWorker = backgroundPoll();
  await until(
    'command accepted before restart',
    () => rows('SELECT submits FROM remote')[0][0] === beforeRestart + 1
  );
  await stopHost();
  assert.notEqual((await interruptedWorker)[0], 0);
  await startHost();
  assert.equal(latest()[0][0], 1, 'Submitting must survive the host crash');
  call('mode', ['normal']);
  poll(2);
  assert.equal(rows('SELECT submits FROM remote')[0][0], beforeRestart + 1);
  assert.equal(latest()[0][0], 4, 'recover the nonzero command outcome');
  assert.equal(latest()[1][1], 7);
  call('remove');
  poll(2);
  assert.equal(rows('SELECT removed FROM remote')[0][0], true);
  console.log(
    JSON.stringify({
      database,
      server,
      result: 'passed',
      checks: [
        'deduplication',
        'ownership',
        'scheduler guard',
        'one active command',
        'zero/nonzero exits',
        'lost response recovery',
        'unknown outcome',
        'deletion',
        'sandbox quota',
        'lost create and delete responses',
        'late create after deletion request',
        'overlapping workers and stale response',
        'host crash after command acceptance',
      ],
    })
  );
} catch (error) {
  if (published && host) {
    console.error(
      JSON.stringify({
        sandboxes: rows('SELECT * FROM daytona.sandbox'),
        remote: rows('SELECT * FROM remote'),
      })
    );
    console.error(run(['logs', '--server', server, database, '-n', '20']));
  }
  throw error;
} finally {
  await stopHost();
  await rm(dataDir, { recursive: true, force: true, maxRetries: 5 });
  console.log(
    'Cleanup: isolated test server and data removed; no paid sandbox created.'
  );
}
