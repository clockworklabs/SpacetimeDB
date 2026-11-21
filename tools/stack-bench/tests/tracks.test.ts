import assert from 'node:assert/strict';
import test from 'node:test';

import { loadTrack, portsFor, RUN_INDEX_CAP, suitesFor }
  from '../src/composition/tracks.js';
import type { TrackSuiteSource } from '../src/composition/tracks.js';

test('an undeclared level never falls back to L1 grading', () => {
  const chat = loadTrack('chat');
  assert.throws(() => suitesFor(chat, 3), /No scenario suites declared/);
});

test('invalid track and run-index inputs throw instead of terminating the process', () => {
  assert.throws(() => loadTrack('missing-track'), /Unknown track/);
  const track = loadTrack('ecommerce');
  for (const runIndex of [-1, 0.5, RUN_INDEX_CAP + 1]) {
    assert.throws(() => portsFor(track, 'mongodb', runIndex), /integer from 0 through/);
  }
});

test('suite inheritance follows the manifest policy instead of suite-name magic', () => {
  const ecommerce = loadTrack('ecommerce');
  const inherited = suitesFor(ecommerce, 3).filter(suite => suite.inherited);
  assert.deepEqual(inherited.map(suite => suite.id), [
    'invariants@L1', 'contention@L1', 'invariants@L2',
  ]);

  const synthetic: TrackSuiteSource = {
    name: 'synthetic',
    dir: ecommerce.dir,
    suites: {
      1: [
        { id: 'features', spec: 'feature.json', inherit: 'all-higher-levels' },
        { id: 'invariants', spec: 'invariant.json', inherit: 'none' },
      ],
      2: [{ id: 'next', spec: 'next.json', inherit: 'none' }],
    },
  };
  assert.deepEqual(suitesFor(synthetic, 2).map(suite => suite.id), ['next', 'features@L1']);
});
