import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { ensureDatabase, hostServiceAddress } from '../commands/agent.js';
import { codingSessionFailure } from '../src/agents/coding-session-recovery.js';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import type { BackendLease } from '../src/runtime/backend-lease.js';
import { loadTrack } from '../src/composition/tracks.js';

const track = loadTrack('ecommerce');

function withLease<Result>(backend: string, run: (lease: BackendLease) => Result): Result {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-agent-db-'));
  const path = join(root, 'lease.json');
  const common = { runId: `agent-${backend}-run0-test`, backend, track: track.name, runIndex: 0 };
  const lease = backend === 'spacetime'
    ? createBackendLease({ ...common, serverUri: 'http://127.0.0.1:3210',
      module: 'app-ecom-run0', dataDir: join(root, 'data') })
    : createBackendLease({ ...common, database: 'app_ecom_run0',
      container: { name: `stack-bench-${backend}`, id: backend.repeat(8) } });
  lease.state = 'active';
  writeBackendLease(path, lease);
  const previousPath = process.env.STACK_BENCH_LEASE;
  const previousToken = process.env.STACK_BENCH_LEASE_TOKEN;
  process.env.STACK_BENCH_LEASE = path;
  process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
  try { return run(lease); }
  finally {
    if (previousPath === undefined) delete process.env.STACK_BENCH_LEASE;
    else process.env.STACK_BENCH_LEASE = previousPath;
    if (previousToken === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN;
    else process.env.STACK_BENCH_LEASE_TOKEN = previousToken;
    rmSync(root, { recursive: true, force: true });
  }
}

test('a failed PostgreSQL create is accepted only when the exact database exists', () => {
  withLease('postgres', lease => {
    const calls: Array<{ args: readonly string[]; options: { timeout?: number } }> = [];
    const createError = new Error('docker daemon unavailable');
    const exec = (_command: string, args: readonly string[], options: { timeout?: number }) => {
      calls.push({ args, options });
      const container = lease.resources.container;
      assert(container);
      if (args[0] === 'inspect') return `${container.id}\n`;
      if (args.some(arg => String(arg).startsWith('CREATE DATABASE'))) throw createError;
      if (args.includes('-tAc')) return '';
      return '';
    };
    assert.throws(() => ensureDatabase('postgres', 0, null, track, true, { exec, lease }),
      error => error === createError);
    assert.equal(calls.every(call => call.options.timeout === 120_000), true);
  });
});

test('a PostgreSQL wipe failure aborts a supposedly clean build', () => {
  withLease('postgres', lease => {
    const exec = (_command: string, args: readonly string[]) => {
      const container = lease.resources.container;
      assert(container);
      if (args[0] === 'inspect') return `${container.id}\n`;
      if (args.some(arg => String(arg).includes('DROP SCHEMA'))) throw new Error('wipe failed');
      return '';
    };
    assert.throws(() => ensureDatabase('postgres', 0, null, track, true, { exec, lease }),
      /could not wipe app_ecom_run0/);
  });
});

test('a MongoDB wipe failure aborts a supposedly clean build', () => {
  withLease('mongodb', lease => {
    const exec = (_command: string, args: readonly string[]) => {
      const container = lease.resources.container;
      assert(container);
      if (args[0] === 'inspect') return `${container.id}\n`;
      throw new Error('wipe failed');
    };
    assert.throws(() => ensureDatabase('mongodb', 0, null, track, true, { exec, lease }),
      /could not wipe app_ecom_run0/);
  });
});

test('Spacetime cleanup ignores absence but rejects authorization and transport failures', () => {
  withLease('spacetime', lease => {
    const absent = Object.assign(new Error('404 Not Found'), { stderr: '404 Not Found' });
    assert.doesNotThrow(() => ensureDatabase('spacetime', 0, null, track, true,
      { exec: () => { throw absent; }, stdbBin: 'spacetime-test', lease }));
    const currentCliAbsent = Object.assign(new Error('command failed'),
      { stderr: Buffer.from('Error: failed to find database `app-ecom-run0`.\n') });
    assert.doesNotThrow(() => ensureDatabase('spacetime', 0, null, track, true,
      { exec: () => { throw currentCliAbsent; }, stdbBin: 'spacetime-test', lease }));
    const unauthorized = Object.assign(new Error('401 Unauthorized'), { stderr: '401 Unauthorized' });
    assert.throws(() => ensureDatabase('spacetime', 0, null, track, true,
      { exec: () => { throw unauthorized; }, stdbBin: 'spacetime-test', lease }),
    /could not delete prior module/);
  });
});

test('coding session failures retain bounded stderr for nonzero exits', () => {
  const detail = codingSessionFailure({ status: 1,
    stdout: Buffer.from('provider stdout detail'),
    stderr: Buffer.from(`provider rejected the session\n${'x'.repeat(5000)}`) });
  assert.match(detail, /coding session failed \(exit 1\)/);
  assert.match(detail, /inner stdout tail:\nprovider stdout detail/);
  assert.match(detail, /inner stderr tail/);
  assert.equal(detail.endsWith('x'.repeat(4000)), true);
  assert.equal(detail.includes('provider rejected the session'), false);
});

test('exit 137 is reported as a kill without guessing that it was OOM', () => {
  const detail = codingSessionFailure({ status: 137,
    stderr: Buffer.from('STACK_BENCH_CODING_PROCESS_DIAGNOSTIC {"status":137,"cgroupMemory":"oom_kill 0"}') });
  assert.match(detail, /forcibly killed/);
  assert.match(detail, /coding-process diagnostic/);
  assert.doesNotMatch(detail, /out of memory|OOM failure/i);
  assert.match(detail, /oom_kill 0/);
});

test('appliance uses the configured host-service address', () => {
  assert.equal(hostServiceAddress({ STACK_BENCH_APPLIANCE: '1' }), '127.0.0.1');
  assert.equal(hostServiceAddress({}), 'host.docker.internal');
  assert.equal(hostServiceAddress({ STACK_BENCH_APPLIANCE: '1',
    STACK_BENCH_HOST_ALIAS: 'explicit-host' }), 'explicit-host');
});
