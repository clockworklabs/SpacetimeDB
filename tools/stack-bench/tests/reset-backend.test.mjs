import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.mjs';
import { resetBackend } from '../commands/reset-backend.mjs';
import { containerReachableSpacetimeUri } from '../src/runtime/spacetime-target.mjs';
import { GeneratedAppLayoutError, resolveSpacetimeModuleLayout } from '../src/runtime/spacetime-layout.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

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
    runIndex: 0, serverUri: 'http://127.0.0.1:3310', module: 'stackbench-ecom-run0',
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

test('Spacetime reset publishes inside the exact leased build container', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reset-node-'));
  const app = join(root, 'app');
  const leasePath = join(root, 'lease.json');
  writeModule(join(app, 'backend', 'spacetimedb'));
  const lease = createBackendLease({ runId: 'reset-test', backend: 'spacetime', track: 'ecommerce',
    runIndex: 0, serverUri: 'http://127.0.0.1:3310', module: 'stackbench-ecom-run0',
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
    assert.ok(publish.argv.includes('stackbench-ecom-run0'));
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
    runIndex: 0, serverUri: 'http://127.0.0.1:3310', module: 'stackbench-ecom-run0',
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
