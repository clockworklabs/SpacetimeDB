import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { captureBackendDiagnostics, controlBackend, hostedStopScript } from '../dist/src/runtime/backend-control.js';
import { createBackendLease, writeBackendLease } from '../dist/src/runtime/backend-lease.js';
import { controlHosted } from '../dist/src/stacks/stack-lifecycle-operations.mjs';

test('backend control refuses without an authenticated lease', async () => {
  const priorPath = process.env.STACK_BENCH_LEASE;
  const priorToken = process.env.STACK_BENCH_LEASE_TOKEN;
  delete process.env.STACK_BENCH_LEASE;
  delete process.env.STACK_BENCH_LEASE_TOKEN;
  try {
    await assert.rejects(controlBackend({ backend: 'mongodb', app: '.', port: 6101, probe: '/api/items' }),
      /STACK_BENCH_LEASE is required/);
  } finally {
    if (priorPath === undefined) delete process.env.STACK_BENCH_LEASE;
    else process.env.STACK_BENCH_LEASE = priorPath;
    if (priorToken === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN;
    else process.env.STACK_BENCH_LEASE_TOKEN = priorToken;
  }
});

test('restart diagnostics are copied only from the exact leased build container', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-backend-log-'));
  const leasePath = join(root, 'lease.json');
  const output = join(root, 'restart.log');
  const lease = createBackendLease({ runId: 'diagnostics-test', backend: 'mongodb',
    track: 'ecommerce', runIndex: 0, database: 'app_ecom_run0',
    container: { name: 'database', id: 'd'.repeat(64) } });
  lease.state = 'active';
  lease.resources.buildContainer = { name: 'leased-build', id: 'a'.repeat(64),
    running: true, owned: true, image: `sha256:${'b'.repeat(64)}`,
    resourceLimits: { cpuCount: 2, memoryBytes: 1024, memorySwapBytes: 1024, pids: 32 } };
  writeBackendLease(leasePath, lease);
  const priorPath = process.env.STACK_BENCH_LEASE;
  const priorToken = process.env.STACK_BENCH_LEASE_TOKEN;
  process.env.STACK_BENCH_LEASE = leasePath;
  process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
  const calls = [];
  const exec = (command, args, options) => {
    calls.push({ argv: [command, ...args], options });
    if (args[0] === 'inspect') return `${lease.resources.buildContainer.id}\n`;
    return '===== /run/application/restart-mongodb-6673.log =====\nserver failed clearly\n';
  };
  try {
    assert.deepEqual(captureBackendDiagnostics(output, { exec }), { captured: true, path: output });
    assert.match(readFileSync(output, 'utf8'), /server failed clearly/);
    assert.deepEqual(calls[1].argv.slice(0, 4), ['docker', 'exec', 'leased-build', 'sh']);
    assert.match(calls[1].argv.at(-1), /reference-server\.log/);
    assert.match(calls[1].argv.at(-1), /reference-client\.log/);
    assert.match(calls[1].argv.at(-1), /restart-\*\.log/);
    assert.equal(calls[0].options.timeout, 120_000);
    assert.equal(calls[1].options.timeout, 120_000);
  } finally {
    if (priorPath === undefined) delete process.env.STACK_BENCH_LEASE;
    else process.env.STACK_BENCH_LEASE = priorPath;
    if (priorToken === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN;
    else process.env.STACK_BENCH_LEASE_TOKEN = priorToken;
    rmSync(root, { recursive: true, force: true });
  }
});

test('hosted backend stop targets safe process groups and exact group-1 listeners', () => {
  const command = hostedStopScript(6301);
  assert.ok(command.match(/lsof -ti tcp:6301 -sTCP:LISTEN/g).length >= 3,
    'listener ownership must be reacquired before TERM, before KILL, and during final verification');
  assert.match(command, /ps -o pgid=/);
  assert.match(command, /\/bin\/kill -TERM -- "-\$pgid"/);
  assert.match(command, /\/bin\/kill -TERM "\$pid"/);
  assert.match(command, /\/bin\/kill -KILL -- "-\$pgid"/);
  assert.match(command, /\/bin\/kill -KILL "\$pid"/);
  assert.match(command, /quiet.*-ge 10/);
  assert.match(command, /hosted backend port 6301 still has a listener/);
  assert.match(command, /self_pgid=/);
  assert.match(command, /init_pgid=/);
  assert.match(command, /"\$pgid" = 1/);
  assert.match(command, /direct="\$direct \$pid"/);
  assert.match(command, /unsafe listener pid/);
  assert.throws(() => hostedStopScript('6301; rm -rf /'), /invalid hosted backend port/);
});

test('hosted backend control inspects and stops listeners as the application user', async () => {
  const id = 'a'.repeat(64);
  const calls = [];
  const exec = (command, args, options) => {
    calls.push({ command, args, options });
    if (args[0] === 'inspect') return `${id}\n`;
    return '';
  };

  await controlHosted({
    adapterId: 'mongodb',
    lease: { resources: { buildContainer: { name: 'leased-build', id, owned: true } } },
    app: '.',
    port: 65534,
    probe: '/',
    mode: 'stop',
    exec,
  });

  assert.deepEqual(calls[1].args.slice(0, 7), [
    'exec', '--user', '10001:10001', '-e', 'HOME=/home/developer', '-e', 'USER=developer',
  ]);
  assert.equal(calls[1].args[7], 'leased-build');
  assert.match(calls[1].args.at(-1), /lsof -ti tcp:65534 -sTCP:LISTEN/);
});
