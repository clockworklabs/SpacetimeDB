import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

function checkScenarioArgs(args: readonly string[]): void {
  const result = spawnSync(process.execPath, ['commands/check-scenarios.js', ...args], {
    cwd: root,
    encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /0 errors; 0 warnings/);
}

test('the progression scenarios use only their selected testing interfaces', () => {
  checkScenarioArgs(['--track', 'ecommerce', '--recipe', 'progression-catalog-1.0.0.json']);
});

for (const recipe of [
  'l1-modular-2.5.0.json',
  'l2-standard-1.6.0.json',
  'l3-standard-1.0.0.json',
]) {
  test(`${recipe} scenarios use the testing interface sent by the whole recipe`, () => {
    checkScenarioArgs(['--track', 'ecommerce', '--recipe', recipe]);
  });
}
