import assert from 'node:assert/strict';
import test from 'node:test';
import { fetchStatus } from '../readiness.mjs';

test('a readiness fetch that never settles terminates at its explicit deadline', async () => {
  const started = Date.now();
  const status = await fetchStatus('http://example.invalid', {
    timeoutMs: 25,
    fetchImpl: () => new Promise(() => {}),
  });
  assert.equal(status, null);
  assert.ok(Date.now() - started >= 15);
  assert.ok(Date.now() - started < 1000);
});

test('readiness returns an HTTP status and clears its deadline', async () => {
  assert.equal(await fetchStatus('http://example.invalid', {
    timeoutMs: 1000,
    fetchImpl: async () => ({ status: 204 }),
  }), 204);
});
