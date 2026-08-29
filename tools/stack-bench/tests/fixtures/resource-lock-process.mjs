import { createBackendLease, acquireResourceLock,
  resourceLockScope } from '../../dist/src/runtime/backend-lease.js';

const [root, runId, mode = 'acquire'] = process.argv.slice(2);
if (!root || !runId) process.exit(2);

try {
  const lease = createBackendLease({
    runId, backend: 'stub', track: 'process-test', runIndex: 0,
  });
  const scope = resourceLockScope({
    STACK_BENCH_APPLIANCE: '1',
    STACK_BENCH_RESOURCE_LOCK_DIR: root,
  });
  lease.resources.locks.push(acquireResourceLock({
    ...scope, key: 'slot:process-test:stub:run0', lease,
  }));
  process.stdout.write('acquired\n');
  if (mode === 'hold') setInterval(() => {}, 1_000);
} catch (error) {
  process.stderr.write(`${error.message}\n`);
  process.exitCode = 3;
}
