import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { createStackAdapterRegistry, executeStackCapability,
  STACK_ADAPTER_SCHEMA_VERSION, STACK_CAPABILITY_SCHEMA_VERSION } from '../stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from '../stack-adapters.mjs';

test('built-in and deterministic test stack adapters preserve the proven port grid', () => {
  assert.deepEqual(STACK_ADAPTER_REGISTRY.ids, ['mongodb', 'postgres', 'spacetime', 'stub']);
  const postgres = STACK_ADAPTER_REGISTRY.get('postgres');
  assert.deepEqual(executeStackCapability(postgres, 'ports', 'for-run',
    { trackOffset: 100, runIndex: 2 }), { vite: 6375, express: 6103, dbPort: 6532 });
  const prepared = executeStackCapability(postgres, 'lease', 'prepare', {
    track: { name: 'shop' }, runIndex: 2, env: {},
    helpers: {
      dbName: (track, runIndex) => `${track.name}-${runIndex}`,
      containerIdentity: name => ({ name, id: 'container-id' }),
    },
  });
  assert.deepEqual(prepared, {
    lease: { serverUri: null, database: 'shop-2', module: null, dataDir: null,
      container: { name: 'stack-bench-postgres', id: 'container-id' } },
    lockKeys: [],
  });
  assert.equal(postgres.capabilities.reset.operations.includes('run'), true);
  assert.equal(postgres.capabilities['database-write'].operations.includes('set-stock'), true);
});

test('build-container plans expose only artifacts owned by the selected stack', () => {
  const appDir = resolve('bench', 'run', 'app');
  const repo = resolve('repo');
  const spacetime = executeStackCapability(STACK_ADAPTER_REGISTRY.get('spacetime'),
    'build-container', 'plan', { repo, appDir });
  assert.equal(spacetime.mounts.some(mount => mount.target === '/deps/spacetimedb-cli'), true);
  assert.equal(spacetime.mounts.some(mount => mount.target === '/root/.config/spacetime'), true);
  assert.equal(spacetime.readyFile, '/deps/.ready');
  const appliance = executeStackCapability(STACK_ADAPTER_REGISTRY.get('spacetime'),
    'build-container', 'plan', {
      repo, appDir, env: { STACK_BENCH_RELEASE_DEPS_VOLUME: 'stack-bench-release-deps' },
    });
  assert.deepEqual(appliance.requiredPaths, []);
  assert.deepEqual(appliance.mounts, [
    { kind: 'bind', source: resolve(appDir, '..', '.spacetime-cli-config'),
      target: '/root/.config/spacetime', readOnly: false },
    { kind: 'volume', source: 'stack-bench-release-deps', target: '/release-deps', readOnly: true },
  ]);
  assert.match(appliance.init, /test -x \/release-deps\/spacetimedb-cli/);
  for (const id of ['postgres', 'mongodb', 'stub']) {
    const plan = executeStackCapability(STACK_ADAPTER_REGISTRY.get(id),
      'build-container', 'plan', { repo, appDir });
    assert.deepEqual(plan.mounts, [], `${id} must not receive another stack's artifacts`);
    assert.deepEqual(plan.requiredPaths, []);
    assert.equal(plan.readyFile, null);
  }
});

test('a new stack registers without changing engine code', () => {
  const stub = STACK_ADAPTER_REGISTRY.get('stub');
  const fake = {
    schemaVersion: STACK_ADAPTER_SCHEMA_VERSION,
    id: 'deterministic-fake',
    version: '1.0.0',
    capabilities: Object.fromEntries(Object.entries(stub.capabilities).map(([name, provider]) =>
      [name, { ...provider, id: `deterministic-fake.${name}` }])),
  };
  fake.capabilities.ports = {
    ...fake.capabilities.ports,
    execute: (operation, { trackOffset = 0, runIndex = 0 } = {}) => operation === 'allocations'
      ? { vite: 7000 }
      : { vite: 7000 + trackOffset + runIndex, express: null, dbPort: null },
  };
  const registry = createStackAdapterRegistry([...STACK_ADAPTER_REGISTRY.ids
    .map(id => STACK_ADAPTER_REGISTRY.get(id)), fake]);
  assert.equal(registry.get('deterministic-fake').version, '1.0.0');
  assert.deepEqual(executeStackCapability(registry.get('deterministic-fake'), 'ports', 'for-run',
    { trackOffset: 3, runIndex: 1 }), { vite: 7004, express: null, dbPort: null });
});

test('unknown stacks, capabilities, operations, and malformed plugins fail closed', () => {
  assert.throws(() => STACK_ADAPTER_REGISTRY.get('new-db'), /unknown stack adapter/);
  const adapter = STACK_ADAPTER_REGISTRY.get('stub');
  assert.throws(() => executeStackCapability(adapter, 'diagnostics', 'capture'), /does not support capability diagnostics/);
  assert.throws(() => executeStackCapability(adapter, 'ports', 'guess'), /does not support operation guess/);
  assert.throws(() => createStackAdapterRegistry([{ schemaVersion: 1, id: 'bad', version: 'latest',
    capabilities: {} }]), /version is invalid/);
  assert.throws(() => createStackAdapterRegistry([{ schemaVersion: 1, id: 'thin', version: '1.0.0',
    capabilities: { ports: { schemaVersion: STACK_CAPABILITY_SCHEMA_VERSION, id: 'thin.ports',
      version: '1.0.0', operations: ['allocations', 'for-run'], execute: () => ({ vite: 1 }) } } }]),
  /missing required capability lease/);
});
