import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { createBackendLease, claimBackendResources, backendResourceLockKeys, resourceLockScope, readBackendLease }
  from '../src/runtime/backend-lease.js';
import type { TextCommandExecutor } from '../src/runtime/command-executor.js';
import { activateConvex, controlConvex, releaseConvex, CONVEX_BACKEND_IMAGE } from '../src/stacks/backends/convex-lifecycle.js';
import { readConvexTables } from '../src/stacks/backends/convex-operations.js';

// Failure cases defined before the transport change: stale ownership must still
// refuse access; missing tables must still fail; both routes must authenticate
// after restart/reset. Compare the same reader over both real routes.
test('Convex native observer preserves ownership and lifecycle behavior over its published port', async () => {
  assert.equal(process.platform, 'linux', 'Run inside the Docker controller');
  const out = process.env.STACK_BENCH_OBSERVER_EVIDENCE;
  assert(out, 'Set STACK_BENCH_OBSERVER_EVIDENCE to a fresh evidence directory');
  mkdirSync(out, { recursive: false });
  const leasePath = join(out, 'lease.json');
  const lease = createBackendLease({ runId: `observer-transport-${Date.now()}`, backend: 'convex',
    track: 'ecommerce', runIndex: 0, serverUri: 'http://127.0.0.1:14310' });
  const ports = { vite: 14309, express: 14311, dbPort: null };
  const samples: { phase: string; route: string; ms: number }[] = [];
  const checks: string[] = [];
  const receipt = { result: 'failed', cleanup: 'pending', samples, checks,
    note: 'Empty native snapshots measure fixed transport overhead. Not a whole-catalog timing or qualification receipt.',
    image: CONVEX_BACKEND_IMAGE,
    sourceSha256: createHash('sha256').update(readFileSync(new URL('../src/stacks/backends/convex-operations.js', import.meta.url))).digest('hex'),
    rerun: 'STACK_BENCH_OBSERVER_EVIDENCE=<fresh directory> node --test dist/tests/convex-observer.integration.js' };
  try {
    claimBackendResources(leasePath, lease, { ...resourceLockScope(), keys: backendResourceLockKeys(lease, ports) });
    activateConvex({ leasePath, leaseToken: lease.ownershipToken, ports });
    for (const phase of ['initial', 'restart', 'reset'] as const) {
      if (phase !== 'initial') await controlConvex({ leasePath, leaseToken: lease.ownershipToken, ports, mode: phase });
      const active = readBackendLease(leasePath, { token: lease.ownershipToken, active: true });
      for (let n = 0; n < 12; n++) {
        const route = n % 2 ? 'current' : 'docker-exec-baseline';
        const exec: TextCommandExecutor = (command, args, options) => {
          const configured = { ...options, env: { ...process.env, http_proxy: 'http://127.0.0.1:9',
            HTTP_PROXY: 'http://127.0.0.1:9', ALL_PROXY: 'http://127.0.0.1:9', no_proxy: '', NO_PROXY: '' } };
          return route === 'docker-exec-baseline' && command === 'curl'
            ? execFileSync('docker', ['exec', '-i', active.resources.container!.id, 'curl', ...args], configured)
            : execFileSync(command, args, configured);
        };
        const started = performance.now();
        assert.deepEqual(readConvexTables([], { lease: active, exec }), {});
        samples.push({ phase, route, ms: performance.now() - started });
      }
      assert.throws(() => readConvexTables(['missing_observer_table'], { lease: active }), /required Convex data table is missing/);
      const stale = structuredClone(active);
      stale.resources.network!.namespaceStartedAt = 'not-the-current-start';
      assert.throws(() => readConvexTables([], { lease: stale }), /namespace anchor/);
      const replaced = structuredClone(active);
      replaced.resources.container!.id = 'not-the-owned-container';
      assert.throws(() => readConvexTables([], { lease: replaced }), /changed after lease creation/);
      checks.push(`${phase}: consistent snapshots; missing table refused; stale namespace refused; replaced container refused`);
    }
    receipt.result = 'passed';
  } catch (error) {
    Object.assign(receipt, { error: String(error) });
    throw error;
  } finally {
    try {
      releaseConvex(leasePath, lease.ownershipToken);
      receipt.cleanup = readBackendLease(leasePath).state;
      assert.equal(receipt.cleanup, 'released');
    } catch (error) {
      Object.assign(receipt, { result: 'failed', cleanup: String(error) });
      throw error;
    } finally { writeFileSync(join(out, 'receipt.json'), JSON.stringify(receipt, null, 2)); }
  }
});
