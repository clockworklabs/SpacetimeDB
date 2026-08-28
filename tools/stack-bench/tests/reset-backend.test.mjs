import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.mjs';
import { resetBackend } from '../src/stacks/backend-reset.mjs';
import { proveMongoDbUse, provePostgresUse,
  resetPostgres } from '../src/stacks/stack-backend-operations.mjs';
import { containerReachableSpacetimeUri } from '../src/runtime/spacetime-target.mjs';
import { GeneratedAppLayoutError, resolveSpacetimeModuleLayout } from '../src/runtime/spacetime-layout.mjs';
import { POSTGRES_APPLICATION_IDENTITY } from '../src/stacks/hosted-database-identity.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const POSTGRES_USER = POSTGRES_APPLICATION_IDENTITY.user;

function writeModule(directory) {
  mkdirSync(join(directory, 'src'), { recursive: true });
  writeFileSync(join(directory, 'package.json'), JSON.stringify({
    dependencies: { spacetimedb: 'file:/deps/bindings-typescript' },
  }));
  writeFileSync(join(directory, 'src', 'index.ts'),
    "import { schema } from 'spacetimedb/server';\nexport default schema({});\n");
}

test('Spacetime container targets follow their recorded network topology', () => {
  const lease = { resources: { serverUri: 'http://127.0.0.1:3310', buildContainer: null } };
  assert.equal(containerReachableSpacetimeUri(lease), 'http://host.docker.internal:3310');
  lease.resources.buildContainer = { networkMode: 'host' };
  assert.equal(containerReachableSpacetimeUri(lease), 'http://127.0.0.1:3310');
  assert.throws(() => containerReachableSpacetimeUri(lease, 'unknown'),
    /unsupported build container network mode/);
});

test('the Node reset entrypoint refuses without an authenticated lease', () => {
  const env = { ...process.env };
  delete env.STACK_BENCH_LEASE;
  delete env.STACK_BENCH_LEASE_TOKEN;
  assert.throws(() => execFileSync(process.execPath,
    [join(ROOT, 'commands', 'reset-backend.mjs'), 'mongodb', '.'], { env, stdio: 'pipe' }),
  /STACK_BENCH_LEASE is required/);
});

