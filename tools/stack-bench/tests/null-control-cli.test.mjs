import assert from 'node:assert/strict';
import test from 'node:test';

import { parseNullControlArgs } from '../null-control.mjs';

test('null qualification can select one exact track and level', () => {
  const args = parseNullControlArgs(['node', 'null-control.mjs',
    '--track', 'ecommerce', '--level', '1']);
  assert.deepEqual(args.tracks, ['ecommerce']);
  assert.equal(args.level, 1);
  assert.throws(() => parseNullControlArgs(['node', 'null-control.mjs',
    '--track', 'ecommerce,chat', '--level', '1']), /exactly one/);
  assert.throws(() => parseNullControlArgs(['node', 'null-control.mjs',
    '--track', 'ecommerce', '--level', '0']), /positive integer/);
});
