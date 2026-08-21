import assert from 'node:assert/strict';
import test from 'node:test';

import { parseRepairArgs } from '../commands/repair-cli.mjs';

test('repair CLI separates inspection from one explicitly bounded grant', () => {
  const status = parseRepairArgs(['node', 'repair-cli.mjs', 'status', './run', '--level', '2']);
  assert.equal(status.command, 'status');
  assert.equal(status.level, 2);

  const grant = parseRepairArgs(['node', 'repair-cli.mjs', 'grant', './run',
    '--level', '2', '--rounds', '4', '--max-budget-usd', '25', '--timeout-minutes', '90']);
  assert.equal(grant.command, 'grant');
  assert.equal(grant.level, 2);
  assert.equal(grant.rounds, 4);
  assert.equal(grant.maxBudgetUsd, 25);
  assert.equal(grant.timeoutMinutes, 90);
});

test('repair CLI rejects unbounded, duplicate, and ambiguous requests', () => {
  const invalid = [
    ['grant', './run', '--level', '1'],
    ['grant', './run', '--level', '1', '--rounds', '0'],
    ['grant', './run', '--level', '1', '--rounds', '21'],
    ['grant', './run', '--level', '1', '--level', '2', '--rounds', '4'],
    ['grant', './run', '--level', '1', '--rounds', '4', '--timeout-minutes', '0'],
    ['status', './run'],
  ];
  for (const args of invalid) {
    assert.throws(() => parseRepairArgs(['node', 'repair-cli.mjs', ...args]));
  }
});
