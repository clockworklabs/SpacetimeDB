import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { createStackAdapterRegistry } from '../src/stacks/stack-adapter-contract.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { setSpacetimeStock } from '../src/stacks/backends/spacetime-operations.js';
import { describesMissingStockInterface } from '../src/stacks/stock-interface.js';

test('build plans expose only artifacts owned by the selected stack', () => {
  const appDir = resolve('bench', 'run', 'app');
  const repo = resolve('repo');
  const spacetime = STACK_ADAPTER_REGISTRY.get('spacetime').buildContainer.plan({ repo, appDir });
  assert.equal(spacetime.mounts.some(mount => mount.target === '/deps/.spacetimedb-cli'), true);
  assert.equal(spacetime.readyFile, '/deps/.ready');

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

  for (const adapter of [STACK_ADAPTER_REGISTRY.get('postgres'),
    STACK_ADAPTER_REGISTRY.get('mongodb'), STACK_ADAPTER_REGISTRY.get('stub')]) {
    const plan = adapter.buildContainer.plan({ env: {} });
    assert.deepEqual(plan.mounts, [], `${adapter.id} must not receive another stack's artifacts`);
    assert.deepEqual(plan.requiredPaths, []);
    assert.equal(plan.readyFile, null);
  }
});

test('a stock write that finds no table, column, or row is the application missing its interface', () => {
  for (const detail of [
    'Error: `stock` does not have a field `quantity`',
    'Error: no such table: stock',
    'table stock is marked private',
    'ERROR:  column "quantity" of relation "stock" does not exist',
    'ERROR:  relation "stock" does not exist',
    'WARNING: This command is UNSTABLE.\n\nError: `id` is not in scope\n\nCaused by:\n    HTTP status client error (400 Bad Request)',
    'Error: `item_id` is not in scope\r\n',
    'Error: `warehouse_id` is not in scope',
    'Table stock not found',
    'relation "stock" does not exist',
    'no such column: quantity',
    'field item_id not found',
  ]) assert.ok(describesMissingStockInterface(detail), detail);
  for (const detail of [
    'connection refused', 'ETIMEDOUT', 'HTTP status server error (500 Internal Server Error)',
    'HTTP status client error (400 Bad Request)',
    'Error: `unrelated_field` is not in scope',
    'Error: syntax error near `id`',
    'transport failed while running query: `id` is not in scope',
    'OCI runtime exec failed: executable file not found in $PATH',
    'FATAL: role "appuser" does not exist',
    'FATAL: database "bench" does not exist',
  ]) assert.equal(describesMissingStockInterface(detail), false, detail);

  const exec = (_command: string, args: readonly string[]): string => {
    if (args[0] === 'inspect') return 'leased-build-id';
    const sql = args.at(-1) ?? '';
    if (/select id from item/.test(sql)) return 'id\n---\n1\n';
    if (/select id from warehouse/.test(sql)) return 'id\n---\n2\n';
    throw Object.assign(new Error('exit 1'), { status: 1,
      stderr: 'Error: `stock` does not have a field `quantity`\n' });
  };
  assert.throws(() => setSpacetimeStock({ item: 'widget', warehouse: 'east', quantity: 3,
    spacetime: { buildContainer: { name: 'leased-build', id: 'leased-build-id' }, mod: 'shop',
      containerUri: 'http://host.docker.internal:3000' }, exec }),
  (error: unknown) => error instanceof Error && 'stockInterface' in error
    && error.stockInterface === true && /does not have a field/.test(error.message));

  assert.throws(() => setSpacetimeStock({ item: 'widget', warehouse: 'east', quantity: 3,
    spacetime: { buildContainer: { name: 'leased-build', id: 'leased-build-id' }, mod: 'shop',
      containerUri: 'http://host.docker.internal:3000' },
    exec: (_command, args) => args[0] === 'inspect' ? 'leased-build-id'
      : /select id from item/.test(args.at(-1) ?? '') ? 'id\n---\n1\n' : 'id\n---\n' }),
  (error: unknown) => error instanceof Error && 'missingRow' in error && error.missingRow === 'warehouse');

  for (const [stderr, expectedInterface] of [
    ['Error: `id` is not in scope\n', true],
    ['Error: `unrelated_field` is not in scope\n', false],
    ['connection refused', false],
  ] as const) {
    assert.throws(() => setSpacetimeStock({ item: 'widget', warehouse: 'east', quantity: 3,
      spacetime: { buildContainer: { name: 'leased-build', id: 'leased-build-id' }, mod: 'shop',
        containerUri: 'http://host.docker.internal:3000' },
      exec: (_command, args) => {
        if (args[0] === 'inspect') return 'leased-build-id';
        throw Object.assign(new Error('exit 1'), { stderr });
      } }),
    (error: unknown) => error instanceof Error
      && ('stockInterface' in error && error.stockInterface === true) === expectedInterface);
  }
});

test('Spacetime stock writes separate missing rows from failed writes and malformed evidence', () => {
  for (const [rows, expectedInterface] of [
    ['warehouse_id | quantity\n---+---\n', true],
    ['invalid output', false],
    ['warehouse_id | quantity\n---+---\n2 | 100\n', false],
  ] as const) {
    let writes = 0;
    assert.throws(() => setSpacetimeStock({ item: 'widget', warehouse: 'east', quantity: 3,
      spacetime: { buildContainer: { name: 'leased-build', id: 'leased-build-id' }, mod: 'shop',
        containerUri: 'http://localhost:3000' }, exec: (_command, args) => {
        const sql = args.at(-1) ?? '';
        if (args[0] === 'inspect') return 'leased-build-id';
        if (/select id from item/.test(sql)) return 'id\n---\n1\n';
        if (/select id from warehouse/.test(sql)) return 'id\n---\n2\n';
        if (/select warehouse_id, quantity/.test(sql)) return rows;
        if (/^update stock/.test(sql)) { writes++; return ''; }
        return 'quantity\n---\n100\n';
      } }), error => error instanceof Error
        && ('stockInterface' in error && error.stockInterface === true) === expectedInterface);
    assert.equal(writes, rows.includes('2 | 100') ? 1 : 0);
  }

  const calls: Array<[string, readonly string[]]> = [];
  setSpacetimeStock({ item: 'widget', warehouse: 'east', quantity: 3,
    spacetime: { buildContainer: { name: 'leased-build', id: 'leased-build-id' }, mod: 'shop',
      containerUri: 'http://host.docker.internal:3000' }, exec: (command, args) => {
      calls.push([command, args]);
      if (args[0] === 'inspect') return 'leased-build-id';
      const sql = args.at(-1) ?? '';
      if (/select id from item/.test(sql)) return 'id\n---\n1\n';
      if (/select id from warehouse/.test(sql)) return 'id\n---\n2\n';
      if (/select warehouse_id, quantity/.test(sql)) return 'warehouse_id | quantity\n---+---\n2 | 3\n';
      if (/select quantity/.test(sql)) return '3\n';
      return '';
    } });
  assert(calls.some(([, args]) => /^update stock/.test(args.at(-1) ?? '')));
  for (const [command, args] of calls.filter(([, args]) => args[0] !== 'inspect')) {
    assert.equal(command, 'docker');
    assert.deepEqual(args.slice(0, 8), ['exec', '--user', '10001:10001', '-e',
      'HOME=/home/developer', '-e', 'USER=developer', 'leased-build-id']);
  }
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
