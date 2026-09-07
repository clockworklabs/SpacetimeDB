import assert from 'node:assert/strict';
import test from 'node:test';

import { ATTEMPT_CONTAINER_LIMIT_TOTALS, BUILD_CONTAINER_RESOURCE_LIMITS, DEFAULT_BUILD_IMAGE,
  PREFLIGHT_RESOURCE_FLOORS, runnerCapacity, MAX_RUNNER_CAPACITY } from '../src/composition/product-config.js';

test('product configuration keeps the published build image and container limits', () => {
  assert.equal(DEFAULT_BUILD_IMAGE, 'stack-bench-build:2.1.226');
  assert.deepEqual(BUILD_CONTAINER_RESOURCE_LIMITS, {
    cpuCount: 2,
    memoryBytes: 4 * 1024 ** 3,
    memorySwapBytes: 4 * 1024 ** 3,
    pids: 512,
  });
});

test('startup baseline stays separate from per-worker container caps', () => {
  assert.deepEqual(PREFLIGHT_RESOURCE_FLOORS, {
    cpuCount: 4,
    memoryBytes: 8 * 1024 ** 3,
    resultDiskBytes: 10 * 1024 ** 3,
    clockSkewMs: 5_000,
  });
  assert.deepEqual(ATTEMPT_CONTAINER_LIMIT_TOTALS, {
    cpuCount: 4,
    memoryBytes: 6.25 * 1024 ** 3,
  });
});


test('runner capacity is one bounded operator setting', () => {
  assert.equal(runnerCapacity({}), 1);
  assert.equal(runnerCapacity({ STACK_BENCH_RUNNER_CAPACITY: '2' }), 2);
  assert.equal(runnerCapacity({ STACK_BENCH_RUNNER_CAPACITY: String(MAX_RUNNER_CAPACITY) }), MAX_RUNNER_CAPACITY);
  for (const value of ['0', '-1', '1.5', ' 2', '2 ', 'NaN', '99999999999999999999', String(MAX_RUNNER_CAPACITY + 1)]) {
    assert.throws(() => runnerCapacity({ STACK_BENCH_RUNNER_CAPACITY: value }), /STACK_BENCH_RUNNER_CAPACITY/);
  }
});
