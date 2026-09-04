import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createServer } from 'node:http';
import test from 'node:test';

import { readArtifactPayload } from '../src/evidence/artifacts.js';
import { acquireResourceLock, createBackendLease, readBackendLease,
  writeBackendLease, type BackendLease } from '../src/runtime/backend-lease.js';
import { recoverBackendLease, recoveryPlan, recoverSupervisedRun, SUPERVISOR_STATE_VERSION,
  validateSupervisorState, type SupervisorState } from '../src/runtime/recovery.js';

function fixture({ state = 'active' }: { state?: BackendLease['state'] } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-recovery-'));
  const output = join(root, 'results');
  const runtimeRoot = join(root, 'runtime');
  const leasePath = join(runtimeRoot, 'private', 'backend-lease.json');
  const statePath = join(runtimeRoot, 'private', 'supervisor.json');
  const locks = join(root, 'locks');
  mkdirSync(output, { recursive: true });
  const lease = createBackendLease({ runId: 'recovery-postgres-run0', backend: 'postgres',
    track: 'ecommerce', runIndex: 0, database: 'app_recovery',
    container: { name: 'stack-bench-postgres', id: 'postgres-id' } });
  lease.state = state;
  lease.resources.locks.push(acquireResourceLock({ root: locks,
    key: 'slot:ecommerce:postgres:run0', lease }));
  writeBackendLease(leasePath, lease);
  const supervisor: SupervisorState = { version: SUPERVISOR_STATE_VERSION, runId: lease.runId,
    backend: lease.backend, runtimeDir: resolve(join(runtimeRoot, 'private')),
    leasePath: resolve(leasePath), ownershipToken: lease.ownershipToken,
    output: resolve(output) };
  writeFileSync(statePath, `${JSON.stringify(supervisor)}\n`);
  return { root, runtimeRoot, output, leasePath, statePath, lease, supervisor };
}