test('the reset entrypoint reports a generated layout separately from a harness failure', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reset-layout-exit-'));
  const app = join(root, 'app');
  const leasePath = join(root, 'lease.json');
  mkdirSync(app);
  const lease = createBackendLease({ runId: 'layout-exit', backend: 'spacetime', track: 'ecommerce',
    runIndex: 0, serverUri: 'http://127.0.0.1:3310', module: 'app-ecom-run0',
    dataDir: join(root, 'data') });
  lease.state = 'active';
  writeBackendLease(leasePath, lease);
  try {
    const result = spawnSync(process.execPath,
      [join(ROOT, 'commands', 'reset-backend.mjs'), 'spacetime', app], {
        encoding: 'utf8',
        env: { ...process.env, STACK_BENCH_LEASE: leasePath,
          STACK_BENCH_LEASE_TOKEN: lease.ownershipToken },
      });
    assert.equal(result.status, 10);
    assert.match(result.stderr, /^GENERATED_APP_LAYOUT:/);
    assert.doesNotMatch(result.stderr, /stack-backend-operations|at file:/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('PostgreSQL per-check reset clears rows without removing the app schema', () => {
  const lease = {
    resources: {
      database: 'app_ecom_run0',
      container: { name: 'postgres-service', id: 'a'.repeat(64) },
    },
  };
  const calls = [];
  const exec = (command, args, options) => {
    calls.push({ command, args, options });
    if (args[0] === 'inspect') return `${lease.resources.container.id}\n`;
    return '';
  };

  resetPostgres({ lease, exec });
  const reset = calls.find(call => call.args.includes('psql'));
  const sql = reset.args[reset.args.indexOf('-c') + 1];
  assert.match(sql, /FROM pg_tables WHERE schemaname = 'public'/);
  assert.match(sql, /TRUNCATE TABLE/);
  assert.match(sql, /RESTART IDENTITY CASCADE/);
  assert.doesNotMatch(sql, /DROP SCHEMA/);
  assert.equal(reset.args[reset.args.indexOf('-v') + 1], 'ON_ERROR_STOP=1');
  assert.equal(reset.args[reset.args.indexOf('-d') + 1], lease.resources.database);
});

test('PostgreSQL runtime provenance finds an application marker in the exact leased database', () => {
  const lease = { resources: { database: 'app_ecom_run0',
    container: { name: 'postgres-service', id: 'a'.repeat(64) } } };
  const calls = [];
  const exec = (command, args, options) => {
    calls.push({ command, args, options });
    if (args[0] === 'inspect') return `${lease.resources.container.id}\n`;
    return 'public.account.username\n';
  };

  assert.deepEqual(provePostgresUse({ lease, marker: "ann'proof", exec }), {
    ok: true,
    verified: true,
    matches: 1,
    reason: 'the application marker exists in the leased PostgreSQL database',
  });
  const proof = calls.at(-1);
  assert.deepEqual(proof.args.slice(0, 3), ['exec', '-i', 'postgres-service']);
  assert.equal(proof.args.includes('ON_ERROR_STOP=1'), true);
  assert.match(proof.options.input, /FROM information_schema\.columns/);
  assert.match(proof.options.input, /ann''proof/);
  assert.match(proof.options.input, /ORDER BY table_name, ordinal_position\n\\gexec/);

  const emptyExec = (command, args) => args[0] === 'inspect'
    ? `${lease.resources.container.id}\n` : '';
  assert.equal(provePostgresUse({ lease, marker: 'missing', exec: emptyExec }).ok, false);
  assert.throws(() => provePostgresUse({ lease, marker: '', exec }),
    /requires a non-empty application marker/);
});

test('MongoDB runtime provenance finds an application marker in the exact leased database', () => {
  const lease = { resources: { database: 'app_ecom_run0',
    container: { name: 'mongodb-service', id: 'b'.repeat(64) } } };
  const calls = [];
  const execWith = output => (command, args) => {
    calls.push({ command, args });
    return args[0] === 'inspect' ? `${lease.resources.container.id}\n` : output;
  };

  assert.deepEqual(proveMongoDbUse({ lease, marker: 'ann-proof', exec: execWith('2\n') }), {
    ok: true,
    verified: true,
    matches: 2,
    reason: 'the application marker exists in the leased MongoDB database',
  });
  const script = calls.at(-1).args.at(-1);
  assert.match(script, /const marker = "ann-proof"/);
  assert.match(script, /containsMarker/);
  assert.equal(proveMongoDbUse({ lease, marker: 'missing', exec: execWith('0\n') }).ok, false);
  assert.throws(() => proveMongoDbUse({ lease, marker: 'proof', exec: execWith('not-a-count\n') }),
    /invalid count/);
  assert.throws(() => proveMongoDbUse({ lease, marker: '', exec: execWith('0\n') }),
    /requires a non-empty application marker/);
});

test('runtime database provenance works against the Docker services', {
  skip: process.env.STACK_BENCH_DATABASE_PROVENANCE_SMOKE !== '1',
}, () => {
  const suffix = `${process.pid}_${Date.now()}`;
  const marker = `application-proof-${suffix}`;
  const postgresDatabase = `application_proof_${suffix}`;
  const mongoDatabase = `application_proof_${suffix}`;
  const docker = (args, options = {}) => execFileSync('docker', args,
    { encoding: 'utf8', stdio: 'pipe', timeout: 120_000, ...options });
  const container = name => ({ name,
    id: docker(['inspect', '--format', '{{.Id}}', name]).trim() });
  try {
    docker(['exec', 'stack-bench-postgres', 'psql', '-U', POSTGRES_USER, '-d', 'postgres',
      '-v', 'ON_ERROR_STOP=1', '-c', `CREATE DATABASE ${postgresDatabase} OWNER ${POSTGRES_USER};`]);
    docker(['exec', 'stack-bench-postgres', 'psql', '-U', POSTGRES_USER, '-d', postgresDatabase,
      '-v', 'ON_ERROR_STOP=1', '-c',
      `CREATE TABLE account (username text NOT NULL); INSERT INTO account VALUES ('${marker}');`]);
    docker(['exec', 'stack-bench-mongodb', 'mongosh', mongoDatabase, '--quiet', '--eval',
      `db.account.insertOne({ username: ${JSON.stringify(marker)} })`]);

    assert.equal(provePostgresUse({ lease: { resources: { database: postgresDatabase,
      container: container('stack-bench-postgres') } }, marker }).ok, true);
    assert.equal(proveMongoDbUse({ lease: { resources: { database: mongoDatabase,
      container: container('stack-bench-mongodb') } }, marker }).ok, true);
  } finally {
    try {
      docker(['exec', 'stack-bench-postgres', 'psql', '-U', POSTGRES_USER, '-d', 'postgres',
        '-v', 'ON_ERROR_STOP=1', '-c', `DROP DATABASE IF EXISTS ${postgresDatabase} WITH (FORCE);`]);
    } catch { /* keep the original test failure */ }
    try {
      docker(['exec', 'stack-bench-mongodb', 'mongosh', mongoDatabase, '--quiet', '--eval',
        'db.dropDatabase()']);
    } catch { /* keep the original test failure */ }
  }
});

test('PostgreSQL reset preserves schema and does not touch another lease database', {
  skip: process.env.STACK_BENCH_POSTGRES_RESET_SMOKE !== '1',
}, () => {
  const suffix = `${process.pid}_${Date.now()}`;
  const target = `application_reset_${suffix}`;
  const neighbor = `application_neighbor_${suffix}`;
  const container = 'stack-bench-postgres';
  const docker = (args, options = {}) => execFileSync('docker', args,
    { encoding: 'utf8', stdio: 'pipe', timeout: 120_000, ...options });
  const database = (name, sql) => docker(['exec', container, 'psql', '-U', POSTGRES_USER,
    '-d', name, '-v', 'ON_ERROR_STOP=1', '-tAc', sql]);
  try {
    const id = docker(['inspect', '--format', '{{.Id}}', container]).trim();
    for (const name of [target, neighbor]) {
      docker(['exec', container, 'psql', '-U', POSTGRES_USER, '-d', 'postgres',
        '-v', 'ON_ERROR_STOP=1', '-c', `CREATE DATABASE ${name} OWNER ${POSTGRES_USER};`]);
      database(name, 'CREATE TABLE item (id bigserial PRIMARY KEY, name text NOT NULL); '
        + "INSERT INTO item(name) VALUES ('kept schema');");
    }
    resetPostgres({ lease: { resources: { database: target,
      container: { name: container, id } } } });
    assert.equal(database(target,
      "SELECT to_regclass('public.item') IS NOT NULL, count(*) FROM item;").trim(), 't|0');
    database(target, "INSERT INTO item(name) VALUES ('reseeded');");
    assert.equal(database(target, "SELECT id FROM item WHERE name = 'reseeded';").trim(), '1');
    assert.equal(database(neighbor, 'SELECT count(*) FROM item;').trim(), '1');
  } finally {
    for (const name of [target, neighbor]) {
      try {
        docker(['exec', container, 'psql', '-U', POSTGRES_USER, '-d', 'postgres',
          '-v', 'ON_ERROR_STOP=1', '-c', `DROP DATABASE IF EXISTS ${name} WITH (FORCE);`]);
      } catch { /* keep the original test failure */ }
    }
  }
});

test('Spacetime reset publishes inside the exact leased build container', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reset-node-'));
  const app = join(root, 'app');
  const leasePath = join(root, 'lease.json');
  writeModule(join(app, 'backend', 'spacetimedb'));
  const lease = createBackendLease({ runId: 'reset-test', backend: 'spacetime', track: 'ecommerce',
    runIndex: 0, serverUri: 'http://127.0.0.1:3310', module: 'app-ecom-run0',
    dataDir: join(root, 'data') });
  lease.state = 'active';
  lease.resources.buildContainer = { name: 'leased-build', id: 'a'.repeat(64), running: true,
    owned: true, image: `sha256:${'b'.repeat(64)}` };
  writeBackendLease(leasePath, lease);
  const previousLease = process.env.STACK_BENCH_LEASE;
  const previousToken = process.env.STACK_BENCH_LEASE_TOKEN;
  process.env.STACK_BENCH_LEASE = leasePath;
  process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
  const calls = [];
  const exec = (command, args, options) => {
    calls.push({ argv: [command, ...args], options });
    if (args[0] === 'inspect') return `${lease.resources.buildContainer.id}\n`;
    return '';
  };
  try {
    resetBackend({ backend: 'spacetime', app, exec });
    const publish = calls.find(call => call.argv.includes('publish'));
    assert.deepEqual(publish.argv.slice(0, 6),
      ['docker', 'exec', '-w', '/app/backend/spacetimedb', 'leased-build', '/deps/spacetimedb-cli']);
    assert.ok(publish.argv.includes('app-ecom-run0'));
    assert.ok(publish.argv.includes('http://host.docker.internal:3310'));
    assert.equal(publish.argv.includes(join(app, 'backend', 'spacetimedb')), false);
    assert.equal(calls.every(call => call.options.timeout === 120_000), true);
  } finally {
    if (previousLease === undefined) delete process.env.STACK_BENCH_LEASE;
    else process.env.STACK_BENCH_LEASE = previousLease;
    if (previousToken === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN;
    else process.env.STACK_BENCH_LEASE_TOKEN = previousToken;
    rmSync(root, { recursive: true, force: true });
  }
});

test('Spacetime reset honors the module path declared by a generated project', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reset-declared-'));
  const app = join(root, 'app');
  const leasePath = join(root, 'lease.json');
  writeModule(join(app, 'server', 'spacetimedb'));
  writeFileSync(join(app, 'server', 'spacetime.json'), JSON.stringify({
    server: 'bench', 'module-path': './spacetimedb',
  }));
  const lease = createBackendLease({ runId: 'reset-declared', backend: 'spacetime', track: 'ecommerce',
    runIndex: 0, serverUri: 'http://127.0.0.1:3310', module: 'app-ecom-run0',
    dataDir: join(root, 'data') });
  lease.state = 'active';
  lease.resources.buildContainer = { name: 'leased-build', id: 'a'.repeat(64), running: true,
    owned: true, image: `sha256:${'b'.repeat(64)}` };
  writeBackendLease(leasePath, lease);
  const previousLease = process.env.STACK_BENCH_LEASE;
  const previousToken = process.env.STACK_BENCH_LEASE_TOKEN;
  process.env.STACK_BENCH_LEASE = leasePath;
  process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
  const calls = [];
  const exec = (command, args, options) => {
    calls.push({ argv: [command, ...args], options });
    if (args[0] === 'inspect') return `${lease.resources.buildContainer.id}\n`;
    return '';
  };
  try {
    assert.deepEqual(resolveSpacetimeModuleLayout(app), {
      moduleDirectory: 'server/spacetimedb',
      hostPath: join(app, 'server', 'spacetimedb'),
      containerPath: '/app/server/spacetimedb',
      configPath: 'server/spacetime.json',
      source: 'spacetime.json',
    });
    resetBackend({ backend: 'spacetime', app, exec });
    const publish = calls.find(call => call.argv.includes('publish'));
    assert.deepEqual(publish.argv.slice(0, 6),
      ['docker', 'exec', '-w', '/app/server/spacetimedb', 'leased-build', '/deps/spacetimedb-cli']);
    assert.equal(publish.argv.filter(value => value === '/app/server/spacetimedb').length, 2);
  } finally {
    if (previousLease === undefined) delete process.env.STACK_BENCH_LEASE;
    else process.env.STACK_BENCH_LEASE = previousLease;
    if (previousToken === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN;
    else process.env.STACK_BENCH_LEASE_TOKEN = previousToken;
    rmSync(root, { recursive: true, force: true });
  }
});

test('Spacetime layout resolution rejects missing, escaping, and ambiguous modules', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-layout-invalid-'));
  try {
    const missing = join(root, 'missing');
    mkdirSync(missing);
    assert.throws(() => resolveSpacetimeModuleLayout(missing), GeneratedAppLayoutError);

    const escaping = join(root, 'escaping');
    mkdirSync(escaping);
    writeFileSync(join(escaping, 'spacetime.json'), JSON.stringify({ 'module-path': '..' }));
    assert.throws(() => resolveSpacetimeModuleLayout(escaping), /escapes the application/);

    const ambiguous = join(root, 'ambiguous');
    writeModule(join(ambiguous, 'one'));
    writeModule(join(ambiguous, 'two'));
    assert.throws(() => resolveSpacetimeModuleLayout(ambiguous), /multiple SpacetimeDB module directories/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('Spacetime layout resolution maps container-absolute module paths into the mounted app', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-layout-container-path-'));
  try {
    writeModule(join(root, 'backend', 'spacetimedb'));
    writeFileSync(join(root, 'spacetime.json'), JSON.stringify({
      'module-path': '/app/backend/spacetimedb',
    }));
    const layout = resolveSpacetimeModuleLayout(root);
    assert.equal(layout.moduleDirectory, 'backend/spacetimedb');
    assert.equal(layout.containerPath, '/app/backend/spacetimedb');
    assert.equal(layout.source, 'spacetime.json');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
