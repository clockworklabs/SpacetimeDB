import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  createBackendLease,
  acquireResourceLock,
  newRunId,
  publicBackendLease,
  readBackendLease,
  releaseResourceLocks,
  updateBackendLease,
  writeBackendLease,
} from '../src/runtime/backend-lease.mjs';
import { releaseBackendLease } from '../src/runtime/backend-teardown.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-lease-'));
  const path = join(root, 'lease.json');
  const lease = createBackendLease({
    runId: 'chat-spacetime-run0-test', backend: 'spacetime', track: 'chat', runIndex: 0,
    serverUri: 'http://127.0.0.1:3210', module: 'stackbench-run0', dataDir: join(root, 'data'),
  });
  return { root, path, lease };
}

test('run ids include track, backend, index, timestamp, and a nonce', () => {
  const id = newRunId({ track: 'Chat', backend: 'Spacetime', runIndex: 2,
    now: new Date('2026-08-10T12:34:56.000Z'), nonce: 'ABCDEF12-rest' });
  assert.equal(id, 'chat-spacetime-run2-20260810123456-abcdef12');
});

test('lease reads require the matching token, backend, and active state', () => {
  const f = fixture();
  try {
    f.lease.state = 'active';
    writeBackendLease(f.path, f.lease);
    assert.equal(readBackendLease(f.path, {
      token: f.lease.ownershipToken, backend: 'spacetime', active: true,
    }).runId, f.lease.runId);
    assert.throws(() => readBackendLease(f.path, { token: 'wrong' }), /token does not match/);
    assert.throws(() => readBackendLease(f.path, { backend: 'postgres' }), /not postgres/);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('lease updates are atomic and retain identity', () => {
  const f = fixture();
  try {
    writeBackendLease(f.path, f.lease);
    updateBackendLease(f.path, { token: f.lease.ownershipToken, runId: f.lease.runId }, next => {
      next.state = 'active';
      next.resources.listenerPids = [8080];
      return next;
    });
    const onDisk = JSON.parse(readFileSync(f.path, 'utf8'));
    assert.equal(onDisk.runId, f.lease.runId);
    assert.deepEqual(onDisk.resources.listenerPids, [8080]);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('lease ownership tokens are stored with private filesystem modes', t => {
  if (process.platform === 'win32') return t.skip('POSIX modes are not enforced on Windows');
  const f = fixture();
  try {
    writeBackendLease(f.path, f.lease);
    assert.equal(statSync(f.root).mode & 0o777, 0o700);
    assert.equal(statSync(f.path).mode & 0o777, 0o600);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('leases reject unknown lifecycle states and malformed process identities', () => {
  const f = fixture();
  f.lease.state = 'guessing';
  assert.throws(() => writeBackendLease(f.path, f.lease), /unknown state guessing/);
  f.lease.state = 'starting';
  f.lease.resources.launchedPid = -1;
  assert.throws(() => writeBackendLease(f.path, f.lease), /launchedPid must be a positive integer/);
  f.lease.resources.launchedPid = null;
  f.lease.resources.listenerPids = ['not-a-pid'];
  assert.throws(() => writeBackendLease(f.path, f.lease), /listenerPids must contain only positive/);
  f.lease.resources.listenerPids = [];
  f.lease.resources.buildContainer = { name: 'build', id: 'container-id', image: 'image-id',
    owned: true, networkMode: 'ambient' };
  assert.throws(() => writeBackendLease(f.path, f.lease), /buildContainer.networkMode is invalid/);
});

test('public lease evidence hashes rather than exposes the ownership token', () => {
  const f = fixture();
  try {
    const publicLease = publicBackendLease(f.lease);
    assert.equal(publicLease.ownershipToken, undefined);
    assert.match(publicLease.ownership.markerSha256, /^[0-9a-f]{64}$/);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('Spacetime leases reject non-loopback and portless targets', () => {
  const base = { runId: 'unsafe', backend: 'spacetime', track: 'chat', runIndex: 0,
    module: 'stackbench-run0', dataDir: tmpdir() };
  assert.throws(() => createBackendLease({ ...base, serverUri: 'https://production.example:443' }),
    /must use http/);
  assert.throws(() => createBackendLease({ ...base, serverUri: 'http://localhost' }),
    /explicit loopback port/);
});

test('supervisor teardown releases an owned lease without runtime processes', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-supervisor-release-'));
  const path = join(root, 'lease.json');
  try {
    const lease = createBackendLease({
      runId: 'chat-postgres-run0-supervisor', backend: 'postgres', track: 'chat', runIndex: 0,
      database: 'stackbench_supervisor', container: { name: 'unused-postgres', id: 'unused-id' },
    });
    lease.state = 'active';
    writeBackendLease(path, lease);
    assert.equal(releaseBackendLease(path, lease.ownershipToken), true);
    const released = readBackendLease(path, { token: lease.ownershipToken });
    assert.equal(released.state, 'released');
    assert(released.releasedAt);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('pre-activation Spacetime cleanup releases only when its leased port stayed empty', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-created-release-'));
  const emptyPath = join(root, 'empty.json');
  const occupiedPath = join(root, 'occupied.json');
  const emptyServer = createServer();
  await new Promise((done, fail) => emptyServer.listen(0, '127.0.0.1', done).once('error', fail));
  const emptyPort = emptyServer.address().port;
  await new Promise(done => emptyServer.close(done));
  const occupied = createServer((_request, response) => response.end('foreign'));
  await new Promise((done, fail) => occupied.listen(0, '127.0.0.1', done).once('error', fail));
  try {
    const empty = createBackendLease({ runId: 'created-empty', backend: 'spacetime', track: 'chat',
      runIndex: 0, serverUri: `http://127.0.0.1:${emptyPort}`, module: 'created-empty',
      dataDir: join(root, 'empty-data') });
    writeBackendLease(emptyPath, empty);
    assert.equal(releaseBackendLease(emptyPath, empty.ownershipToken), true);
    assert.equal(readBackendLease(emptyPath, { token: empty.ownershipToken }).state, 'released');

    const blocked = createBackendLease({ runId: 'created-occupied', backend: 'spacetime', track: 'chat',
      runIndex: 1, serverUri: `http://127.0.0.1:${occupied.address().port}`,
      module: 'created-occupied', dataDir: join(root, 'occupied-data') });
    writeBackendLease(occupiedPath, blocked);
    assert.equal(releaseBackendLease(occupiedPath, blocked.ownershipToken), false);
    assert.equal(readBackendLease(occupiedPath, { token: blocked.ownershipToken }).state, 'created');
    assert.equal((await fetch(`http://127.0.0.1:${occupied.address().port}`)).status, 200);
  } finally {
    occupied.closeAllConnections();
    await new Promise(done => occupied.close(done));
    rmSync(root, { recursive: true, force: true });
  }
});

test('starting Spacetime cleanup requires and stops the exact launched process', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-starting-release-'));
  const path = join(root, 'lease.json');
  const ambiguousPath = join(root, 'ambiguous.json');
  const first = createServer();
  await new Promise((done, fail) => first.listen(0, '127.0.0.1', done).once('error', fail));
  const ownedPort = first.address().port;
  await new Promise(done => first.close(done));
  const second = createServer();
  await new Promise((done, fail) => second.listen(0, '127.0.0.1', done).once('error', fail));
  const ambiguousPort = second.address().port;
  await new Promise(done => second.close(done));
  const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'],
    { detached: true, stdio: 'ignore' });
  child.unref();
  try {
    const lease = createBackendLease({ runId: 'starting-owned', backend: 'spacetime', track: 'chat',
      runIndex: 0, serverUri: `http://127.0.0.1:${ownedPort}`, module: 'starting-owned',
      dataDir: join(root, 'owned-data') });
    lease.state = 'starting';
    lease.resources.launchedPid = child.pid;
    writeBackendLease(path, lease);
    assert.equal(releaseBackendLease(path, lease.ownershipToken), true);
    assert.equal(readBackendLease(path, { token: lease.ownershipToken }).state, 'released');

    const ambiguous = createBackendLease({ runId: 'starting-ambiguous', backend: 'spacetime',
      track: 'chat', runIndex: 1, serverUri: `http://127.0.0.1:${ambiguousPort}`,
      module: 'starting-ambiguous', dataDir: join(root, 'ambiguous-data') });
    ambiguous.state = 'starting';
    writeBackendLease(ambiguousPath, ambiguous);
    assert.equal(releaseBackendLease(ambiguousPath, ambiguous.ownershipToken), false);
    assert.equal(readBackendLease(ambiguousPath, { token: ambiguous.ownershipToken }).state, 'starting');
  } finally {
    try { child.kill('SIGKILL'); } catch { /* already stopped by lease teardown */ }
    rmSync(root, { recursive: true, force: true });
  }
});

test('listener operations refuse a process not captured by the lease', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-listener-'));
  const path = join(root, 'lease.json');
  const server = createServer((_req, res) => res.end('ok'));
  await new Promise((resolve, reject) => server.listen(0, '127.0.0.1', resolve).once('error', reject));
  try {
    const port = server.address().port;
    const lease = createBackendLease({
      runId: 'listener-refusal', backend: 'spacetime', track: 'chat', runIndex: 0,
      serverUri: `http://127.0.0.1:${port}`, module: 'stackbench-run0', dataDir: join(root, 'data'),
    });
    lease.state = 'active';
    lease.resources.listenerPids = [process.pid + 1000];
    writeBackendLease(path, lease);
    assert.throws(() => execFileSync(process.execPath,
      [join(HERE, '..', 'commands', 'lease-cli.mjs'), 'listener-pid', 'spacetime'], {
        env: { ...process.env, STACK_BENCH_LEASE: path,
          STACK_BENCH_LEASE_TOKEN: lease.ownershipToken },
        stdio: 'pipe',
      }), /Command failed/);
  } finally {
    await new Promise(resolve => server.close(resolve));
    rmSync(root, { recursive: true, force: true });
  }
});

test('resource locks exclude a concurrent run and release only for their owner', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-lock-'));
  try {
    const first = createBackendLease({ runId: 'first', backend: 'stub', track: 'loop', runIndex: 0 });
    const second = createBackendLease({ runId: 'second', backend: 'stub', track: 'loop', runIndex: 0 });
    first.resources.locks.push(acquireResourceLock({ root, key: 'slot:loop:stub:run0', lease: first }));
    assert.throws(() => acquireResourceLock({ root, key: 'slot:loop:stub:run0', lease: second }),
      /already leased by first/);
    releaseResourceLocks(first);
    second.resources.locks.push(acquireResourceLock({ root, key: 'slot:loop:stub:run0', lease: second }));
    releaseResourceLocks(second);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('resource acquisition reclaims a lock whose owner process is gone', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-lock-'));
  try {
    const stale = createBackendLease({ runId: 'stale', backend: 'stub', track: 'loop', runIndex: 0,
      ownerPid: 2_147_483_646 });
    stale.resources.locks.push(acquireResourceLock({ root, key: 'shared-listener', lease: stale }));
    const current = createBackendLease({ runId: 'current', backend: 'stub', track: 'loop', runIndex: 1 });
    current.resources.locks.push(acquireResourceLock({ root, key: 'shared-listener', lease: current }));
    releaseResourceLocks(current);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
