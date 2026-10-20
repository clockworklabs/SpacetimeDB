import assert from 'node:assert/strict';
import test from 'node:test';

import { parseRepairArgs } from '../commands/repair-cli.js';

test('repair CLI rejects unbounded, duplicate, and ambiguous requests', () => {
  const accepted = [
    ['status', './run', '--level', '2'],
    ['grant', './run', '--level', '2', '--repairs', '4', '--max-budget-usd', '25', '--timeout-minutes', '90'],
  ];
  for (const args of accepted) {
    assert.doesNotThrow(() => parseRepairArgs(['node', 'repair-cli.js', ...args]));
  }
  const invalid = [
    ['grant', './run', '--level', '1'],
    ['grant', './run', '--level', '1', '--repairs', '0'],
    ['grant', './run', '--level', '1', '--repairs', '1.5'],
    ['grant', './run', '--level', '1', '--level', '2', '--repairs', '4'],
    ['grant', './run', '--level', '1', '--repairs', '4', '--timeout-minutes', '0'],
    ['status', './run'],
  ];
  for (const args of invalid) {
    assert.throws(() => parseRepairArgs(['node', 'repair-cli.js', ...args]));
  }
});
