import test from 'node:test';
import assert from 'node:assert/strict';
import { harnessBrowserFailure, harnessProcessFailure,
  runBrowserInfrastructureOperation } from '../src/evidence/harness-errors.js';

test('child-process timeouts are harness failures, not application findings', () => {
  const error = Object.assign(new Error('spawnSync docker ETIMEDOUT'), {
    code: 'ETIMEDOUT',
    path: 'docker',
    status: null,
  });
  assert.equal(harnessProcessFailure(error), 'docker failed in the harness (ETIMEDOUT)');
});

test('a child process non-zero exit remains eligible as an application finding', () => {
  const error = Object.assign(new Error('command failed'), { status: 1, path: 'docker' });
  assert.equal(harnessProcessFailure(error), null);
});

test('a missing harness database container is not blamed on the application', () => {
  const error = Object.assign(new Error('docker exec failed'), {
    status: 1,
    stderr: 'Error response from daemon: No such container: stack-bench-mongodb',
  });
  assert.equal(harnessProcessFailure(error),
    'database container selected by the harness is unavailable');
});

test('a crashed browser target is inconclusive harness evidence', async () => {
  assert.match(harnessBrowserFailure(new Error('browserContext.setOffline: Target crashed ')) ?? '',
    /^browser target failed in the harness/);
  assert.equal(harnessBrowserFailure(new Error('expected stock 15, saw 20')), null);
  // Browser setup operations are harness failures, but app navigation is not.
  const infrastructure = await runBrowserInfrastructureOperation('page creation', async () => {
    throw new Error('page allocation failed');
  }).catch((error: unknown) => error);
  assert.match(harnessBrowserFailure(infrastructure) ?? '', /browser page creation failed/);
  assert.equal(harnessBrowserFailure(new Error('net::ERR_CONNECTION_REFUSED')), null);
});
