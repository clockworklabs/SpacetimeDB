import assert from 'node:assert/strict';
import test from 'node:test';

import { parseGradeArgs } from '../grader/grade.js';
import { parseMutationArgs, remainingMutationBatchMs } from '../grader/mutation-test.js';

test('grader arguments require an explicit scenario and valid numeric selectors', () => {
  assert.throws(() => parseGradeArgs(['node', 'grade', '--url', 'http://localhost:1']),
    /--spec/);
  assert.throws(() => parseGradeArgs(['node', 'grade', '--url', 'file:///tmp/app',
    '--spec', 'scenario.json']), /HTTP or HTTPS/);
  assert.throws(() => parseGradeArgs(['node', 'grade', '--url', 'http://localhost:1',
    '--spec', 'scenario.json', '--level', '0']), /positive integer/);
  assert.equal(parseGradeArgs(['node', 'grade', '--url', 'http://localhost:1',
    '--spec', 'scenario.json', '--level', '2']).level, 2);
});

test('mutation arguments fail before execution when the batch bounds are invalid', () => {
  const base = ['node', 'mutation-test', '--app', 'app', '--url', 'http://localhost:1',
    '--mutations', 'mutations.json', '--level', '3', '--recipe', 'recipe@1.0.0'];
  assert.throws(() => parseMutationArgs([...base, '--mutation-shard-index', '0']),
    /must be supplied together/);
  assert.throws(() => parseMutationArgs([...base, '--max-runtime-minutes', '0']),
    /from 1 through 120/);
  assert.equal(parseMutationArgs([...base, '--max-runtime-minutes', '30']).maxRuntimeMinutes, 30);
});

test('mutation operations use only the remaining batch time', () => {
  assert.equal(remainingMutationBatchMs(10_000, 2_500), 7_500);
  assert.throws(() => remainingMutationBatchMs(10_000, 10_000),
    /deadline reached/);
});
