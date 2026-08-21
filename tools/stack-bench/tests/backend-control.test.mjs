import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { captureBackendDiagnostics, controlBackend, hostedStopScript } from '../src/runtime/backend-control.mjs';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.mjs';

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
    track: 'ecommerce', runIndex: 0, database: 'stackbench_ecom_run0',
    container: { name: 'database', id: 'd'.repeat(64) } });
  lease.state = 'active';
  lease.resources.buildContainer = { name: 'leased-build', id: 'a'.repeat(64),
    running: true, owned: true, image: `sha256:${'b'.repeat(64)}` };
  writeBackendLease(leasePath, lease);
  const priorPath = process.env.STACK_BENCH_LEASE;
  const priorToken = process.env.STACK_BENCH_LEASE_TOKEN;
  process.env.STACK_BENCH_LEASE = leasePath;
  process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
  const calls = [];
  const exec = (command, args, options) => {
    calls.push({ argv: [command, ...args], options });
    if (args[0] === 'inspect') return `${lease.resources.buildContainer.id}\n`;
    return '===== /tmp/restart-mongodb-6673.log =====\nserver failed clearly\n';
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

test('hosted backend stop targets the listener process group, not only its child PID', () => {
  const command = hostedStopScript(6301);
  assert.match(command, /lsof -ti tcp:6301 -sTCP:LISTEN/);
  assert.match(command, /ps -o pgid=/);
  assert.match(command, /\/bin\/kill -TERM -- "-\$pgid"/);
  assert.match(command, /\/bin\/kill -KILL -- "-\$pgid"/);
  assert.match(command, /\|1\).*exit 4/);
  assert.throws(() => hostedStopScript('6301; rm -rf /'), /invalid hosted backend port/);
});
