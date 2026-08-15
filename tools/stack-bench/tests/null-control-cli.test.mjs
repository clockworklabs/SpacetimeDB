import assert from 'node:assert/strict';
import test from 'node:test';

import { nullControlSuites, parseNullControlArgs } from '../null-control.mjs';
import { loadTrack } from '../tracks.mjs';

test('null qualification can select one exact track and level', () => {
  const args = parseNullControlArgs(['node', 'null-control.mjs',
    '--track', 'ecommerce', '--level', '1']);
  assert.deepEqual(args.tracks, ['ecommerce']);
  assert.equal(args.level, 1);
  assert.throws(() => parseNullControlArgs(['node', 'null-control.mjs',
    '--track', 'ecommerce,chat', '--level', '1']), /exactly one/);
  assert.throws(() => parseNullControlArgs(['node', 'null-control.mjs',
    '--track', 'ecommerce', '--level', '0']), /positive integer/);
  assert.equal(parseNullControlArgs(['node', 'null-control.mjs', '--track', 'ecommerce',
    '--level', '1', '--recipe', 'ecommerce.l1-standard@1.1.0']).recipe,
  'ecommerce.l1-standard@1.1.0');
  assert.throws(() => parseNullControlArgs(['node', 'null-control.mjs', '--track', 'ecommerce',
    '--recipe', 'ecommerce.l1-standard@1.1.0']), /requires --level/);
  const track = loadTrack('ecommerce');
  assert.deepEqual(nullControlSuites(track, 2).map(suite => suite.level), [2, 2, 2, 2, 2]);
  assert.throws(() => nullControlSuites(track, 4), /not declared/);
});
