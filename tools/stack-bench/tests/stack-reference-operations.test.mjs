import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { deployMongoDbReference, deployPostgresReference, deploySpacetimeReference }
  from '../dist/src/stacks/stack-reference-operations.mjs';

test('PostgreSQL reference deployment uses its locked schema tool', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-postgres-reference-'));
  const app = join(root, 'app');
  mkdirSync(join(app, 'server'), { recursive: true });
  writeFileSync(join(app, 'server', 'package.json'), JSON.stringify({ scripts: { start: 'node server.js' } }));
  const dockerCalls = [];
  const helpers = {
    dbName() { return 'app_ecom_run0'; },
    runSync(label) {
      return label === 'inspecting leased database container' ? 'container-id\n' : '';
    },
    docker(...args) { dockerCalls.push(args); },
    phase() {},
    startDetached() {},
    async waitFor() {},
    containerLogs() { return ''; },
  };
  try {
    await deployPostgresReference({
      args: { app, backend: 'postgres', runIndex: 0 },
      metadata: { installDirectories: [], server: { directory: 'server' },
        client: { directory: 'client' } },
      lease: { resources: { database: 'app_ecom_run0',
        container: { name: 'postgres', id: 'container-id' } } },
      track: { restartProbe: '/api/items' }, container: 'build-0',
      ports: { dbPort: 6532, express: 6301, vite: 6573 }, buildNetworkMode: 'host', helpers,
    });

    assert.equal(dockerCalls.length, 1);
    assert.deepEqual(dockerCalls[0].slice(0, 4), [
      'build-0', '/app/server', './node_modules/.bin/drizzle-kit', ['push', '--force'],
    ]);
    assert.match(dockerCalls[0][4].DATABASE_URL, /app_ecom_run0/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('hosted reference credentials stay in process environment', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-environment-'));
  try {
    const app = join(root, 'app');
    mkdirSync(join(app, 'server'), { recursive: true });
    writeFileSync(join(app, 'server', 'package.json'), JSON.stringify({ scripts: { start: 'node server.js' } }));
    const starts = [];
    const helpers = {
      dbName() { return 'app_ecom_run0'; },
      runSync(label) {
        return label === 'inspecting leased database container' ? 'container-id\n' : '';
      },
      docker() {},
      phase() {},
      startDetached(...args) { starts.push(args); },
      async waitFor() {},
      containerLogs() { return ''; },
    };
    await deployMongoDbReference({
      args: { app, backend: 'mongodb', runIndex: 0 },
      metadata: { installDirectories: [], server: { directory: 'server' },
        client: { directory: 'client' } },
      lease: { resources: { database: 'app_ecom_run0',
        container: { name: 'mongodb', id: 'container-id' } } },
      track: {}, container: 'build-0', ports: { dbPort: 6537, express: 6401, vite: 6673 },
      buildNetworkMode: 'host', helpers,
    });
    assert.equal(existsSync(join(app, 'server', '.env')), false);
    assert.match(starts[0][3].DATABASE_URL, /app_ecom_run0/);
    assert.equal(starts[0][3].PORT, '6401');
    assert.equal(starts[0][3].JWT_SECRET, 'stack-bench-reference-only-secret-2026');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('Spacetime reference client uses its assigned Vite port', async () => {
  const starts = [];
  const waits = [];
  const helpers = {
    docker() {},
    loadTrack() { return { id: 'ecommerce' }; },
    moduleName() { return 'ecommerce_42'; },
    startDetached(...args) { starts.push(args); },
    async waitFor(...args) { waits.push(args); },
    containerLogs() { return ''; },
  };

  await deploySpacetimeReference({
    args: { track: 'ecommerce', runIndex: 42 },
    metadata: {
      installDirectories: [],
      moduleDirectory: 'backend/spacetimedb',
      bindingsDirectory: 'client/src/module_bindings',
      client: { directory: 'client' },
    },
    lease: { resources: {
      module: 'ecommerce_42',
      serverUri: 'ws://host.docker.internal:3315',
    } },
    container: 'build-42',
    ports: { vite: 6475 },
    buildNetworkMode: 'bridge',
    helpers,
  });

  assert.equal(starts.length, 1);
  assert.deepEqual(starts[0].slice(0, 3), ['build-42', '/app/client', 'reference-client']);
  assert.equal(starts[0][3].VITE_PORT, '6475');
  assert.deepEqual(starts[0][4], { networkVisible: true, port: 6475 });
  assert.equal(waits[0][0], 'http://127.0.0.1:6475');
});
