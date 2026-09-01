import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import type { Server } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { compiledEntrypoint } from '../src/package-root.js';
import {
  createBackendLease,
  acquireResourceLock,
  acquireResourceLocks,
  backendResourceLockKeys,
  newRunId,
  publicBackendLease,
  readBackendLease,
  releaseResourceLocks,
  resourceLockScope,
  updateBackendLease,
  writeBackendLease,
} from '../src/runtime/backend-lease.js';
import { releaseBackendLease, stopLeasedContainer } from '../src/runtime/backend-teardown.js';
import { processIdentity } from '../src/runtime/platform.js';

async function listen(server: Server): Promise<number> {
  await new Promise<void>((resolve, reject) =>
    server.listen(0, '127.0.0.1', resolve).once('error', reject));
  const address = server.address();
  assert(address && typeof address !== 'string');
  return address.port;
}

async function close(server: Server): Promise<void> {
  await new Promise<void>((resolve, reject) =>
    server.close(error => error ? reject(error) : resolve()));
}

function processStderr(error: unknown): string {
  if (error === null || typeof error !== 'object' || !('stderr' in error)) return '';
  return String(error.stderr ?? '');
}

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-lease-'));
  const path = join(root, 'lease.json');
  const lease = createBackendLease({
    runId: 'chat-spacetime-run0-test', backend: 'spacetime', track: 'chat', runIndex: 0,
    serverUri: 'http://127.0.0.1:3210', module: 'app-run0', dataDir: join(root, 'data'),
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
      next.resources.listenerProcesses = [{ pid: 8080, startMarker: '1' }];
      return next;
    });
    const onDisk = JSON.parse(readFileSync(f.path, 'utf8'));
    assert.equal(onDisk.runId, f.lease.runId);
    assert.deepEqual(onDisk.resources.listenerProcesses, [{ pid: 8080, startMarker: '1' }]);
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
  assert.throws(() => writeBackendLease(f.path, { ...f.lease, state: 'guessing' }),
    /unknown state guessing/);
  assert.throws(() => writeBackendLease(f.path, { ...f.lease, state: 'starting',
    resources: { ...f.lease.resources, launchedProcess: { pid: -1, startMarker: '1' } } }),
  /launchedProcess must be a process identity/);
  assert.throws(() => writeBackendLease(f.path, { ...f.lease,
    resources: { ...f.lease.resources, listenerProcesses: [{ pid: 1, startMarker: 'bad' }] } }),
  /listenerProcesses must contain only process identities/);
  const invalidContainer = { name: 'build', id: 'container-id', image: 'image-id',
    owned: true, networkMode: 'ambient' };
  assert.throws(() => writeBackendLease(f.path, { ...f.lease,
    resources: { ...f.lease.resources, buildContainer: invalidContainer } }),
  /buildContainer.networkMode is invalid/);
  const missingLimits = { ...invalidContainer, networkMode: 'bridge' };
  assert.throws(() => writeBackendLease(f.path, { ...f.lease,
    resources: { ...f.lease.resources, buildContainer: missingLimits } }),
    /buildContainer.resourceLimits is invalid/);
  const invalidLimits = { ...missingLimits, resourceLimits: {
    cpuCount: 2, memoryBytes: 4096, memorySwapBytes: 2048, pids: 512 } };
  assert.throws(() => writeBackendLease(f.path, { ...f.lease,
    resources: { ...f.lease.resources, buildContainer: invalidLimits } }),
    /buildContainer.resourceLimits is invalid/);
});

