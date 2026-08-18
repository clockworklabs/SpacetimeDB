import test from 'node:test';
import assert from 'node:assert/strict';
import { harnessBrowserFailure, harnessProcessFailure } from '../src/evidence/harness-errors.mjs';

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

test('a crashed browser target is inconclusive harness evidence', () => {
  assert.match(harnessBrowserFailure(new Error('browserContext.setOffline: Target crashed ')),
    /^browser target failed in the harness/);
  assert.equal(harnessBrowserFailure(new Error('expected stock 15, saw 20')), null);
});
