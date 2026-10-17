import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { STACK_BENCH_ROOT, compiledEntrypoint } from '../src/package-root.js';
import { referencedInterfaceValues } from '../commands/check-scenarios.js';

test('interface checks include nested action inputs and replay targets', () => {
  const step = { do: 'parallel', branches: [[
    { do: 'callAction', input: { testid: 'item-card', attribute: 'data-buy-input' } },
    { do: 'replayAs', namedTarget: { testid: 'pending-restock-item', attribute: 'data-entity-id' } },
  ]] };
  assert.deepEqual(referencedInterfaceValues(step, 'testid'), ['item-card', 'pending-restock-item']);
  assert.deepEqual(referencedInterfaceValues(step, 'attribute'), ['data-buy-input', 'data-entity-id']);
});

function checkScenarioArgs(args: readonly string[]): void {
  const result = spawnSync(process.execPath, [compiledEntrypoint('commands',
    'check-scenarios.js'), ...args], {
    cwd: STACK_BENCH_ROOT,
    encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /0 errors; 0 warnings/);
}

test('the progression scenarios use only their selected application interfaces', () => {
  checkScenarioArgs(['--track', 'ecommerce', '--recipe', 'progression-catalog.json']);
});

for (const recipe of [
  'sequential-l1.json',
  'sequential-l2.json',
  'sequential-l3.json',
]) {
  test(`${recipe} scenarios match the product request and application interface`, () => {
    checkScenarioArgs(['--track', 'ecommerce', '--recipe', recipe]);
  });
}
