import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { GRADING_CAPABILITY_IDS } from '../src/actions/action-contract.js';
import { createStackAdapterRegistry } from '../src/stacks/stack-adapter-contract.js';
import { leasedDatabaseEnvironment, STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { stackAdapterVersion } from '../src/stacks/stack-identities.js';
import { setSpacetimeStock } from '../src/stacks/backends/spacetime-operations.js';
import type { Track } from '../src/composition/tracks.js';

test('built-in adapters preserve the port grid and lease identity', () => {
  assert.deepEqual(STACK_ADAPTER_REGISTRY.ids, ['mongodb', 'postgres', 'spacetime', 'stub']);
  assert.equal(stackAdapterVersion('postgres'), '1.5.0');
  assert.equal(STACK_ADAPTER_REGISTRY.get('mongodb').version, '1.4.0');
  assert.equal(STACK_ADAPTER_REGISTRY.get('spacetime').version, '1.3.0');
  assert.equal(STACK_ADAPTER_REGISTRY.get('stub').version, '1.1.0');
  assert.throws(() => stackAdapterVersion('unknown'), /unknown stack adapter/);

  const postgres = STACK_ADAPTER_REGISTRY.get('postgres');
  assert.deepEqual(postgres.ports.forRun({ trackOffset: 100, runIndex: 2 }),
    { vite: 6375, express: 6103, dbPort: 6532 });
  assert.deepEqual(postgres.lease.prepare({
    track: { name: 'shop' } as Track,
    runIndex: 2,
    serverUri: null,
    runtimeDir: resolve('runtime'),
    env: {},
    helpers: {
      moduleName: () => 'unused',
      dbName: (track: Track, runIndex: number) => `${track.name}-${runIndex}`,
      containerIdentity: (name: string) => ({ name, id: 'container-id' }),
    },
  }), {
    lease: { serverUri: null, database: 'shop-2', module: null, dataDir: null,
      container: { name: 'stack-bench-dev-postgres', id: 'container-id' } },
    lockKeys: [],
  });
  assert.equal(typeof postgres.reset.run, 'function');
  assert.equal(typeof postgres.databaseWrite.setStock, 'function');
});

test('build plans expose only artifacts owned by the selected stack', () => {
  const appDir = resolve('bench', 'run', 'app');
  const repo = resolve('repo');
  const spacetime = STACK_ADAPTER_REGISTRY.get('spacetime').buildContainer.plan({ repo, appDir });
  assert.equal(spacetime.mounts.some(mount => mount.target === '/deps/.spacetimedb-cli'), true);
  const encodedWrapper = spacetime.init.match(/printf %s ([A-Za-z0-9+/=]+) \| base64/)?.[1];
  assert(encodedWrapper);
  assert.match(Buffer.from(encodedWrapper, 'base64').toString(),
    /SpacetimeDB publish and dev must use the run identity/);
  assert.equal(spacetime.readyFile, '/deps/.ready');
  assert.equal(spacetime.networkNamespace, null);

  const appliance = STACK_ADAPTER_REGISTRY.get('spacetime').buildContainer.plan({
    repo,
    appDir,
    env: { STACK_BENCH_RELEASE_DEPS_VOLUME: 'stack-bench-release-deps',
      STACK_BENCH_APPLIANCE: '1' },
  });
  assert.deepEqual(appliance.requiredPaths, []);
  assert.deepEqual(appliance.mounts, [
    { kind: 'volume', source: 'stack-bench-release-deps', target: '/release-deps', readOnly: true },
  ]);
  assert.doesNotMatch(appliance.init, /npm install|npm pack/);
  assert.equal(appliance.networkNamespace, 'host');

  for (const adapter of [STACK_ADAPTER_REGISTRY.get('postgres'),
    STACK_ADAPTER_REGISTRY.get('mongodb'), STACK_ADAPTER_REGISTRY.get('stub')]) {
    const plan = adapter.buildContainer.plan({ env: {} });
    assert.deepEqual(plan.mounts, [], `${adapter.id} must not receive another stack's artifacts`);
    assert.deepEqual(plan.requiredPaths, []);
    assert.equal(plan.readyFile, null);
  }
});

test('hosted stacks receive the exact leased database environment', () => {
  assert.deepEqual(leasedDatabaseEnvironment(STACK_ADAPTER_REGISTRY.get('postgres'), {
    database: 'app_ecom_run6', networkMode: 'host',
  }), { DATABASE_URL: 'postgresql://appuser:local-app-password@127.0.0.1:6532/app_ecom_run6' });
  assert.deepEqual(leasedDatabaseEnvironment(STACK_ADAPTER_REGISTRY.get('mongodb'), {
    database: 'app_ecom_run7', networkMode: 'bridge',
  }), { DATABASE_URL: 'mongodb://host.docker.internal:6537/app_ecom_run7' });
  assert.deepEqual(leasedDatabaseEnvironment(STACK_ADAPTER_REGISTRY.get('spacetime'), {
    database: null, networkMode: 'host',
  }), {});
});

test('SpacetimeDB container operations use the isolated agent identity', () => {
  const calls: Array<[string, readonly string[]]> = [];
  const exec = (command: string, args: readonly string[]): string => {
    calls.push([command, args]);
    const sql = args.at(-1);
    assert(sql);
    if (/select id from item/.test(sql)) return '1\n';
    if (/select id from warehouse/.test(sql)) return '2\n';
    if (/select quantity/.test(sql)) return '3\n';
    return '';
  };
  setSpacetimeStock({ item: 'widget', warehouse: 'east', quantity: 3,
    spacetime: { buildContainer: { name: 'leased-build', id: 'leased-build-id' }, mod: 'shop',
      containerUri: 'http://host.docker.internal:3000' }, exec });
  assert.equal(calls.length, 4);
  for (const [command, args] of calls) {
    assert.equal(command, 'docker');
    assert.deepEqual(args.slice(0, 8), ['exec', '--user', '10001:10001', '-e',
      'HOME=/home/developer', '-e', 'USER=developer', 'leased-build-id']);
  }
});

test('named actions map parameters to HTTP and SpacetimeDB requests', () => {
  const restock = { id: 'restock', path: '/api/admin/restock', reducer: 'admin_restock',
    args: [0, 0, 1], params: [{ name: 'itemId', in: 'body' as const, wireType: 'u64' as const },
      { name: 'warehouseId', in: 'body' as const, wireType: 'u64' as const },
      { name: 'quantity', in: 'body' as const }] };
  const http = STACK_ADAPTER_REGISTRY.get('postgres').namedAction.request({
    action: restock,
    input: { values: { itemId: 'item', warehouseId: 'warehouse', quantity: 3 } },
    url: 'http://app.test',
  });
  assert.deepEqual(JSON.parse(http.body),
    { itemId: 'item', warehouseId: 'warehouse', quantity: 3 });

  const spacetime = STACK_ADAPTER_REGISTRY.get('spacetime').namedAction.request({
    action: restock,
    input: { values: { itemId: '7', warehouseId: '18446744073709551615', quantity: 3 } },
    spacetime: { uri: 'http://stdb.test', mod: 'shop' },
  });
  assert.equal(spacetime.url, 'http://stdb.test/v1/database/shop/call/admin_restock');
  assert.equal(spacetime.body, '[7,18446744073709551615,3]');
  assert.throws(() => STACK_ADAPTER_REGISTRY.get('spacetime').namedAction.request({
    action: restock,
    input: { values: { itemId: '-1', warehouseId: '9', quantity: 3 } },
    spacetime: { uri: 'http://stdb.test', mod: 'shop' },
  }), /invalid u64 value/);
});

test('every adapter declares what the grader can measure on it', () => {
  const known = new Set<string>(GRADING_CAPABILITY_IDS);
  for (const id of STACK_ADAPTER_REGISTRY.ids) {
    const { grading } = STACK_ADAPTER_REGISTRY.get(id);
    assert.ok(['http', 'reducer'].includes(grading.transport), `${id} transport`);
    assert.ok(grading.capabilities.every(capability => known.has(capability)), `${id} capabilities`);
    assert.equal(new Set(grading.capabilities).size, grading.capabilities.length, `${id} duplicates`);
  }
  assert.equal(STACK_ADAPTER_REGISTRY.get('spacetime').grading.transport, 'reducer');
  for (const id of ['postgres', 'mongodb', 'stub'] as const) {
    assert.equal(STACK_ADAPTER_REGISTRY.get(id).grading.transport, 'http');
  }
  for (const id of ['spacetime', 'postgres', 'mongodb'] as const) {
    assert.deepEqual([...STACK_ADAPTER_REGISTRY.get(id).grading.capabilities], [...GRADING_CAPABILITY_IDS]);
  }
  const stub = STACK_ADAPTER_REGISTRY.get('stub');
  const stubCapabilities: readonly string[] = stub.grading.capabilities;
  assert.deepEqual(GRADING_CAPABILITY_IDS.filter(id => !stubCapabilities.includes(id)),
    ['backend-lifecycle', 'database-write']);
  assert.equal('databaseWrite' in stub, false);
  assert.equal(stub.lifecycle.control, undefined);
});

test('registry rejects unknown, duplicate, and invalid adapter identities', () => {
  assert.throws(() => STACK_ADAPTER_REGISTRY.get('new-db'), /unknown stack adapter/);
  const identity = { id: 'fake', version: '1.0.0',
    lifecycle: { activate: () => undefined } } as const;
  assert.equal(createStackAdapterRegistry([identity]).get('fake').version, '1.0.0');
  assert.throws(() => createStackAdapterRegistry([identity, identity]), /duplicate stack adapter/);
  assert.throws(() => createStackAdapterRegistry([{ ...identity, version: 'latest' }]),
    /version is invalid/);
});
