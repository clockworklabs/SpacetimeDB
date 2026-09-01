import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { createStackAdapterRegistry, executeStackCapability,
  STACK_ADAPTER_SCHEMA_VERSION, STACK_CAPABILITY_SCHEMA_VERSION } from '../src/stacks/stack-adapter-contract.js';
import { leasedDatabaseEnvironment, STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { stackAdapterVersion } from '../src/stacks/stack-identities.js';
import { setSpacetimeStock } from '../src/stacks/backends/spacetime-operations.js';
import type { Track } from '../src/composition/tracks.js';
import type { StackAdapter } from '../src/stacks/stack-adapter-contract.js';

type UnknownRecord = Record<string, unknown>;

function isRecord(value: unknown): value is UnknownRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function record(value: unknown, label: string): UnknownRecord {
  assert(isRecord(value), `${label} must be an object`);
  return value;
}

function buildPlan(value: unknown) {
  const plan = record(value, 'build container plan');
  assert(Array.isArray(plan.mounts));
  assert(Array.isArray(plan.requiredPaths));
  assert(typeof plan.init === 'string');
  assert(plan.readyFile === null || typeof plan.readyFile === 'string');
  assert(plan.networkNamespace === null || typeof plan.networkNamespace === 'string');
  const mounts = plan.mounts.map((value, index) => {
    const mount = record(value, `build container mount ${index}`);
    assert(typeof mount.target === 'string');
    return mount;
  });
  return { mounts, requiredPaths: plan.requiredPaths, init: plan.init,
    readyFile: plan.readyFile, networkNamespace: plan.networkNamespace };
}

function namedActionRequest(value: unknown) {
  const request = record(value, 'named action request');
  assert(request.url === null || typeof request.url === 'string');
  assert(typeof request.method === 'string');
  assert(typeof request.body === 'string');
  return { url: request.url, method: request.method, body: request.body };
}

test('built-in and deterministic test stack adapters preserve the proven port grid', () => {
  assert.deepEqual(STACK_ADAPTER_REGISTRY.ids, ['mongodb', 'postgres', 'spacetime', 'stub']);
  const postgres = STACK_ADAPTER_REGISTRY.get('postgres');
  assert.equal(postgres.version, '1.4.0');
  assert.equal(STACK_ADAPTER_REGISTRY.get('mongodb').version, '1.3.0');
  assert.equal(STACK_ADAPTER_REGISTRY.get('spacetime').version, '1.1.0');
  assert.equal(STACK_ADAPTER_REGISTRY.get('stub').version, '1.1.0');
  assert.equal(stackAdapterVersion('postgres'), '1.4.0');
  assert.throws(() => stackAdapterVersion('unknown'), /unknown stack adapter/);
  assert.deepEqual(executeStackCapability(postgres, 'ports', 'for-run',
    { trackOffset: 100, runIndex: 2 }), { vite: 6375, express: 6103, dbPort: 6532 });
  const prepared = executeStackCapability(postgres, 'lease', 'prepare', {
    track: { name: 'shop' }, runIndex: 2, env: {},
    helpers: {
      dbName: (track: Track, runIndex: number) => `${track.name}-${runIndex}`,
      containerIdentity: (name: string) => ({ name, id: 'container-id' }),
    },
  });
  assert.deepEqual(prepared, {
    lease: { serverUri: null, database: 'shop-2', module: null, dataDir: null,
      container: { name: 'stack-bench-dev-postgres', id: 'container-id' } },
    lockKeys: [],
  });
  assert(postgres.capabilities.reset);
  assert(postgres.capabilities['database-write']);
  assert.equal(postgres.capabilities.reset.operations.includes('run'), true);
  assert.equal(postgres.capabilities['database-write'].operations.includes('set-stock'), true);
});

test('build-container plans expose only artifacts owned by the selected stack', () => {
  const appDir = resolve('bench', 'run', 'app');
  const repo = resolve('repo');
  const spacetime = buildPlan(executeStackCapability(STACK_ADAPTER_REGISTRY.get('spacetime'),
    'build-container', 'plan', { repo, appDir }));
  assert.equal(spacetime.mounts.some(mount => mount.target === '/deps/spacetimedb-cli'), true);
  assert.equal(spacetime.mounts.some(mount =>
    mount.target === '/home/developer/.config/spacetime'), true);
  assert.equal(spacetime.readyFile, '/deps/.ready');
  assert.equal(spacetime.networkNamespace, null);
  const appliance = buildPlan(executeStackCapability(STACK_ADAPTER_REGISTRY.get('spacetime'),
    'build-container', 'plan', {
      repo, appDir, env: { STACK_BENCH_RELEASE_DEPS_VOLUME: 'stack-bench-release-deps',
        STACK_BENCH_APPLIANCE: '1' },
    }));
  assert.deepEqual(appliance.requiredPaths, []);
  assert.deepEqual(appliance.mounts, [
    { kind: 'bind', source: resolve(appDir, '..', '.spacetime-cli-config'),
      target: '/home/developer/.config/spacetime', readOnly: false },
    { kind: 'volume', source: 'stack-bench-release-deps', target: '/release-deps', readOnly: true },
  ]);
  assert.match(appliance.init, /test -x \/release-deps\/spacetimedb-cli/);
  assert.match(appliance.init, /mkdir -p \/deps/);
  assert.match(appliance.init, /test -f \/release-deps\/spacetimedb\.tgz/);
  assert.doesNotMatch(appliance.init, /npm install|npm pack/);
  assert.equal(appliance.networkNamespace, 'host');
  for (const id of ['postgres', 'mongodb', 'stub']) {
    const plan = buildPlan(executeStackCapability(STACK_ADAPTER_REGISTRY.get(id),
      'build-container', 'plan', { repo, appDir }));
    assert.deepEqual(plan.mounts, [], `${id} must not receive another stack's artifacts`);
    assert.deepEqual(plan.requiredPaths, []);
    assert.equal(plan.readyFile, null);
    assert.equal(plan.networkNamespace, null);
    const appliancePlan = buildPlan(executeStackCapability(STACK_ADAPTER_REGISTRY.get(id),
      'build-container', 'plan', { repo, appDir, env: { STACK_BENCH_APPLIANCE: '1' } }));
    assert.equal(appliancePlan.networkNamespace, id === 'stub' ? null : 'host');
  }
});

test('hosted stacks receive the exact leased database through the standard environment', () => {
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
    assert(sql, 'SpacetimeDB command must include SQL');
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

test('a new stack registers without changing engine code', () => {
  const stub = STACK_ADAPTER_REGISTRY.get('stub');
  const capabilities = Object.fromEntries(Object.entries(stub.capabilities).map(([name, provider]) =>
    [name, { ...provider, id: `deterministic-fake.${name}` }]));
  const stubPorts = capabilities.ports;
  assert(stubPorts, 'stub adapter must have ports');
  capabilities.ports = {
    ...stubPorts,
    execute: (operation: string, input: unknown) => {
      if (operation === 'allocations') return { vite: 7000 };
      const request = record(input, 'fake ports input');
      const trackOffset = request.trackOffset ?? 0;
      const runIndex = request.runIndex ?? 0;
      assert(typeof trackOffset === 'number');
      assert(typeof runIndex === 'number');
      return { vite: 7000 + trackOffset + runIndex, express: null, dbPort: null };
    },
  };
  const fake: StackAdapter = {
    schemaVersion: STACK_ADAPTER_SCHEMA_VERSION,
    id: 'deterministic-fake',
    version: '1.0.0',
    lifecycle: stub.lifecycle,
    capabilities,
  };
  const registry = createStackAdapterRegistry([...STACK_ADAPTER_REGISTRY.ids
    .map(id => STACK_ADAPTER_REGISTRY.get(id)), fake]);
  assert.equal(registry.get('deterministic-fake').version, '1.0.0');
  assert.deepEqual(executeStackCapability(registry.get('deterministic-fake'), 'ports', 'for-run',
    { trackOffset: 3, runIndex: 1 }), { vite: 7004, express: null, dbPort: null });
});

test('named action parameters map to HTTP paths/bodies and reducer argument order', () => {
  const buy = { id: 'buy', path: '/api/items/:item/buy', reducer: 'buy_now', args: [0],
    params: [{ name: 'itemId', in: 'path', placeholder: ':item' }] };
  const restock = { id: 'restock', path: '/api/admin/restock', reducer: 'admin_restock', args: [0, 0, 1],
    params: [{ name: 'itemId', in: 'body', wireType: 'u64' },
      { name: 'warehouseId', in: 'body', wireType: 'u64' },
      { name: 'quantity', in: 'body' }] };
  const http = namedActionRequest(executeStackCapability(STACK_ADAPTER_REGISTRY.get('postgres'),
    'named-action', 'request', { action: buy, input: { values: { itemId: 42 } },
      url: 'http://app.test/' }));
  assert.equal(http.url, 'http://app.test/api/items/42/buy');
  assert.equal(http.method, 'POST');
  assert.deepEqual(JSON.parse(http.body), {});
  const httpBody = namedActionRequest(executeStackCapability(STACK_ADAPTER_REGISTRY.get('mongodb'),
    'named-action', 'request', { action: restock,
      input: { values: { itemId: 'item', warehouseId: 'warehouse', quantity: 3 } },
      url: 'http://app.test' }));
  assert.deepEqual(JSON.parse(httpBody.body),
    { itemId: 'item', warehouseId: 'warehouse', quantity: 3 });
  const patched = namedActionRequest(executeStackCapability(STACK_ADAPTER_REGISTRY.get('postgres'),
    'named-action', 'request', { action: { ...restock, method: 'PATCH' },
      input: { values: { itemId: 'item', warehouseId: 'warehouse', quantity: 3 } },
      url: 'http://app.test' }));
  assert.equal(patched.method, 'PATCH');
  const spacetime = namedActionRequest(executeStackCapability(STACK_ADAPTER_REGISTRY.get('spacetime'),
    'named-action', 'request', { action: restock,
      input: { values: { itemId: 7, warehouseId: 9, quantity: 3 } },
      spacetime: { uri: 'http://stdb.test', mod: 'shop' } }));
  assert.equal(spacetime.url, 'http://stdb.test/v1/database/shop/call/admin_restock');
  assert.equal(spacetime.method, 'POST');
  assert.deepEqual(JSON.parse(spacetime.body), [7, 9, 3]);
  const spacetimeStringIds = namedActionRequest(executeStackCapability(
    STACK_ADAPTER_REGISTRY.get('spacetime'),
    'named-action', 'request', { action: restock,
      input: { values: { itemId: '7', warehouseId: '18446744073709551615', quantity: 3 } },
      spacetime: { uri: 'http://stdb.test', mod: 'shop' } }));
  assert.equal(spacetimeStringIds.body, '[7,18446744073709551615,3]');
  for (const itemId of ['-1', '18446744073709551616', '01']) {
    assert.throws(() => executeStackCapability(STACK_ADAPTER_REGISTRY.get('spacetime'),
      'named-action', 'request', { action: restock,
        input: { values: { itemId, warehouseId: '9', quantity: 3 } },
        spacetime: { uri: 'http://stdb.test', mod: 'shop' } }),
    /invalid u64 value for reducer parameter "itemId"/);
  }
  const spacetimeText = namedActionRequest(executeStackCapability(STACK_ADAPTER_REGISTRY.get('spacetime'),
    'named-action', 'request', { action: { id: 'lookup', path: '/lookup', reducer: 'lookup',
      args: [''], params: [{ name: 'code', in: 'body' }] },
      input: { values: { code: '007' } },
      spacetime: { uri: 'http://stdb.test', mod: 'shop' } }));
  assert.equal(spacetimeText.body, '["007"]');
});

test('unknown stacks, capabilities, operations, and malformed plugins fail closed', () => {
  assert.throws(() => STACK_ADAPTER_REGISTRY.get('new-db'), /unknown stack adapter/);
  const adapter = STACK_ADAPTER_REGISTRY.get('stub');
  assert.throws(() => executeStackCapability(adapter, 'diagnostics', 'capture'), /does not support capability diagnostics/);
  assert.throws(() => executeStackCapability(adapter, 'ports', 'guess'), /does not support operation guess/);
  assert.throws(() => createStackAdapterRegistry([{ schemaVersion: 1, id: 'bad', version: 'latest',
    capabilities: {} }]), /version is invalid/);
  assert.throws(() => createStackAdapterRegistry([{ schemaVersion: 1, id: 'thin', version: '1.0.0',
    lifecycle: { activate: () => {} },
    capabilities: { ports: { schemaVersion: STACK_CAPABILITY_SCHEMA_VERSION, id: 'thin.ports',
      version: '1.0.0', operations: ['allocations', 'for-run'], execute: () => ({ vite: 1 }) } } }]),
  /missing required capability lease/);
});
