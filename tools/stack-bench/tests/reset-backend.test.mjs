import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { createBackendLease, writeBackendLease } from '../backend-lease.mjs';
import { resetBackend } from '../reset-backend.mjs';
import { containerReachableSpacetimeUri } from '../spacetime-target.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

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
    [join(ROOT, 'reset-backend.mjs'), 'mongodb', '.'], { env, stdio: 'pipe' }),
  /STACK_BENCH_LEASE is required/);
});

test('Spacetime reset publishes inside the exact leased build container', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reset-node-'));
  const app = join(root, 'app');
  const leasePath = join(root, 'lease.json');
  mkdirSync(join(app, 'backend', 'spacetimedb'), { recursive: true });
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
