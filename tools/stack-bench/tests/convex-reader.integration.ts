import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import test from 'node:test';
import { createBackendLease, claimBackendResources, backendResourceLockKeys, resourceLockScope, readBackendLease, writeBackendLease }
  from '../src/runtime/backend-lease.js';
import { activateConvex, controlConvex, CONVEX_BACKEND_IMAGE } from '../src/stacks/backends/convex-lifecycle.js';
import { releaseBackendLease } from '../src/runtime/backend-teardown.js';
import { createConvexOrderDataReader, getConvexCheckoutState, convexAdminKey } from '../src/stacks/backends/convex-operations.js';
import { orderDataColumns } from '../src/stacks/order-data.js';

// Failure cases, before implementation: cached rows can hide new writes; cached
// clients can retain expired deadlines; cached credentials must not cross lease,
// error, close, restart or reset boundaries. Compare full fresh snapshots, and
// require a real ownership failure before any HTTP request on a warm reader.
test('scoped Convex reader retains fresh state and refuses stale ownership', async () => {
  assert.equal(process.platform, 'linux');
  const out = process.env.STACK_BENCH_OBSERVER_EVIDENCE;
  assert(out, 'Set STACK_BENCH_OBSERVER_EVIDENCE to a fresh directory');
  mkdirSync(out, { recursive: false });
  const leasePath = `${out}/lease.json`;
  const lease = createBackendLease({ runId: `convex-reader-${Date.now()}`, backend: 'convex', track: 'ecommerce',
    runIndex: 0, serverUri: 'http://127.0.0.1:14310' });
  const ports = { vite: 14309, express: 14311, dbPort: null };
  const storage = { kind: 'order-data', cart: true, warehouses: true } as const;
  const selection = { storage, account: 'reader-observer', item: 'Product 0' };
  const checks: string[] = [];
  const samples: { phase: string; route: string; population: number; ms: number }[] = [];
  const receipt = { result: 'failed', cleanup: 'pending', checks, samples, image: CONVEX_BACKEND_IMAGE,
    controller: process.env.STACK_BENCH_CONTROLLER_IMAGE_ID,
    sourceSha256: createHash('sha256').update(readFileSync(new URL('../src/stacks/backends/convex-operations.js', import.meta.url))).digest('hex'),
    rerun: 'STACK_BENCH_OBSERVER_EVIDENCE=<fresh directory> node --test dist/tests/convex-reader.integration.js' };
  try {
    claimBackendResources(leasePath, lease, { ...resourceLockScope(), keys: backendResourceLockKeys(lease, ports) });
    activateConvex({ leasePath, leaseToken: lease.ownershipToken, ports });
    let population = 0;
    for (const phase of ['initial', 'restart', 'reset'] as const) {
      if (phase !== 'initial') await controlConvex({ leasePath, leaseToken: lease.ownershipToken, ports, mode: phase });
      const active = readBackendLease(leasePath, { token: lease.ownershipToken, active: true });
      const key = convexAdminKey(active);
      const mutation = (path: string, args: unknown) => {
        const config = [`url = ${JSON.stringify(active.resources.serverUri + '/api/mutation')}`,
          `header = ${JSON.stringify('Authorization: Convex ' + key)}`, 'header = "Content-Type: application/json"',
          `data = ${JSON.stringify(JSON.stringify({ path, args, format: 'json' }))}`].join('\n');
        const result = JSON.parse(execFileSync('curl', ['--disable', '--noproxy', '*', '--silent', '--show-error', '--fail',
          '--max-time', '30', '--config', '-'], { encoding: 'utf8', input: config, timeout: 30_000 }));
        assert.equal(result.status, 'success');
      };
      if (phase !== 'restart') {
        population = 0;
        for (const table of Object.keys(orderDataColumns(storage))) mutation('_system/frontend/createTable', { table, componentId: null });
      }
      let keys = 0;
      let requests = 0;
      let stale = false;
      const reader = createConvexOrderDataReader({ path: leasePath, lease: active, exec(command, args, options) {
        if (args.includes('./generate_admin_key.sh')) keys++;
        if (command === 'curl') requests++;
        const result = execFileSync(command, args, options);
        // A real Docker inspection, with the namespace start replaced to emulate
        // an external restart without destroying the owned fixture mid-test.
        if (stale && args[0] === 'inspect') return result.replaceAll(active.resources.network!.namespaceStartedAt!, 'stale-start');
        return result;
      } });
      try {
        for (const count of [3, 247, 750]) {
          mutation('_system/frontend/addDocument', { table: 'item', componentId: null,
            documents: Array.from({ length: count }, (_, i) => ({ name: `Product ${population + i}`, price: 11.25,
              description: 'Catalog product', category: 'Performance', variants: [] })) });
          population += count;
          for (let n = 0; n < 6; n++) {
            const route = n % 2 ? 'scoped' : 'stateless';
            const start = performance.now();
            const snapshot = route === 'scoped' ? reader.read(selection) : getConvexCheckoutState({ ...selection, lease: active });
            samples.push({ phase, route, population, ms: performance.now() - start });
            assert.equal(snapshot.catalog.length, population);
            assert.equal(new Set(snapshot.catalog.map(row => row.name)).size, population);
            assert(snapshot.catalog.some(row => row.name === `Product ${population - 1}`));
            assert.deepEqual(snapshot, getConvexCheckoutState({ ...selection, lease: active }));
          }
        }
        assert.equal(keys, 1, 'one native credential per feature');
        stale = true;
        const before = requests;
        assert.throws(() => reader.read(selection), /namespace anchor/);
        assert.equal(requests, before, 'ownership refusal precedes HTTP');
        stale = false;
        assert.deepEqual(reader.read(selection), getConvexCheckoutState({ ...selection, lease: active }));
        assert.equal(keys, 2, 'read failure discards credential');
        const beforeRelease = requests;
        try {
          writeBackendLease(leasePath, { ...active, state: 'released' });
          assert.throws(() => reader.read(selection), /active/);
          assert.equal(requests, beforeRelease, 'released lease refuses access while container still runs');
        } finally { writeBackendLease(leasePath, active); }
        assert.deepEqual(reader.read(selection), getConvexCheckoutState({ ...selection, lease: active }));
        assert.equal(keys, 3, 'lease failure discards credential');
        checks.push(`${phase}: full snapshots match; new writes visible; stale ownership refused; error discards credential`);
        checks.push(`${phase}: released lease refused before HTTP while its container remains alive`);
      } finally { reader.close(); }
      assert.throws(() => reader.read(selection), /closed/);
      checks.push(`${phase}: closed reader refuses reuse before the next lifecycle operation`);
    }
    receipt.result = 'passed';
  } catch (error) { Object.assign(receipt, { error: String(error) }); throw error; }
  finally {
    try {
      releaseBackendLease(leasePath, lease.ownershipToken);
      receipt.cleanup = readBackendLease(leasePath).state;
      assert.equal(receipt.cleanup, 'released');
    } catch (error) {
      Object.assign(receipt, { result: 'failed', cleanup: String(error) });
      // A cleanup failure must fail the test even after an earlier error.
      // eslint-disable-next-line no-unsafe-finally
      throw error;
    } finally { writeFileSync(`${out}/receipt.json`, JSON.stringify(receipt, null, 2)); }
  }
});
