import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { createServer } from 'node:http';
import type { Server } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { compiledEntrypoint } from '../src/package-root.js';
import { createBackendLease, readBackendLease, writeBackendLease }
  from '../src/runtime/backend-lease.js';
import { releaseBackendLease } from '../src/runtime/backend-teardown.js';
import { processIdentity } from '../src/runtime/platform.js';

// Lease rules that need a real second process: an owned runtime process to
// stop, and a lock held by another controller.

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
