import assert from 'node:assert/strict';
import test from 'node:test';

import { compareCriterionEvidence, formatRepairProgress } from '../scoring.mjs';
import { createCheckEvidence } from '../check-evidence.mjs';

const criterion = (id, passed, options = {}) => ({
  ...typedCriterion(id, options.inconclusive ? 'inconclusive' : passed ? 'passed' : 'failed', null),
  points: options.points ?? 1,
});
const typedCriterion = (id, status, summary) => {
  const evidence = createCheckEvidence({ status, code: 'test_result', phase: 'assertion', summary,
    startedAtMs: 1, completedAtMs: 2 });
  return { id, points: 1, evidence };
};
const bundle = (criteria, suite = 'features', featureId = 1) => ({
  suites: { [suite]: { features: [{ id: featureId, name: 'Display name', criteria }] } },
});

test('a newly measurable failure does not make unchanged shared criteria regress', () => {
  const before = bundle([
    criterion('stable', true),
    criterion('new', false, { inconclusive: true }),
  ]);
  const after = bundle([
    criterion('stable', true),
    criterion('new', false),
  ]);

  const result = compareCriterionEvidence(before, after);
  assert.equal(result.before, 1);
  assert.equal(result.after, 1);
  assert.deepEqual(result.newlyConclusive, ['features/1/new']);
  assert.deepEqual(result.lostEvidence, []);
  assert.equal(formatRepairProgress(result,
    { before: 1, beforeMax: 1, after: 1, afterMax: 2 }),
  'no change among 1 criterion measured in both rounds (1/1 points); 1 previously unavailable criterion became measurable; overall 1/1 -> 1/2');
});

test('unchanged repair evidence is reported without hiding the overall scores', () => {
  const result = compareCriterionEvidence(
    bundle([criterion('a', true)]),
    bundle([criterion('a', true)]),
  );
  assert.equal(formatRepairProgress(result,
    { before: 1, beforeMax: 1, after: 1, afterMax: 1 }),
  'no improvement among 1 criterion measured in both rounds (1/1 points); overall 1/1 -> 1/1');
});

test('pass to inconclusive is reported as lost evidence', () => {
  const result = compareCriterionEvidence(
    bundle([criterion('a', true)]),
    bundle([criterion('a', false, { inconclusive: true })]),
  );
  assert.deepEqual(result.lostEvidence, ['features/1/a']);
});

test('fail to inconclusive is reported as lost evidence', () => {
  const result = compareCriterionEvidence(
    bundle([criterion('a', false)]),
    bundle([criterion('a', false, { inconclusive: true })]),
  );
  assert.deepEqual(result.lostEvidence, ['features/1/a']);
});

test('a real regression is compared on stable evidence', () => {
  const result = compareCriterionEvidence(
    bundle([criterion('a', true)]),
    bundle([criterion('a', false)]),
  );
  assert.equal(result.before, 1);
  assert.equal(result.after, 0);
  assert.deepEqual(result.lostEvidence, []);
});

test('suite identity prevents unrelated criteria from colliding', () => {
  const before = {
    suites: {
      features: bundle([criterion('same', true)]).suites.features,
      invariants: bundle([criterion('same', false)]).suites.features,
    },
  };
  const after = {
    suites: {
      features: bundle([criterion('same', false)]).suites.features,
      invariants: bundle([criterion('same', true)]).suites.features,
    },
  };
  const result = compareCriterionEvidence(before, after);
  assert.equal(result.count, 2);
  assert.equal(result.before, 1);
  assert.equal(result.after, 1);
});

test('rubric point changes invalidate the comparison', () => {
  const result = compareCriterionEvidence(
    bundle([criterion('a', true)]),
    bundle([criterion('a', true, { points: 2 })]),
  );
  assert.deepEqual(result.definitionChanges, [
    { key: 'features/1/a', before: 1, after: 2 },
  ]);
});

test('typed scoring never reparses a misleading summary', () => {
  const result = compareCriterionEvidence(
    bundle([typedCriterion('a', 'passed', null)]),
    bundle([typedCriterion('a', 'failed', 'INCONCLUSIVE: merely presentation text')]),
  );
  assert.equal(result.count, 1);
  assert.equal(result.before, 1);
  assert.equal(result.after, 0);
  assert.deepEqual(result.lostEvidence, []);
});
