import { activateHosted } from '../src/stacks/hosted-lifecycle.js';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import type { Server } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { runBounded } from '../src/runtime/bounded-process.js';
import {
  CAPACITY_WAIT_RECEIPT_ENV,
  readCapacityWait,
  createBackendLease,
  acquireResourceLocks,
  backendResourceLockKeys,
  publicBackendLease,
  readBackendLease,
  resourceLockScope,
  updateBackendLease,
  writeBackendLease,
} from '../src/runtime/backend-lease.js';
import { dockerNetworkMissing, handoffBuildWorkspace, releaseBackendLease, stopLeasedContainer } from '../src/runtime/backend-teardown.js';
import { ATTEMPT_CREATION_LABEL } from '../src/runtime/container-identity.js';

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

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-lease-'));
  const path = join(root, 'lease.json');
  const lease = createBackendLease({
    runId: 'chat-spacetime-run0-test', backend: 'spacetime', track: 'chat', runIndex: 0,
    serverUri: 'http://127.0.0.1:3210', module: 'app-run0', dataDir: join(root, 'data'),
  });
  return { root, path, lease };
}

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
    f.lease.campaignDelegation = { path: '/private/delegation.json', token: 'private-child-token' };
    const publicLease = publicBackendLease(f.lease);
    assert.equal('ownershipToken' in publicLease, false);
    assert.equal('campaignDelegation' in publicLease, false);
    assert.match(publicLease.ownership.markerSha256, /^[0-9a-f]{64}$/);
    assert(publicLease.resources.buildContainer);
    assert.deepEqual(publicLease.resources.buildContainer.resourceLimits,
      { cpuCount: 2, memoryBytes: 4096, memorySwapBytes: 4096, pids: 512 });
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('Spacetime and Convex leases reject non-loopback and portless targets', () => {
  for (const base of [
    { runId: 'unsafe', backend: 'spacetime', track: 'chat', runIndex: 0, module: 'app-run0', dataDir: tmpdir() },
    { runId: 'unsafe', backend: 'convex', track: 'ecommerce', runIndex: 0 },
  ]) {
    assert.throws(() => createBackendLease({ ...base, serverUri: 'https://production.example:443' }),
      /must use http/);
    assert.throws(() => createBackendLease({ ...base, serverUri: 'http://localhost' }),
      /explicit loopback port/);
  }
});

