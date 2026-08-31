#!/usr/bin/env node
// Deterministic stand-in used by the fault-injection command. It asks the real Docker
// launcher to create and lease the build container, enters the partial restart
// state, then fails before producing an agent result. The benchmark runner must clean up
// without relying on a successful model session or grade.

import { execFileSync } from 'node:child_process';
import { mkdirSync, renameSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { publicBackendLease, readBackendLease } from '../src/runtime/backend-lease.js';
import { compiledEntrypoint, STACK_BENCH_ROOT } from '../src/package-root.js';
const args: Record<string, string | undefined> = {};
for (let i = 2; i < process.argv.length; i += 2) {
  const option = process.argv[i];
  if (option) args[option.replace(/^--/, '')] = process.argv[i + 1];
}

const app = args.app;
const backend = args.backend;
if (!app || !backend) throw new Error('fault-agent requires --app and --backend');
const leasePath = process.env.STACK_BENCH_LEASE ?? '';
const leaseToken = process.env.STACK_BENCH_LEASE_TOKEN ?? '';
mkdirSync(app, { recursive: true });
// Match the real agent's build-mode pin. Teardown must know that any app
// watchers live inside Docker before it considers host process cleanup.
writeFileSync(join(app, '..', '.stack-bench-isolation'), 'container');

const output = execFileSync(process.execPath,
  [compiledEntrypoint('container', 'run-build.js'), '--app', app,
    '--backend', backend, '--prepare-only'],
  { encoding: 'utf8', stdio: 'pipe', maxBuffer: 16 * 1024 * 1024 });
const prepared = JSON.parse(output.trim().split(/\r?\n/).pop() ?? '');
const beforeRestart = readBackendLease(leasePath, {
  token: leaseToken, backend, active: true,
});

// Enter the real restart script's most dangerous partial state: the old owned
// listener is gone, the lease is `restarting`, and the replacement has not yet
// started. The script's test hook exits 86 at that exact boundary.
const restartEnv: NodeJS.ProcessEnv = { ...process.env, STACK_BENCH_TEST_FAIL_AFTER_RESTART_STOP: '1' };
if (process.platform === 'win32') {
  const bridge = (restartEnv.WSLENV ?? '').split(':').filter(Boolean);
  bridge.push('STACK_BENCH_TEST_FAIL_AFTER_RESTART_STOP');
  restartEnv.WSLENV = [...new Set(bridge)].join(':');
}
let restartStopped = false;
let restartFailure = '';
try {
  execFileSync('bash', ['restart-backend.sh', backend, '.', '', '', 'restart'],
    { cwd: STACK_BENCH_ROOT, env: restartEnv, stdio: 'pipe' });
} catch (error: unknown) {
  const childError = error instanceof Error && 'status' in error ? error as Error & {
    status?: number; stderr?: unknown;
  } : null;
  restartStopped = childError?.status === 86;
  restartFailure = `status=${childError?.status} stderr=${String(childError?.stderr).trim()}`;
}
if (!restartStopped) throw new Error(
  `restart fault hook did not stop at the expected boundary (${restartFailure || 'restart exited zero'})`);
const lease = readBackendLease(leasePath, {
  token: leaseToken, backend,
});
if (lease.state !== 'restarting') throw new Error(`expected restarting lease, got ${lease.state}`);
const marker = join(app, '.fault-ready.json');
const temporary = `${marker}.${process.pid}.tmp`;
writeFileSync(temporary, `${JSON.stringify({ prepared, phase: 'restart-stopped',
  listenerBeforeRestart: beforeRestart.resources.listenerProcesses,
  leasePath,
  lease: publicBackendLease(lease) }, null, 2)}\n`);
renameSync(temporary, marker);

// An ordinary non-zero child exit exercises the same finally/exit cleanup path
// as an unexpected coding-agent failure and works on Windows, where Node maps
// SIGTERM to TerminateProcess and cannot run JavaScript signal handlers.
process.exit(42);