test('recovery plans are deterministic, public, and actionable', () => {
  const f = fixture();
  try {
    const plan = recoveryPlan(f.lease, { cleanupSucceeded: false, reason: 'identity mismatch\nsecret tail' });
    assert.equal(plan.status, 'quarantined');
    assert.equal(plan.reason, 'identity mismatch');
    assert.deepEqual(plan.resources.locks,
      [{ key: 'slot:ecommerce:postgres:run0', released: false }]);
    assert.equal(JSON.stringify(plan).includes(f.lease.ownershipToken), false);
    assert.ok(plan.instructions.some(line => /Do not start another run/.test(line)));
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('authenticated recovery releases exact lease resources and removes private state', () => {
  const f = fixture();
  try {
    const result = recoverSupervisedRun(f.statePath, { runtimeRoot: f.runtimeRoot });
    assert.equal(result.ok, true);
    assert.equal(existsSync(f.statePath), false);
    assert.equal(existsSync(f.leasePath), false, 'private runtime lease must be removed after recovery');
    assert.equal(existsSync(firstLockPath(f.lease)), false);
    const recovery = readArtifactPayload(join(f.output, 'recovery.json'),
      { expectedKind: 'recovery' });
    assert.equal(recovery.status, 'clean');
    assert.equal(JSON.stringify(recovery).includes(f.lease.ownershipToken), false);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('authenticated lease recovery works when parent supervisor state is missing', () => {
  const f = fixture();
  const output = join(f.root, 'lease-recovery');
  try {
    rmSync(f.statePath, { force: true });
    const result = recoverBackendLease(f.leasePath, resolve(output), { runtimeRoot: f.runtimeRoot });
    assert.equal(result.ok, true);
    assert.equal(existsSync(f.leasePath), false);
    assert.equal(existsSync(firstLockPath(f.lease)), false);
    const recovery = readArtifactPayload(join(output, 'recovery.json'),
      { expectedKind: 'recovery' });
    assert.equal(recovery.status, 'clean');
    assert.equal(JSON.stringify(recovery).includes(f.lease.ownershipToken), false);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('recovery refuses a lease outside the configured runtime root', () => {
  const f = fixture();
  const sentinel = join(f.runtimeRoot, 'private', 'keep.txt');
  try {
    writeFileSync(sentinel, 'keep\n');
    assert.throws(() => recoverBackendLease(f.leasePath, resolve(join(f.root, 'rejected')), {
      runtimeRoot: join(f.root, 'different-runtime-root'),
    }), /not a direct child/);
    assert.equal(existsSync(sentinel), true);
    assert.equal(existsSync(f.leasePath), true);
    assert.equal(existsSync(firstLockPath(f.lease)), true);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('wrong credentials and malformed supervisor state never clean resources', () => {
  const f = fixture();
  try {
    const wrong = { ...f.supervisor, ownershipToken: 'wrong' };
    writeFileSync(f.statePath, JSON.stringify(wrong));
    assert.throws(() => recoverSupervisedRun(f.statePath, { runtimeRoot: f.runtimeRoot }),
      /token does not match/);
    assert.equal(readBackendLease(f.leasePath, { token: f.lease.ownershipToken }).state, 'active');
    assert.equal(existsSync(firstLockPath(f.lease)), true);
    assert.throws(() => validateSupervisorState({ ...f.supervisor, extra: true }), /extra is unknown/);
    assert.throws(() => validateSupervisorState({ ...f.supervisor, leasePath: 'relative.json' }),
      /must be absolute/);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('an unleased live listener produces quarantine and remains untouched until safe retry', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-recovery-refusal-'));
  const output = join(root, 'results');
  const runtimeRoot = join(root, 'runtime');
  const leasePath = join(runtimeRoot, 'private', 'backend-lease.json');
  const statePath = join(runtimeRoot, 'private', 'supervisor.json');
  mkdirSync(output, { recursive: true });
  const server = createServer((_request, response) => response.end('foreign'));
  await new Promise<void>((done, fail) => server.listen(0, '127.0.0.1', done).once('error', fail));
  try {
    const address = server.address();
    assert(address && typeof address !== 'string', 'the recovery listener must have a port');
    const port = address.port;
    const lease = createBackendLease({ runId: 'recovery-spacetime-run0', backend: 'spacetime',
      track: 'ecommerce', runIndex: 0, serverUri: `http://127.0.0.1:${port}`,
      module: 'recovery-module', dataDir: join(root, 'data') });
    lease.state = 'active';
    lease.resources.listenerProcesses = [{ pid: process.pid + 100_000, startMarker: '1' }];
    writeBackendLease(leasePath, lease);
    writeFileSync(statePath, JSON.stringify({ version: SUPERVISOR_STATE_VERSION,
      runId: lease.runId, backend: lease.backend, runtimeDir: resolve(join(runtimeRoot, 'private')),
      leasePath: resolve(leasePath),
      ownershipToken: lease.ownershipToken, output: resolve(output) }));

    const refused = recoverSupervisedRun(statePath, { runtimeRoot });
    assert.deepEqual(refused, { ok: false, state: 'quarantined', runId: lease.runId,
      recoveryPath: join(output, 'recovery.json') });
    assert.equal(existsSync(statePath), true, 'private recovery authority must survive refusal');
    assert.equal((await fetch(`http://127.0.0.1:${port}`)).status, 200,
      'unleased listener was disturbed');
    const quarantine = readArtifactPayload(join(output, 'recovery.json'),
      { expectedKind: 'recovery' });
    assert.equal(quarantine.status, 'quarantined');
    assert.deepEqual(quarantineResources(quarantine).listenerProcesses,
      [{ pid: process.pid + 100_000, startMarker: '1' }]);

    server.closeAllConnections();
    await closeServer(server);
    const cleaned = recoverSupervisedRun(statePath, { runtimeRoot });
    assert.equal(cleaned.ok, true);
    assert.equal(existsSync(statePath), false);
  } finally {
    if (server.listening) {
      server.closeAllConnections();
      await closeServer(server);
    }
    rmSync(root, { recursive: true, force: true });
  }
});

function firstLockPath(lease: BackendLease): string {
  const [lock] = lease.resources.locks;
  if (!lock) throw new Error('the recovery fixture must own a lock');
  return lock.path;
}

function quarantineResources(value: unknown): { listenerProcesses: unknown[] } {
  if (!isRecord(value) || !isRecord(value.resources) || !Array.isArray(value.resources.listenerProcesses)) {
    throw new Error('recovery artifact has no listener process ids');
  }
  return { listenerProcesses: value.resources.listenerProcesses };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function closeServer(server: ReturnType<typeof createServer>): Promise<void> {
  return new Promise((resolveClose, reject) => server.close(error => error ? reject(error) : resolveClose()));
}
