import assert from 'node:assert/strict';
import test from 'node:test';
import { parseRunProgress } from '../../dashboard/dashboard-model.js';
import { elapsed } from '../../dashboard/public/format.js';

test('execution elapsed time uses the execution clock, and stops at completion', () => {
  const start = '2026-09-07T12:00:00Z';
  const end = '2026-09-07T12:03:00Z';
  const now = Date.parse('2026-09-07T12:05:00Z');
  assert.equal(elapsed(start, null, now), '5m');
  assert.equal(elapsed(start, end, now), '3m');
  assert.equal(elapsed(null, null, now), '—');
  assert.equal(elapsed('invalid', null, now), '—');
  assert.equal(elapsed(start, null, Date.parse(start) - 1), '0m');
});

test('stopped attempts do not claim a previous grading or repair phase is live', () => {
  for (const log of ['=== postgres-l1-first (postgres) ===',
    '--- feature repair 2: Catalog ---']) {
    assert.equal(parseRunProgress(log, { running: false, status: 'invalid' }).phase,
      'Stopped without a valid result');
    assert.equal(parseRunProgress(log, { running: false, status: 'completed' }).phase, 'Finished');
    assert.equal(parseRunProgress(log, { running: false, status: 'pending' }).phase, 'Waiting to start');
    assert.notEqual(parseRunProgress(log, { running: true, status: 'running' }).phase, 'Finished');
  }
});
