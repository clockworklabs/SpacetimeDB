import assert from 'node:assert/strict';
import test from 'node:test';

import { resolveCalibrationForRelease } from '../src/composition/calibration-compiler.js';
import type { CalibrationPlan } from '../src/composition/calibration-compiler.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';

test('a shared calibration compile returns the fresh result and is not recompiled', () => {
  const track = loadTrack('ecommerce');
  const { release } = resolveRecipeRelease(track, 3, 'ecommerce.progression-catalog');
  const options = { trackRoot: track.dir, alias: 'L3' };
  const compiled = new Map<string, CalibrationPlan | Error>();
  const fresh = resolveCalibrationForRelease(release, options);
  const shared = resolveCalibrationForRelease(release, { ...options, compiled });
  assert(fresh && shared);
  assert.deepEqual(shared, fresh);
  assert(compiled.size > 0);

  // Callers own their copy; a change must not reach the next reader.
  shared.qualification.checks = [];
  assert.deepEqual(resolveCalibrationForRelease(release, { ...options, compiled }), fresh);

  // The saved outcome is reused, including a failure.
  for (const key of compiled.keys()) compiled.set(key, new Error('saved failure'));
  assert.throws(() => resolveCalibrationForRelease(release, { ...options, compiled }), /saved failure/);
});
