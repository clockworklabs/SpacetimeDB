import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

test('the progression scenarios use only their selected testing interfaces', () => {
  const result = spawnSync(process.execPath, [
    'commands/check-scenarios.mjs',
    '--track', 'ecommerce',
    '--recipe', 'progression-catalog-1.0.0.json',
  ], { cwd: root, encoding: 'utf8' });

  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /0 errors; 0 warnings/);
});

for (const recipe of [
  'l1-modular-2.5.0.json',
  'l2-standard-1.6.0.json',
  'l3-standard-1.0.0.json',
]) {
  test(`${recipe} scenarios use the testing interface sent by the whole recipe`, () => {
    const result = spawnSync(process.execPath, [
      'commands/check-scenarios.mjs',
      '--track', 'ecommerce',
      '--recipe', recipe,
    ], { cwd: root, encoding: 'utf8' });

    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.match(result.stdout, /0 errors; 0 warnings/);
  });
}