test('public lease evidence hashes rather than exposes the ownership token', () => {
  const f = fixture();
  try {
    f.lease.resources.buildContainer = { name: 'build', id: 'container-id', image: 'image-id',
      owned: true, networkMode: 'bridge', resourceLimits: {
        cpuCount: 2, memoryBytes: 4096, memorySwapBytes: 4096, pids: 512,
    } };
    const publicLease = publicBackendLease(f.lease);
    assert.equal('ownershipToken' in publicLease, false);
    assert.match(publicLease.ownership.markerSha256, /^[0-9a-f]{64}$/);
    assert(publicLease.resources.buildContainer);
    assert.deepEqual(publicLease.resources.buildContainer.resourceLimits,
      { cpuCount: 2, memoryBytes: 4096, memorySwapBytes: 4096, pids: 512 });
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('Spacetime leases reject non-loopback and portless targets', () => {
  const base = { runId: 'unsafe', backend: 'spacetime', track: 'chat', runIndex: 0,
    module: 'app-run0', dataDir: tmpdir() };
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
      database: 'app_supervisor', container: { name: 'unused-postgres', id: 'unused-id' },
    });
    lease.state = 'active';
    writeBackendLease(path, lease);
    assert.equal(releaseBackendLease(path, lease.ownershipToken), true);
    const released = readBackendLease(path, { token: lease.ownershipToken });
    assert.equal(released.state, 'released');
    assert(released.releasedAt);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('container teardown retries transient Docker removal failures', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-container-retry-'));
  const path = join(root, 'lease.json');
  try {
    const lease = createBackendLease({
      runId: 'container-retry', backend: 'postgres', track: 'chat', runIndex: 0,
      database: 'container_retry', container: { name: 'owned-container', id: 'owned-id' },
    });
    lease.state = 'active';
    lease.resources.buildContainer = {
      name: 'owned-build', id: 'owned-build-id', image: 'build-image', owned: true,
      running: true, networkMode: 'bridge',
      resourceLimits: { cpuCount: 2, memoryBytes: 4096, memorySwapBytes: 4096, pids: 512 },
    };
    writeBackendLease(path, lease);
    let attempts = 0;
    const delays: number[] = [];
    assert.equal(stopLeasedContainer(path, lease.ownershipToken, {
      inspect: () => 'owned-build-id',
      remove: () => {
        attempts += 1;
        if (attempts < 3) throw new Error('Docker daemon is temporarily unavailable');
      },
      wait: delay => delays.push(delay),
    }), true);
    assert.equal(attempts, 3);
    assert.deepEqual(delays, [0, 250, 750]);
    assert.equal(readBackendLease(path, { token: lease.ownershipToken })
      .resources.buildContainer?.running, false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('container teardown accepts an exact container removed after inspection', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-container-missing-'));
  const path = join(root, 'lease.json');
  try {
    const lease = createBackendLease({
      runId: 'container-missing', backend: 'postgres', track: 'chat', runIndex: 0,
      database: 'container_missing', container: { name: 'owned-container', id: 'owned-id' },
    });
    lease.state = 'active';
    lease.resources.buildContainer = {
      name: 'owned-build', id: 'owned-build-id', image: 'build-image', owned: true,
      running: true, networkMode: 'bridge',
      resourceLimits: { cpuCount: 2, memoryBytes: 4096, memorySwapBytes: 4096, pids: 512 },
    };
    writeBackendLease(path, lease);
    assert.equal(stopLeasedContainer(path, lease.ownershipToken, {
      inspect: () => 'owned-build-id',
      remove: () => { throw new Error('No such container: owned-build-id'); },
      wait: () => undefined,
    }), true);
    assert.equal(readBackendLease(path, { token: lease.ownershipToken })
      .resources.buildContainer?.running, false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('pre-activation Spacetime cleanup releases only when its leased port stayed empty', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-created-release-'));
  const emptyPath = join(root, 'empty.json');
  const occupiedPath = join(root, 'occupied.json');
  const emptyServer = createServer();
  const emptyPort = await listen(emptyServer);
  await close(emptyServer);
  const occupied = createServer((_request, response) => response.end('foreign'));
  const occupiedPort = await listen(occupied);
  try {
    const empty = createBackendLease({ runId: 'created-empty', backend: 'spacetime', track: 'chat',
      runIndex: 0, serverUri: `http://127.0.0.1:${emptyPort}`, module: 'created-empty',
      dataDir: join(root, 'empty-data') });
    writeBackendLease(emptyPath, empty);
    assert.equal(releaseBackendLease(emptyPath, empty.ownershipToken), true);
    assert.equal(readBackendLease(emptyPath, { token: empty.ownershipToken }).state, 'released');

    const blocked = createBackendLease({ runId: 'created-occupied', backend: 'spacetime', track: 'chat',
      runIndex: 1, serverUri: `http://127.0.0.1:${occupiedPort}`,
      module: 'created-occupied', dataDir: join(root, 'occupied-data') });
    writeBackendLease(occupiedPath, blocked);
    assert.equal(releaseBackendLease(occupiedPath, blocked.ownershipToken), false);
    assert.equal(readBackendLease(occupiedPath, { token: blocked.ownershipToken }).state, 'created');
    assert.equal((await fetch(`http://127.0.0.1:${occupiedPort}`)).status, 200);
  } finally {
    occupied.closeAllConnections();
    await close(occupied);
    rmSync(root, { recursive: true, force: true });
  }
});

test('starting Spacetime cleanup requires and stops the exact launched process', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-starting-release-'));
  const path = join(root, 'lease.json');
  const ambiguousPath = join(root, 'ambiguous.json');
  const first = createServer();
  const ownedPort = await listen(first);
  await close(first);
  const second = createServer();
  const ambiguousPort = await listen(second);
  await close(second);
  const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'],
    { detached: true, stdio: 'ignore' });
  child.unref();
  try {
    const lease = createBackendLease({ runId: 'starting-owned', backend: 'spacetime', track: 'chat',
      runIndex: 0, serverUri: `http://127.0.0.1:${ownedPort}`, module: 'starting-owned',
      dataDir: join(root, 'owned-data') });
    lease.state = 'starting';
    lease.resources.launchedProcess = child.pid ? processIdentity(child.pid) : null;
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

test('resource lock release preserves a replacement owner record', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-lock-release-'));
  try {
    const first = createBackendLease({ runId: 'first', backend: 'stub', track: 'loop', runIndex: 0 });
    const lock = acquireResourceLock({ root, key: 'shared', lease: first });
    first.resources.locks.push(lock);
    writeFileSync(lock.path, JSON.stringify({ runId: 'replacement', ownerPid: process.pid,
      ownerStartMarker: processIdentity(process.pid)?.startMarker ?? null,
      ownershipMarkerSha256: 'replacement' }));
    assert.throws(() => releaseResourceLocks(first), /no longer belongs/);
    assert.equal(existsSync(lock.path), true);
    assert.equal(JSON.parse(readFileSync(lock.path, 'utf8')).runId, 'replacement');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('bench and reference leases use the same canonical slot and backend keys', () => {
  const input = { backend: 'spacetime', track: 'ecommerce', runIndex: 0,
    serverUri: 'http://127.0.0.1:3310', module: 'app-ecom-run0',
    dataDir: tmpdir() };
  const bench = createBackendLease({ ...input, runId: 'bench' });
  const reference = createBackendLease({ ...input, runId: 'reference' });
  const preparedKeys = ['listener:http://127.0.0.1:3310'];
  const expected = [
    'listener:http://127.0.0.1:3310',
    'slot:ecommerce:spacetime:run0',
  ];
  assert.deepEqual(backendResourceLockKeys(bench, preparedKeys), expected);
  assert.deepEqual(backendResourceLockKeys(reference, preparedKeys), expected);
});

test('multi-lock acquisition rolls back earlier keys when a later key is busy', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-lock-rollback-'));
  const held = createBackendLease({ runId: 'held', backend: 'stub', track: 'loop', runIndex: 0 });
  const target = createBackendLease({ runId: 'target', backend: 'stub', track: 'loop', runIndex: 1 });
  const probe = createBackendLease({ runId: 'probe', backend: 'stub', track: 'loop', runIndex: 2 });
  try {
    held.resources.locks.push(acquireResourceLock({ root, key: 'b-held', lease: held }));
    assert.throws(() => acquireResourceLocks({
      root, keys: ['a-rollback', 'b-held'], lease: target,
    }), /already leased by held/);
    probe.resources.locks.push(acquireResourceLock({ root, key: 'a-rollback', lease: probe }));
    releaseResourceLocks(probe);
    releaseResourceLocks(held);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('appliance controllers share locks and require recovery after an owner exits', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-shared-lock-'));
  const helper = compiledEntrypoint('tests', 'fixtures', 'resource-lock-process.js');
  const owner = spawn(process.execPath, [helper, root, 'first-controller', 'hold'], {
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  try {
    await new Promise<void>((resolve, reject) => {
      let stderr = '';
      owner.stderr.on('data', chunk => { stderr += chunk; });
      owner.stdout.once('data', chunk => {
        if (String(chunk).includes('acquired')) resolve();
        else reject(new Error(`lock helper returned unexpected output: ${chunk}`));
      });
      owner.once('error', reject);
      owner.once('exit', code => reject(new Error(`lock helper exited ${code}: ${stderr}`)));
    });
    assert.throws(() => execFileSync(process.execPath,
      [helper, root, 'second-controller'], { encoding: 'utf8', stdio: 'pipe' }),
    error => /already leased by first-controller/.test(processStderr(error)));

    owner.kill('SIGTERM');
    await new Promise<void>(resolve => owner.once('exit', () => resolve()));
    assert.throws(() => execFileSync(process.execPath,
      [helper, root, 'second-controller'], { encoding: 'utf8', stdio: 'pipe' }),
    error => /remains leased by first-controller; run authenticated recovery/
      .test(processStderr(error)));
  } finally {
    if (owner.exitCode === null) owner.kill('SIGKILL');
    rmSync(root, { recursive: true, force: true });
  }
});

test('resource lock scope is shared only for the appliance', () => {
  const temporaryDirectory = join(tmpdir(), 'scope-test');
  assert.deepEqual(resourceLockScope({}, { temporaryDirectory }), {
    root: join(temporaryDirectory, 'stack-bench-resource-locks'), reclaimStale: true,
  });
  assert.deepEqual(resourceLockScope({ STACK_BENCH_APPLIANCE: '1' }), {
    root: '/var/lib/stack-bench/controller-home/resource-locks', reclaimStale: false,
  });
  assert.throws(() => resourceLockScope({ STACK_BENCH_RESOURCE_LOCK_DIR: 'relative' }),
    /must be an absolute path/);
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

test('resource acquisition rejects PID reuse as lock ownership', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-lock-pid-reuse-'));
  const key = 'shared-listener';
  const digest = createHash('sha256').update(key).digest('hex');
  const path = join(root, `${digest}.lock.json`);
  try {
    writeFileSync(path, JSON.stringify({ version: 1, key, runId: 'dead-owner',
      ownerPid: process.pid, ownerStartMarker: 'not-this-process',
      ownershipMarkerSha256: 'dead-owner', acquiredAt: new Date().toISOString() }));
    const current = createBackendLease({ runId: 'current', backend: 'stub', track: 'loop', runIndex: 1 });
    current.resources.locks.push(acquireResourceLock({ root, key, lease: current }));
    releaseResourceLocks(current);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
