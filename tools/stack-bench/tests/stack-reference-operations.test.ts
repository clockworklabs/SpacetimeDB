import assert from 'node:assert/strict';
import test from 'node:test';

import { deployMongoDbReference, deployPostgresReference, deploySpacetimeReference }
  from '../src/stacks/stack-reference-operations.js';
import type { HostedReferenceHelpers, SpacetimeReferenceHelpers }
  from '../src/stacks/stack-reference-operations.js';
import { loadTrack } from '../src/composition/tracks.js';

test('PostgreSQL reference deployment uses its locked schema tool', async () => {
  const dockerCalls: Array<Parameters<HostedReferenceHelpers['docker']>> = [];
  const helpers: HostedReferenceHelpers = {
    dbName() { return 'app_ecom_run0'; },
    runSync(label: string) {
      return label === 'inspecting leased database container' ? 'container-id\n' : '';
    },
    docker(...args) { dockerCalls.push(args); },
    phase() {},
    startDetached() {},
    async waitFor() {},
    containerLogs() { return ''; },
  };
  await deployPostgresReference({
    args: { backend: 'postgres', runIndex: 0 },
    metadata: { installDirectories: [], server: { directory: 'server' },
      client: { directory: 'client' } },
    lease: { resources: { database: 'app_ecom_run0',
      container: { name: 'postgres', id: 'container-id' } } },
    track: { slug: 'ecommerce', restartProbe: '/api/items' }, container: 'build-0',
    ports: { dbPort: 6532, vite: 6573 }, buildNetworkMode: 'host', helpers,
  });

  assert.equal(dockerCalls.length, 2);
  const dockerCall = dockerCalls.find(call => call[2] === './node_modules/.bin/drizzle-kit');
  assert(dockerCall);
  assert.deepEqual(dockerCall.slice(0, 4), [
    'build-0', '/app/server', './node_modules/.bin/drizzle-kit', ['push', '--force'],
  ]);
  assert.match(dockerCall[4]?.DATABASE_URL ?? '', /app_ecom_run0/);
  assert.deepEqual(dockerCalls[1]?.slice(0, 4), [
    'build-0', '/app/client', 'npm', ['run', 'build'],
  ]);
});

test('hosted reference credentials stay in process environment', async () => {
  const starts: Array<Parameters<HostedReferenceHelpers['startDetached']>> = [];
  const helpers: HostedReferenceHelpers = {
    dbName() { return 'app_ecom_run0'; },
    runSync(label: string) {
      return label === 'inspecting leased database container' ? 'container-id\n' : '';
    },
    docker() {},
    phase() {},
    startDetached(...args) { starts.push(args); },
    async waitFor() {},
    containerLogs() { return ''; },
  };
  await deployMongoDbReference({
    args: { backend: 'mongodb', runIndex: 0 },
    metadata: { installDirectories: [], server: { directory: 'server' },
      client: { directory: 'client' } },
    lease: { resources: { database: 'app_ecom_run0',
      container: { name: 'mongodb', id: 'container-id' } } },
    track: { slug: 'ecommerce', restartProbe: '' }, container: 'build-0',
    ports: { dbPort: 6537, vite: 6673 },
    buildNetworkMode: 'host', helpers,
  });
  const applicationStart = starts[0];
  assert(applicationStart);
  assert.match(applicationStart[3].DATABASE_URL ?? '', /app_ecom_run0/);
  assert.deepEqual(applicationStart.slice(0, 3), ['build-0', '/app', 'reference-application']);
  assert.equal(applicationStart[3].PORT, '6673');
  assert.equal(applicationStart[3].JWT_SECRET, 'stack-bench-reference-only-secret-2026');
  assert.deepEqual(applicationStart[4], { script: 'start' });
});

test('Spacetime reference client uses its assigned Vite port', async () => {
  const starts: Array<Parameters<SpacetimeReferenceHelpers['startDetached']>> = [];
  const waits: Array<Parameters<SpacetimeReferenceHelpers['waitFor']>> = [];
  const helpers: SpacetimeReferenceHelpers = {
    docker() {},
    loadTrack() { return loadTrack('ecommerce'); },
    moduleName() { return 'ecommerce_42'; },
    startDetached(...args) { starts.push(args); },
    async waitFor(...args) { waits.push(args); },
    containerLogs() { return ''; },
  };

  await deploySpacetimeReference({
    args: { track: 'ecommerce', runIndex: 42 },
    metadata: {
      kind: 'spacetime',
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
  const clientStart = starts[0];
  const clientWait = waits[0];
  assert(clientStart);
  assert(clientWait);
  assert.deepEqual(clientStart.slice(0, 3), ['build-42', '/app/client', 'reference-client']);
  assert.equal(clientStart[3].VITE_PORT, '6475');
  assert.deepEqual(clientStart[4], { networkVisible: true, port: 6475 });
  assert.equal(clientWait[0], 'http://127.0.0.1:6475');
});
