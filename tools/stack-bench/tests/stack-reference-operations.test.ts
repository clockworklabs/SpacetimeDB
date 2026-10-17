import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

import { deployMongoDbReference, deployPostgresReference, deploySpacetimeReference }
  from '../src/stacks/stack-reference-operations.js';
import type { ReferenceDeployInput, ReferenceHelpers }
  from '../src/stacks/stack-reference-operations.js';
import { loadTrack } from '../src/composition/tracks.js';
import { attemptDatabaseIdentity } from '../src/stacks/hosted-database-identity.js';

const unused = () => { throw new Error('not used by this deployment'); };
const helpersWith = (overrides: Partial<ReferenceHelpers>): ReferenceHelpers => ({
  phase() {}, docker() {}, startDetached() {}, async waitFor() {}, containerLogs() { return ''; },
  runSync: unused, dbName: unused, moduleName: unused, loadTrack: unused, ...overrides,
});
// Tests give only the lease fields each deployment reads.
const leaseOf = (resources: object, extra: object = {}) =>
  ({ resources, ...extra }) as unknown as ReferenceDeployInput['lease'];

test('PostgreSQL reference starts its schema through the normal startup path', async () => {
  const dockerCalls: Array<Parameters<ReferenceHelpers['docker']>> = [];
  const starts: Array<Parameters<ReferenceHelpers['startDetached']>> = [];
  const commands: Array<readonly string[]> = [];
  const helpers = helpersWith({
    dbName() { return 'app_ecom_run0'; },
    runSync(_label, _command, args) {
      commands.push(args);
      return args[0] === 'inspect' ? 'container-id\n' : '';
    },
    docker(...args) { dockerCalls.push(args); },
    startDetached(...args) { starts.push(args); },
  });
  await deployPostgresReference({
    args: { backend: 'postgres', track: 'ecommerce', runIndex: 0 },
    metadata: { installDirectories: [], server: { directory: 'server' },
      client: { directory: 'client' } },
    lease: leaseOf({ database: 'app_ecom_run0',
      container: { name: 'postgres', id: 'container-id' } }),
    track: { slug: 'ecommerce', restartProbe: '/api/items' }, container: 'build-0',
    ports: { dbPort: 6532, vite: 6573, express: null }, buildNetworkMode: 'host', helpers,
  });

  assert(commands.some(args => args.includes('dropdb') && args.at(-1) === 'app_ecom_run0'));
  assert(commands.some(args => args.includes('createdb') && args.at(-1) === 'app_ecom_run0'));
  assert.deepEqual(dockerCalls.map(call => call.slice(0, 4)), [
    ['build-0', '/app/client', 'npm', ['run', 'build']],
  ]);
  assert.equal(starts[0]?.[4]?.script, 'start');
  const serverPackage = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'reference-apps/ecommerce/postgres/server/package.json'), 'utf8'));
  assert.equal(serverPackage.scripts.prestart, undefined, 'restarts must not push over extension data');
  const serverSource = readFileSync(join(STACK_BENCH_ROOT,
    'reference-apps/ecommerce/postgres/server/src/index.ts'), 'utf8');
  const initialize = serverSource.indexOf('await initializeCoreSchema()');
  assert(initialize >= 0 && initialize < serverSource.indexOf('httpServer.listen('));
  assert(serverPackage.devDependencies['drizzle-kit']);

});

test('hosted reference credentials stay in process environment', async () => {
  const starts: Array<Parameters<ReferenceHelpers['startDetached']>> = [];
  const commands: Array<readonly string[]> = [];
  const ownershipToken = 'reference-mongodb-authority';
  const helpers = helpersWith({
    dbName() { return 'app_ecom_run0'; },
    runSync(_label, _command, args) {
      commands.push(args);
      return args[0] === 'inspect' ? 'container-id\n' : '';
    },
    startDetached(...args) { starts.push(args); },
  });
  await deployMongoDbReference({
    args: { backend: 'mongodb', track: 'ecommerce', runIndex: 0 },
    metadata: { installDirectories: [], server: { directory: 'server' },
      client: { directory: 'client' } },
    lease: leaseOf({ database: 'app_ecom_run0',
      container: { name: 'mongodb', id: 'container-id' },
      network: { name: 'attempt', id: 'a'.repeat(64), namespaceContainerId: 'b'.repeat(64),
        hostAddresses: [], services: [], firewallSha256: null, firewallInstalledAt: null } }, { ownershipToken }),
    track: { slug: 'ecommerce', restartProbe: '' }, container: 'build-0',
    ports: { dbPort: 6537, vite: 6723, express: null },
    buildNetworkMode: 'host', helpers,
  });
  const applicationStart = starts[0];
  assert(applicationStart);
  assert.match(applicationStart[3].DATABASE_URL ?? '', /app_ecom_run0/);
  assert.deepEqual(applicationStart.slice(0, 3), ['build-0', '/app', 'reference-application']);
  assert.equal(applicationStart[3].PORT, '6723');
  assert.equal(applicationStart[3].JWT_SECRET, 'stack-bench-reference-only-secret-2026');
  assert.deepEqual(applicationStart[4], { script: 'start' });
  const identity = attemptDatabaseIdentity(ownershipToken);
  const reset = commands.find(args => args.includes('db.dropDatabase()'));
  assert(reset);
  assert.deepEqual(reset.slice(0, 4), ['exec', 'container-id', 'mongosh', 'app_ecom_run0']);
  assert.equal(reset[reset.indexOf('--username') + 1], 'admin');
  assert.equal(reset[reset.indexOf('--password') + 1], identity.adminPassword);
  assert.equal(reset[reset.indexOf('--authenticationDatabase') + 1], 'admin');
  assert(applicationStart[3].DATABASE_URL?.includes(identity.password));
  assert(!JSON.stringify(starts).includes(identity.adminPassword));
});

test('Spacetime reference client uses its assigned Vite port', async () => {
  const starts: Array<Parameters<ReferenceHelpers['startDetached']>> = [];
  const waits: Array<Parameters<ReferenceHelpers['waitFor']>> = [];
  const helpers = helpersWith({
    loadTrack() { return loadTrack('ecommerce'); },
    moduleName() { return 'ecommerce_42'; },
    startDetached(...args) { starts.push(args); },
    async waitFor(...args) { waits.push(args); },
  });

  await deploySpacetimeReference({
    args: { backend: 'spacetime', track: 'ecommerce', runIndex: 42 },
    metadata: {
      kind: 'spacetime',
      installDirectories: [],
      moduleDirectory: 'backend/spacetimedb',
      bindingsDirectory: 'client/src/module_bindings',
      client: { directory: 'client' },
    },
    lease: leaseOf({
      module: 'ecommerce_42',
      serverUri: 'ws://host.docker.internal:3315',
    }),
    track: { slug: 'ecommerce', restartProbe: '' },
    container: 'build-42',
    ports: { vite: 6475, express: null, dbPort: null },
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
