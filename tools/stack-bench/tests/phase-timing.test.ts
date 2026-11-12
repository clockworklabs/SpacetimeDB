import assert from 'node:assert/strict';
import test from 'node:test';
import { measurePhase, type PhaseTiming } from '../src/evidence/phase-timing.js';
import { createArtifact } from '../src/evidence/artifact-schema.js';

test('phase measurement preserves results and failures and records both', async () => {
  const timings: PhaseTiming[] = [];
  const failure = new Error('reset failed');
  assert.equal(await measurePhase(timings, 'reset', null, () => false), false);
  await assert.rejects(measurePhase(timings, 'grader', 'check-1', async () => {
    throw failure;
  }), error => error === failure);
  assert.deepEqual(timings.map(({ phase, suite, threw }) => ({ phase, suite, threw })), [
    { phase: 'reset', suite: null, threw: false },
    { phase: 'grader', suite: 'check-1', threw: true },
  ]);
  assert.ok(timings.every(timing => Number.isFinite(timing.durationMs) && timing.durationMs >= 0));
  const artifact = (phaseTimings: unknown) => createArtifact({ kind: 'grade_bundle',
    id: 'timing-check', payload: { suites: {}, phaseTimings } });
  assert.doesNotThrow(() => artifact(timings));
  for (const durationMs of [-1, NaN, Infinity, '1']) {
    assert.throws(() => artifact([{ ...timings[0], durationMs }]), /phaseTimings/);
  }
});
