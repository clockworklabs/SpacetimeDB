import test from 'node:test';
import assert from 'node:assert/strict';
import { waitFor } from '../src/stacks/lifecycle-readiness.js';

test('readiness polling preserves cancellation reasons before and during its delay', async () => {
  const reason = new Error('operator stopped the run');
  await assert.rejects(waitFor(async () => false, 1000, 'app', AbortSignal.abort(reason)),
    error => error === reason);
  const controller = new AbortController();
  const pending = waitFor(async () => false, 1000, 'app', controller.signal);
  setImmediate(() => controller.abort(reason));
  await assert.rejects(pending, error => error === reason);
  await assert.rejects(waitFor(async () => false, 1000, 'app', AbortSignal.abort(null)),
    /backend control cancelled/);
  await waitFor(async () => true, 1000, 'app', null);
  await assert.rejects(waitFor(async () => false, 0, 'app'), /timed out waiting for app/);
});
