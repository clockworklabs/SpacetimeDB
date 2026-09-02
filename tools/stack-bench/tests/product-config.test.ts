import assert from 'node:assert/strict';
import test from 'node:test';

import { BUILD_CONTAINER_RESOURCE_LIMITS, DEFAULT_BUILD_IMAGE,
  preflightResourceFloors } from '../src/composition/product-config.js';

test('product configuration keeps the published build image and container limits', () => {
  assert.equal(DEFAULT_BUILD_IMAGE, 'stack-bench-build:2.1.226');
  assert.deepEqual(BUILD_CONTAINER_RESOURCE_LIMITS, {
    cpuCount: 2,
    memoryBytes: 4 * 1024 ** 3,
    memorySwapBytes: 4 * 1024 ** 3,
    pids: 512,
  });
});

test('preflight resource floors scale with concurrent attempts', () => {
  assert.deepEqual(preflightResourceFloors(1), {
    cpuCount: 4,
    memoryBytes: 8 * 1024 ** 3,
    resultDiskBytes: 10 * 1024 ** 3,
    clockSkewMs: 5_000,
  });
  assert.deepEqual(preflightResourceFloors(3), {
    cpuCount: 10,
    memoryBytes: 16 * 1024 ** 3,
    resultDiskBytes: 10 * 1024 ** 3,
    clockSkewMs: 5_000,
  });
  assert.equal(preflightResourceFloors(9).memoryBytes, 40 * 1024 ** 3);
  assert.throws(() => preflightResourceFloors(0), /parallelism must be a positive integer/);
  assert.throws(() => preflightResourceFloors(1.5), /parallelism must be a positive integer/);
});
