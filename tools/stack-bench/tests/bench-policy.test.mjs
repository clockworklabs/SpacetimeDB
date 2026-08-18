import assert from 'node:assert/strict';
import test from 'node:test';

import { parseArgs } from '../bench.mjs';

test('direct runs default to ten repair rounds while an explicit budget still wins', () => {
  assert.equal(parseArgs(['node', 'bench', '--backend', 'postgres']).fixRounds, 10);
  assert.equal(parseArgs(['node', 'bench', '--backend', 'postgres',
    '--fix-rounds', '4']).fixRounds, 4);
});
