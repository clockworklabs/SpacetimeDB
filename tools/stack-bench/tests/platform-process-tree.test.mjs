import test from 'node:test';
import assert from 'node:assert/strict';

import { processTreePids } from '../src/runtime/platform.mjs';

test('process trees are ordered deepest-child-first for safe teardown', () => {
  const rows = `
    10 1
    11 10
    12 10
    13 11
    14 13
    20 1
  `;
  assert.deepEqual(processTreePids(10, rows), [14, 13, 11, 12, 10]);
});

test('malformed, unrelated, and cyclic process rows cannot broaden teardown', () => {
  const rows = `
    not-a-row
    31 30
    30 31
    40 1
  `;
  assert.deepEqual(processTreePids(30, rows), [31, 30]);
  assert.deepEqual(processTreePids(0, rows), []);
});
