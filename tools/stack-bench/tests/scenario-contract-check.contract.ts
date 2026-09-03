import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { STACK_BENCH_ROOT, compiledEntrypoint } from '../src/package-root.js';

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
  checkScenarioArgs(['--track', 'ecommerce', '--recipe', 'progression-catalog-2.0.2.json']);
});

for (const recipe of [
  'progression-depth3-2.0.2.json',
  'sequential-l1-2.5.0.json',
  'sequential-l2-1.6.0.json',
  'sequential-l3-1.0.0.json',
]) {
  test(`${recipe} scenarios match the product request and application interface`, () => {
    checkScenarioArgs(['--track', 'ecommerce', '--recipe', recipe]);
  });
}
