import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createServer } from 'node:http';
import test from 'node:test';

import { readArtifactPayload } from '../artifacts.mjs';
import { acquireResourceLock, createBackendLease, readBackendLease,
  writeBackendLease } from '../backend-lease.mjs';
import { recoveryPlan, recoverSupervisedRun, SUPERVISOR_STATE_VERSION,
  validateSupervisorState } from '../recovery.mjs';

function fixture({ state = 'active' } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-recovery-'));
  const output = join(root, 'results');
  const leasePath = join(root, 'private', 'backend-lease.json');
  const statePath = join(root, 'private', 'supervisor.json');
  const locks = join(root, 'locks');
  mkdirSync(output, { recursive: true });
  const lease = createBackendLease({ runId: 'recovery-postgres-run0', backend: 'postgres',
    track: 'ecommerce', runIndex: 0, database: 'stackbench_recovery',
    container: { name: 'stack-bench-postgres', id: 'postgres-id' } });
  lease.state = state;
  lease.resources.locks.push(acquireResourceLock({ root: locks,
    key: 'slot:ecommerce:postgres:run0', lease }));
  writeBackendLease(leasePath, lease);
  const supervisor = { version: SUPERVISOR_STATE_VERSION, runId: lease.runId,
    backend: lease.backend, runtimeDir: resolve(join(root, 'private')),
    leasePath: resolve(leasePath), ownershipToken: lease.ownershipToken,
    output: resolve(output) };
  writeFileSync(statePath, `${JSON.stringify(supervisor)}\n`);
  return { root, output, leasePath, statePath, lease, supervisor };
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
    const result = recoverSupervisedRun(f.statePath);
    assert.equal(result.ok, true);
    assert.equal(existsSync(f.statePath), false);
    assert.equal(existsSync(f.leasePath), false, 'private runtime lease must be removed after recovery');
    assert.equal(existsSync(f.lease.resources.locks[0].path), false);
    const recovery = readArtifactPayload(join(f.output, 'recovery.json'),
      { expectedKind: 'recovery' });
    assert.equal(recovery.status, 'clean');
    assert.equal(JSON.stringify(recovery).includes(f.lease.ownershipToken), false);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('wrong credentials and malformed supervisor state never clean resources', () => {
  const f = fixture();
  try {
    const wrong = { ...f.supervisor, ownershipToken: 'wrong' };
    writeFileSync(f.statePath, JSON.stringify(wrong));
    assert.throws(() => recoverSupervisedRun(f.statePath), /token does not match/);
    assert.equal(readBackendLease(f.leasePath, { token: f.lease.ownershipToken }).state, 'active');
    assert.equal(existsSync(f.lease.resources.locks[0].path), true);
    assert.throws(() => validateSupervisorState({ ...f.supervisor, extra: true }), /extra is unknown/);
    assert.throws(() => validateSupervisorState({ ...f.supervisor, leasePath: 'relative.json' }),
      /must be absolute/);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('an unleased live listener produces quarantine and remains untouched until safe retry', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-recovery-refusal-'));
  const output = join(root, 'results');
  const leasePath = join(root, 'private', 'backend-lease.json');
  const statePath = join(root, 'private', 'supervisor.json');
  mkdirSync(output, { recursive: true });
  const server = createServer((_request, response) => response.end('foreign'));
  await new Promise((done, fail) => server.listen(0, '127.0.0.1', done).once('error', fail));
  try {
    const port = server.address().port;
    const lease = createBackendLease({ runId: 'recovery-spacetime-run0', backend: 'spacetime',
      track: 'ecommerce', runIndex: 0, serverUri: `http://127.0.0.1:${port}`,
      module: 'recovery-module', dataDir: join(root, 'data') });
    lease.state = 'active';
    lease.resources.listenerPids = [process.pid + 100_000];
    writeBackendLease(leasePath, lease);
    writeFileSync(statePath, JSON.stringify({ version: SUPERVISOR_STATE_VERSION,
      runId: lease.runId, backend: lease.backend, runtimeDir: resolve(join(root, 'private')),
      leasePath: resolve(leasePath),
      ownershipToken: lease.ownershipToken, output: resolve(output) }));

    const refused = recoverSupervisedRun(statePath);
    assert.deepEqual(refused, { ok: false, state: 'quarantined', runId: lease.runId,
      recoveryPath: join(output, 'recovery.json') });
    assert.equal(existsSync(statePath), true, 'private recovery authority must survive refusal');
    assert.equal((await fetch(`http://127.0.0.1:${port}`)).status, 200,
      'unleased listener was disturbed');
    const quarantine = readArtifactPayload(join(output, 'recovery.json'),
      { expectedKind: 'recovery' });
    assert.equal(quarantine.status, 'quarantined');
    assert.deepEqual(quarantine.resources.listenerPids, [String(process.pid + 100_000)]);

    server.closeAllConnections();
    await new Promise(done => server.close(done));
    const cleaned = recoverSupervisedRun(statePath);
    assert.equal(cleaned.ok, true);
    assert.equal(existsSync(statePath), false);
  } finally {
    if (server.listening) {
      server.closeAllConnections();
      await new Promise(done => server.close(done));
    }
    rmSync(root, { recursive: true, force: true });
  }
});
