import assert from 'node:assert/strict';
import test from 'node:test';
import { createCheckEvidence } from '../check-evidence.mjs';
import { aggregatePackRuntime, exceededPackBudgets, measureGradePackRuntime,
  PACK_RUNTIME_METRIC } from '../pack-runtime.mjs';

const evidence = durationMs => createCheckEvidence({ status: 'passed', code: 'completed',
  phase: 'assertion', startedAtMs: 100, completedAtMs: 100 + durationMs });
const setupEvidence = durationMs => createCheckEvidence({ status: 'passed', code: 'completed',
  phase: 'setup', startedAtMs: 10, completedAtMs: 10 + durationMs });

function report() {
  return {
    selection: { checks: [
      { stableKey: 'pack-a/check-1', packId: 'pack-a' },
      { stableKey: 'pack-b/check-1', packId: 'pack-b' },
      { stableKey: 'pack-a/check-2', packId: 'pack-a' },
    ] },
    features: [{ setupEvidence: setupEvidence(5), criteria: [
      { stableKey: 'pack-a/check-1', evidence: evidence(30) },
      { stableKey: 'pack-b/check-1', evidence: evidence(10) },
      { stableKey: 'pack-a/check-2', evidence: evidence(20) },
    ] }],
  };
}

test('grade timing is attributed by permanent check key and pack id', () => {
  assert.deepEqual(measureGradePackRuntime(report()), {
    schemaVersion: 1,
    metric: PACK_RUNTIME_METRIC,
    packs: [
      { id: 'pack-a', checkCount: 2, setupRuntimeMs: 5, criterionRuntimeMs: 50,
        measuredRuntimeMs: 55 },
      { id: 'pack-b', checkCount: 1, setupRuntimeMs: 5, criterionRuntimeMs: 10,
        measuredRuntimeMs: 15 },
    ],
  });
});

test('pack timing refuses missing, repeated, and out-of-scope evidence', () => {
  const missing = report();
  missing.features[0].criteria.pop();
  assert.throws(() => measureGradePackRuntime(missing), /missing pack-a\/check-2/);
  const repeated = report();
  repeated.features[0].criteria.push(repeated.features[0].criteria[0]);
  assert.throws(() => measureGradePackRuntime(repeated), /repeats stable key/);
  const unknown = report();
  unknown.features[0].criteria[0].stableKey = 'other/check';
  assert.throws(() => measureGradePackRuntime(unknown), /unknown stable key/);
});

test('suite timing accumulates across suites and enforces only declared bounds', () => {
  const first = { packRuntime: measureGradePackRuntime(report()) };
  const secondReport = report();
  secondReport.features[0].criteria[0].evidence = evidence(11);
  secondReport.features[0].criteria[1].evidence = evidence(9);
  secondReport.features[0].criteria[2].evidence = evidence(10);
  const runtime = aggregatePackRuntime([first, { packRuntime: measureGradePackRuntime(secondReport) }], [
    { id: 'pack-a', budget: { status: 'bounded', maxRuntimeMs: 80 } },
    { id: 'pack-b', budget: { status: 'unmeasured' } },
  ]);
  assert.deepEqual(runtime.packs, [
    { id: 'pack-a', checkCount: 4, setupRuntimeMs: 10, criterionRuntimeMs: 71,
      measuredRuntimeMs: 81, budget: { status: 'bounded', maxRuntimeMs: 80 }, exceeded: true },
    { id: 'pack-b', checkCount: 2, setupRuntimeMs: 10, criterionRuntimeMs: 19, measuredRuntimeMs: 29,
      budget: { status: 'unmeasured' }, exceeded: null },
  ]);
  assert.deepEqual(exceededPackBudgets(runtime).map(pack => pack.id), ['pack-a']);
});