test('private Convex lifecycle releases before first creation intent and preserves refusal', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-convex-release-'));
  const path = join(root, 'lease.json');
  const lease = createBackendLease({ runId: 'convex-before-start', backend: 'convex',
    track: 'ecommerce', runIndex: 0, serverUri: 'http://127.0.0.1:14310' });
  try {
    writeBackendLease(path, lease);
    assert.throws(() => releaseBackendLease(path, 'wrong-token'), /ownership token does not match/);
    assert.equal(releaseBackendLease(path, lease.ownershipToken, { hostTeardown: () => false }), false);
    assert.equal(readBackendLease(path).state, 'created');
    assert.equal(releaseBackendLease(path, lease.ownershipToken), true);
    assert.equal(readBackendLease(path).state, 'released');
    assert.equal(releaseBackendLease(path, lease.ownershipToken), true);
    assert.throws(() => createBackendLease({ ...lease, serverUri: 'http://127.0.0.1:14310',
      container: { name: 'ambient', id: 'ambient' } }), /requires an owned container/);
    const sharedPath = join(root, 'shared.json');
    const shared = createBackendLease({
      runId: 'chat-postgres-run0-supervisor', backend: 'postgres', track: 'chat', runIndex: 0,
      database: 'app_supervisor', container: { name: 'unused-postgres', id: 'unused-id' },
    });
    shared.state = 'active';
    writeBackendLease(sharedPath, shared);
    assert.equal(releaseBackendLease(sharedPath, shared.ownershipToken), true);
    const released = readBackendLease(sharedPath, { token: shared.ownershipToken });
    assert.equal(released.state, 'released');
    assert(released.releasedAt);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('private host teardown refusal retains resource locks', { skip: process.platform !== 'linux' }, () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-convex-refused-'));
  const path = join(root, 'lease.json');
  const lease = createBackendLease({ runId: 'convex-refused', backend: 'convex',
    track: 'ecommerce', runIndex: 0, serverUri: 'http://127.0.0.1:14310' });
  try {
    const lock = acquireResourceLocks({ root, keys: ['port:14310'], lease })[0]!;
    lease.resources.locks.push(lock);
    writeBackendLease(path, lease);
    assert.equal(releaseBackendLease(path, lease.ownershipToken, { hostTeardown: () => false }), false);
    assert(existsSync(lock.path));
    assert.equal(readBackendLease(path).resources.locks[0]?.releasedAt, undefined);
    assert.equal(releaseBackendLease(path, lease.ownershipToken), true);
    assert(!existsSync(lock.path));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('build workspace handback stops private-namespace writers before changing permissions', () => {
  const id = 'a'.repeat(64);
  const recorded: string[][] = [];
  handoffBuildWorkspace(id, (_command, args) => {
    recorded.push([...args]);
    return args[0] === 'inspect' ? JSON.stringify({ state: { Status: 'running', Running: true }, pidMode: '' }) : '';
  });
  assert.equal(recorded.length, 4);
  assert.equal(recorded[0]?.at(-1), id);
  assert.deepEqual(recorded[1]?.slice(0, 6), ['exec', '--user', '0:0', id, 'sh', '-ec']);
  assert.match(recorded[1]?.at(-1) ?? '', /pkill -KILL -u 10001/);
  assert.match(recorded[1]?.at(-1) ?? '', /application writers did not stop/);
  assert.deepEqual(recorded.slice(2).map(args => args.slice(4)), [
    ['chown', '-R', `10001:${process.getgid?.() ?? 0}`, '/app'],
    ['chmod', '-R', 'u+rwX,g+rwX,o-rwx', '/app'],
  ]);
  for (const runtime of [
    { state: { Status: 'exited', Running: false }, pidMode: '' },
    { state: { Status: 'running', Running: true }, pidMode: 'host' },
    { state: { Status: 'running', Running: true }, pidMode: `container:${'b'.repeat(64)}` },
  ]) {
    assert.throws(() => handoffBuildWorkspace(id, (_command, args) => {
      assert.equal(args[0], 'inspect', 'unsafe containers must never receive a signal');
      return JSON.stringify(runtime);
    }), /workspace handback requires/);
  }
  handoffBuildWorkspace(id, (_command, args) => {
    assert.equal(args[0], 'inspect');
    return JSON.stringify({ state: { Status: 'created', Running: false }, pidMode: '' });
  });
});

test('network teardown recognizes both Docker missing-network responses without hiding other failures', () => {
  for (const message of [
    'Error response from daemon: No such network: sb-owned-network',
    `Error response from daemon: network ${'a'.repeat(64)} not found`,
  ]) {
    assert(dockerNetworkMissing(new Error(message)));
    assert(dockerNetworkMissing({ stderr: Buffer.from(message) }));
  }
  for (const message of [
    'Cannot connect to the Docker daemon', 'permission denied',
    'network sb-owned-network has active endpoints', 'No such container: owned-container',
  ]) assert.equal(dockerNetworkMissing(new Error(message)), false);
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
    const refused = {
      inspect: () => 'owned-build-id',
      handoffWorkspace: () => { throw new Error('workspace handback failed'); },
      remove: () => assert.fail('a container must remain available until workspace handback succeeds'),
      wait: () => undefined,
    };
    assert.throws(() => stopLeasedContainer(path, lease.ownershipToken, refused), /workspace handback failed/);
    assert.equal(readBackendLease(path, { token: lease.ownershipToken })
      .resources.buildContainer?.workspaceHandedBackAt, undefined);
    assert.throws(() => stopLeasedContainer(path, lease.ownershipToken, {
      ...refused, inspect: () => { throw new Error('No such container: owned-build-id'); },
    }), /disappeared before workspace handback/);
    assert.equal(stopLeasedContainer(path, lease.ownershipToken, {
      ...refused, inspect: () => 'replacement-id',
    }), false);
    assert.equal(readBackendLease(path, { token: lease.ownershipToken }).resources.buildContainer?.running, true);
    let attempts = 0;
    const delays: number[] = [];
    assert.equal(stopLeasedContainer(path, lease.ownershipToken, {
      inspect: () => 'owned-build-id',
      handoffWorkspace: () => assert.equal(attempts, 0),
      remove: () => {
        assert(readBackendLease(path, { token: lease.ownershipToken }).resources.buildContainer?.workspaceHandedBackAt);
        attempts += 1;
        if (attempts < 3) throw new Error('Docker daemon is temporarily unavailable');
      },
      wait: delay => delays.push(delay),
    }), true);
    assert.equal(attempts, 3);
    assert.deepEqual(delays, [0, 250, 750]);
    assert.equal(readBackendLease(path, { token: lease.ownershipToken })
      .resources.buildContainer?.running, false);
    updateBackendLease(path, { token: lease.ownershipToken }, next => {
      delete next.resources.buildContainer!.removedAt;
      next.resources.buildContainer!.running = true;
      return next;
    });
    assert.equal(stopLeasedContainer(path, lease.ownershipToken, {
      ...refused, inspect: () => { throw new Error('No such container: owned-build-id'); },
    }), true, 'handback evidence closes the crash gap between removal and its lease update');
    updateBackendLease(path, { token: lease.ownershipToken }, next => {
      delete next.resources.buildContainer!.removedAt;
      next.resources.buildContainer!.running = true;
      return next;
    });
    assert.equal(stopLeasedContainer(path, lease.ownershipToken, {
      inspect: () => 'owned-build-id',
      handoffWorkspace: () => undefined,
      remove: () => { throw new Error('No such container: owned-build-id'); },
      wait: () => undefined,
    }), true, 'an exact container removed after inspection is already gone');
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

test('bench and reference leases use the same canonical slot and backend keys', () => {
  const input = { backend: 'spacetime', track: 'ecommerce', runIndex: 0,
    serverUri: 'http://127.0.0.1:3310', module: 'app-ecom-run0',
    dataDir: tmpdir() };
  const bench = createBackendLease({ ...input, runId: 'bench' });
  const reference = createBackendLease({ ...input, runId: 'reference' });
  const preparedKeys = ['listener:http://127.0.0.1:3310'];
  const expected = [
    'listener:http://127.0.0.1:3310',
    'port:3310',
    'port:6473',
    'slot:ecommerce:spacetime:run0',
  ];
  assert.deepEqual(backendResourceLockKeys(bench, { vite: 6473, express: null }, preparedKeys), expected);
  assert.deepEqual(backendResourceLockKeys(reference, { vite: 6473, express: null }, preparedKeys), expected);
});

test('resource lock scope is shared only for the appliance', () => {
  const temporaryDirectory = join(tmpdir(), 'scope-test');
  assert.deepEqual(resourceLockScope({}, { temporaryDirectory }), {
    root: join(temporaryDirectory, 'stack-bench-resource-locks'),
  });
  assert.deepEqual(resourceLockScope({ STACK_BENCH_APPLIANCE: '1' }), {
    root: '/var/lib/stack-bench/controller-home/resource-locks',
  });
  assert.throws(() => resourceLockScope({ STACK_BENCH_RESOURCE_LOCK_DIR: 'relative' }),
    /must be an absolute path/);
});

test('a different PID start marker cannot authorize reclamation across controller namespaces', { skip: process.platform !== 'linux' ? 'Kernel flock requires the Linux appliance' : false }, () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-lock-pid-reuse-'));
  const key = 'shared-listener';
  const digest = createHash('sha256').update(key).digest('hex');
  const path = join(root, `${digest}.lock.json`);
  const current = createBackendLease({ runId: 'current', backend: 'stub', track: 'loop', runIndex: 1 });
  try {
    for (const owner of [
      { ownerPid: process.pid, ownerStartMarker: 'not-this-process' },
      { ownerPid: 2_147_483_646, ownerStartMarker: null },
    ]) {
      writeFileSync(path, JSON.stringify({ version: 1, key, runId: 'dead-owner', ...owner,
        ownershipMarkerSha256: 'dead-owner', acquiredAt: new Date().toISOString() }));
      const before = readFileSync(path, 'utf8');
      assert.throws(() => acquireResourceLocks({ root, keys: [key], lease: current }), /run authenticated recovery before reuse/);
      assert.equal(readFileSync(path, 'utf8'), before);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('resource-free stub activation stays resource-free inside the appliance', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-stub-activation-'));
  const previous = process.env.STACK_BENCH_APPLIANCE;
  try {
    process.env.STACK_BENCH_APPLIANCE = '1';
    const lease = createBackendLease({ runId: 'stub-appliance', backend: 'stub', track: 'loop', runIndex: 0 });
    const path = join(root, 'lease.json');
    writeBackendLease(path, lease);
    activateHosted({ leasePath: path, leaseToken: lease.ownershipToken, lease,
      ports: { vite: 1, express: null, dbPort: null } });
    const activated = readBackendLease(path, { token: lease.ownershipToken, active: true });
    assert.equal(activated.resources.container, null);
    assert.equal(activated.resources.network, undefined);
    assert.deepEqual(activated.resources.locks, []);
  } finally {
    if (previous === undefined) delete process.env.STACK_BENCH_APPLIANCE;
    else process.env.STACK_BENCH_APPLIANCE = previous;
    rmSync(root, { recursive: true, force: true });
  }
});

test('a supervised child waiting for host capacity does not spend its timeout', async () => {
  const root = mkdtempSync(join(tmpdir(), 'capacity-wait-'));
  const receipt = join(root, 'capacity-wait.json');
  const lease = new URL('../src/runtime/backend-lease.js', import.meta.url).href;
  const child = `import { writeCapacityWait } from ${JSON.stringify(lease)};
    const startedAt = Date.now();
    writeCapacityWait({ startedAt, resumedAt: null });
    await new Promise(resolve => setTimeout(resolve, 4000));
    writeCapacityWait({ startedAt, resumedAt: Date.now() });
    await new Promise(resolve => setTimeout(resolve, 300));`;
  try {
    const result = await runBounded(process.execPath, ['--input-type=module', '-e', child], {
      stdio: 'ignore', timeoutMs: 2500, env: { ...process.env, [CAPACITY_WAIT_RECEIPT_ENV]: receipt },
      pauseInterval: () => readCapacityWait(receipt), terminate: pid => process.kill(pid, 'SIGKILL'),
    });
    assert.equal(result.error, null);
    assert.equal(result.timedOut, false);
    assert.equal(result.ok, true);
    assert(result.pausedMs! >= 3900, String(result.pausedMs));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

function platformLease(root: string) {
  const anchor = 'a'.repeat(64);
  const lease = createBackendLease({ runId: 'supabase-services', backend: 'supabase', track: 'ecommerce', runIndex: 0,
    serverUri: 'http://127.0.0.1:13410', database: 'postgres' });
  lease.state = 'active';
  lease.resources.container = { name: 'sb-anchor-backend', id: anchor, image: `sha256:${'d'.repeat(64)}`,
    owned: true, networkMode: 'b'.repeat(64) };
  lease.resources.network = { name: 'sb-anchor-network', id: 'b'.repeat(64), namespaceContainerId: anchor,
    hostAddresses: ['172.20.0.1'], services: [], firewallSha256: null, firewallInstalledAt: null };
  const service = (role: string, digit: string) => ({ name: `sb-anchor-service-${role}`, id: digit.repeat(64),
    image: `sha256:${'e'.repeat(64)}`, owned: true, networkMode: `container:${anchor}` });
  lease.resources.serviceContainers = { gateway: service('gateway', '1'), auth: service('auth', '2') };
  lease.resources.browserContainer = { ...service('browser', '3'), name: 'sb-anchor-browser' };
  const intent = (name: string, digit: string) => ({ name, creationToken: digit.repeat(32) });
  lease.resources.creationIntents = { network: intent('sb-anchor-network', '4'), backend: intent('sb-anchor-backend', '5'),
    'service-auth': intent('sb-anchor-service-auth', '6'), 'service-gateway': intent('sb-anchor-service-gateway', '7'),
    'service-realtime': intent('sb-anchor-service-realtime', '8'), browser: intent('sb-anchor-browser', '9') };
  const path = join(root, 'lease.json');
  writeBackendLease(path, lease);
  return { path, lease, anchor };
}

test('platform service containers are owned, exact, and inside the anchor namespace', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-services-'));
  try {
    const { path, lease, anchor } = platformLease(root);
    assert.equal(readBackendLease(path).resources.serviceContainers?.auth?.networkMode, `container:${anchor}`);
    assert.equal(publicBackendLease(lease).resources.serviceContainers?.gateway?.name, 'sb-anchor-service-gateway');
    const invalid = (change: (next: typeof lease) => void, message: RegExp) => {
      const next = structuredClone(lease);
      change(next);
      assert.throws(() => writeBackendLease(join(root, 'invalid.json'), next), message);
    };
    invalid(next => { next.resources.serviceContainers!.auth!.networkMode = `container:${'c'.repeat(64)}`; },
      /serviceContainers.auth is outside the leased network namespace/);
    invalid(next => { next.resources.serviceContainers!.auth!.owned = false; }, /serviceContainers.auth must identify an owned/);
    invalid(next => { next.resources.serviceContainers!.auth!.image = 'supabase/gotrue:latest'; }, /must identify an owned/);
    invalid(next => { next.resources.serviceContainers = { 'Auth-1': next.resources.serviceContainers!.auth! }; },
      /map service roles/);
    invalid(next => { (next.resources.creationIntents as Record<string, unknown>)['service-'] = { name: 'x', creationToken: '1'.repeat(32) }; },
      /creation intent is invalid/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('teardown removes platform services before their namespace anchor, including unrecorded creations', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-services-'));
  try {
    const { path, lease, anchor } = platformLease(root);
    const removed: string[] = [];
    const present = new Set([...Object.values(lease.resources.serviceContainers!).map(container => container.id),
      lease.resources.browserContainer!.id, anchor]);
    const names = new Map([...Object.values(lease.resources.serviceContainers!), lease.resources.browserContainer!,
      lease.resources.container!].map(container => [container.name, container.id]));
    const unrecorded = 'f'.repeat(64);
    const attempt: string[][] = [];
    assert.equal(releaseBackendLease(path, lease.ownershipToken, {
      hostTeardown: () => true,
      docker: {
        inspect: name => {
          const id = names.get(name);
          if (!id || !present.has(id)) throw new Error(`No such object: ${name}`);
          return id;
        },
        handoffWorkspace: () => assert.fail('no build container'),
        remove: id => { removed.push(id); present.delete(id); },
        wait: () => undefined,
      },
      attempt: args => {
        attempt.push(args);
        const name = args.at(-1)!;
        if (args[0] === 'network' && args[1] === 'inspect') {
          return JSON.stringify([{ Id: lease.resources.network!.id, Labels: { [ATTEMPT_CREATION_LABEL]: '4'.repeat(32) }, Containers: {} }]);
        }
        if (args[0] === 'container' && name === 'sb-anchor-service-realtime') {
          return JSON.stringify([{ Id: unrecorded, Config: { Labels: { [ATTEMPT_CREATION_LABEL]: '8'.repeat(32) } } }]);
        }
        if (args[0] === 'container') throw new Error(`No such container: ${name}`);
        return '';
      },
    }), true);
    assert.deepEqual(removed, [lease.resources.browserContainer!.id, lease.resources.serviceContainers!.auth!.id,
      lease.resources.serviceContainers!.gateway!.id, anchor]);
    const order = attempt.filter(args => args[0] === 'container').map(args => args.at(-1));
    assert.deepEqual(order, ['sb-anchor-browser', 'sb-anchor-service-auth', 'sb-anchor-service-gateway',
      'sb-anchor-service-realtime', 'sb-anchor-backend']);
    assert.deepEqual(attempt.filter(args => args[0] === 'rm'), [['rm', '-f', '--volumes', unrecorded]]);
    assert.deepEqual(attempt.at(-1), ['network', 'rm', lease.resources.network!.id]);
    const released = readBackendLease(path);
    assert.equal(released.state, 'released');
    assert(Object.values(released.resources.serviceContainers!).every(container => container.removedAt && !container.running));
  } finally { rmSync(root, { recursive: true, force: true }); }
});
