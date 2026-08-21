#!/usr/bin/env node
// Deterministic stand-in used by fault-injection.mjs. It asks the real Docker
// launcher to create and lease the build container, enters the partial restart
// state, then fails before producing an agent result. bench.mjs must clean up
// without relying on a successful model session or grade.

import { execFileSync } from 'node:child_process';
import { mkdirSync, renameSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { publicBackendLease, readBackendLease } from '../src/runtime/backend-lease.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = dirname(HERE);
const args = {};
for (let i = 2; i < process.argv.length; i += 2) {
  args[process.argv[i].replace(/^--/, '')] = process.argv[i + 1];
}

if (!args.app || !args.backend) throw new Error('fault-agent requires --app and --backend');
mkdirSync(args.app, { recursive: true });
// Match the real agent's build-mode pin. Teardown must know that any app
// watchers live inside Docker before it considers host process cleanup.
writeFileSync(join(args.app, '..', '.stack-bench-isolation'), 'container');

const output = execFileSync(process.execPath,
  [join(ROOT, 'container', 'run-build.mjs'), '--app', args.app,
    '--backend', args.backend, '--prepare-only'],
  { encoding: 'utf8', stdio: 'pipe', maxBuffer: 16 * 1024 * 1024 });
const prepared = JSON.parse(output.trim().split(/\r?\n/).pop());
const beforeRestart = readBackendLease(process.env.STACK_BENCH_LEASE, {
  token: process.env.STACK_BENCH_LEASE_TOKEN, backend: args.backend, active: true,
});

// Enter the real restart script's most dangerous partial state: the old owned
// listener is gone, the lease is `restarting`, and the replacement has not yet
// started. The script's test hook exits 86 at that exact boundary.
const restartEnv = { ...process.env, STACK_BENCH_TEST_FAIL_AFTER_RESTART_STOP: '1' };
if (process.platform === 'win32') {
  const bridge = (restartEnv.WSLENV ?? '').split(':').filter(Boolean);
  bridge.push('STACK_BENCH_TEST_FAIL_AFTER_RESTART_STOP');
  restartEnv.WSLENV = [...new Set(bridge)].join(':');
}
let restartStopped = false;
let restartFailure = '';
try {
  execFileSync('bash', ['restart-backend.sh', args.backend, '.', '', '', 'restart'],
    { cwd: ROOT, env: restartEnv, stdio: 'pipe' });
} catch (error) {
  restartStopped = error.status === 86;
  restartFailure = `status=${error.status} stderr=${String(error.stderr).trim()}`;
}
if (!restartStopped) throw new Error(
  `restart fault hook did not stop at the expected boundary (${restartFailure || 'restart exited zero'})`);
const lease = readBackendLease(process.env.STACK_BENCH_LEASE, {
  token: process.env.STACK_BENCH_LEASE_TOKEN, backend: args.backend,
});
if (lease.state !== 'restarting') throw new Error(`expected restarting lease, got ${lease.state}`);
const marker = join(args.app, '.fault-ready.json');
const temporary = `${marker}.${process.pid}.tmp`;
writeFileSync(temporary, `${JSON.stringify({ prepared, phase: 'restart-stopped',
  listenerBeforeRestart: beforeRestart.resources.listenerPids,
  leasePath: process.env.STACK_BENCH_LEASE,
  lease: publicBackendLease(lease) }, null, 2)}\n`);
renameSync(temporary, marker);

// An ordinary non-zero child exit exercises the same finally/exit cleanup path
// as an unexpected coding-agent failure and works on Windows, where Node maps
// SIGTERM to TerminateProcess and cannot run JavaScript signal handlers.
process.exit(42);
