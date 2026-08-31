import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import test from 'node:test';

import { levelPrompt, listTracks, loadTrack, suitesFor }
  from '../src/composition/tracks.js';
import type { TrackSuiteSource } from '../src/composition/tracks.js';

test('each validated track level has a prompt and declared scenario files', () => {
  for (const name of listTracks()) {
    const track = loadTrack(name);
    assert.ok(track.validatedThrough >= 0, `${name} validatedThrough must be non-negative`);
    assert.ok(track.plannedThrough >= track.validatedThrough,
      `${name} plannedThrough must include its validated levels`);
    for (let level = 1; level <= track.validatedThrough; level += 1) {
      assert.ok(levelPrompt(track, level).trim(), `${name} L${level} prompt is empty`);
      const suites = suitesFor(track, level);
      assert.ok(suites.length > 0, `${name} L${level} has no suites`);
      for (const suite of suites) {
        assert.ok(existsSync(suite.spec), `${name} L${level} suite is missing: ${suite.spec}`);
      }
    }
  }
});

test('an undeclared level never falls back to L1 grading', () => {
  const chat = loadTrack('chat');
  assert.throws(() => suitesFor(chat, 3), /No scenario suites declared/);
});

test('declared levels remain available before validation', () => {
  const chat = loadTrack('chat');
  const ecommerce = loadTrack('ecommerce');
  assert.equal(chat.validatedThrough, 0);
  assert.equal(ecommerce.validatedThrough, 0);
  assert.equal(ecommerce.plannedThrough, 6);
  assert.ok(suitesFor(ecommerce, 3).some(suite => suite.id === 'features'));
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
