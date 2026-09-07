import assert from 'node:assert/strict';
import test from 'node:test';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import { checkpointChecks, checkpointSessions, completionCurve, recordRunCheckpoint } from '../src/evidence/run-checkpoints.js';
import type { CheckpointRun } from '../src/evidence/run-checkpoints.js';

const selected = [{ stableKey: 'feature.a', points: 9 }, { stableKey: 'feature.b', points: 1 },
  { stableKey: 'control', points: 0 }];
const run = (): CheckpointRun => ({ condition: { requested: { levels: [{ selection: { scoredChecks: selected } }] } } });
const session = (costUsd: number, exact = true) => ({ costUsd, costComplete: true,
  costReceipts: [{ receipt: { costUsd, exact, complete: true, reconciled: true, error: null } }] });
function measure(target: CheckpointRun, passed: string[], costUsd: number, accepted = true, exact = true) {
  const sequence = (target.checkpoints?.length ?? 0) + 1;
  return recordRunCheckpoint(target, { phase: sequence === 1 ? 'first-build' : 'repair', level: 1,
    accepted, extraSessions: [session(costUsd, exact)], sourceSha256: String(sequence).repeat(64),
    evidence: { path: `grades/${sequence}/bundle.json`, sha256: 'a'.repeat(64) },
    bundle: { selection: { sha256: 'b'.repeat(64), reportedChecks: selected.map(check => check.stableKey) },
      suites: { app: { features: [{ criteria: selected.map(check => ({ ...check,
        evidence: createCheckEvidence({ status: passed.includes(check.stableKey) ? 'passed' : 'failed',
          code: passed.includes(check.stableKey) ? 'completed' : 'application_assertion',
          phase: 'assertion', startedAtMs: 1, completedAtMs: 2 }) })) }] } } } });
}

test('check completion has a fixed denominator, excludes zero-point controls, and counts no weighted points', () => {
  const point = measure(run(), ['feature.a'], 2);
  assert.deepEqual(point.completion, { selected: 2, passed: 1, failed: 1, blocked: 0, unmeasured: 0, rate: 0.5 });
  assert.deepEqual(point.cost, { status: 'exact', costUsd: 2 });
});

test('cost curves use measured checkpoints, preserve regressions, bounds, and targets not reached', () => {
  const target = run();
  measure(target, ['feature.a'], 2);
  measure(target, [], 3, false);
  measure(target, ['feature.a', 'feature.b'], 4, true, false);
  const curve = completionCurve(target.checkpoints!, [1, 2, 3, 4], [0.5, 1]);
  assert.deepEqual(curve.completionAtSpend.map(point => point.completion?.rate ?? null), [null, 0.5, 0.5, 1]);
  assert.deepEqual(curve.costToCompletion.map(point => point.cost), [
    { status: 'exact', costUsd: 2 }, { status: 'upper-bound', costUsd: 4 }]);
  assert.equal(curve.checkpoints[1]?.completion.rate, 0, 'rejected regression remains visible');
  const partial = completionCurve(target.checkpoints!.slice(0, 1), [], [1]);
  assert.equal(partial.costToCompletion[0]?.status, 'not-reached');
  assert.equal(partial.costToCompletion[0]?.cost.costUsd, null);
  assert.equal(completionCurve([], [2], [1]).costToCompletion[0]?.status, 'unmeasured');
});

test('session extraction excludes inherited levels and preserves distinct equal-cost paid sessions', () => {
  const first = session(2);
  const second = session(2);
  const target = { levels: [{ level: 1, buildSessions: [session(9)] },
    { level: 2, buildSessions: [first] }], progressionResume: { inheritedLevels: [1] } };
  assert.deepEqual(checkpointSessions(target, [first, second]), [first, second]);
  const resumed = { ...run(), ...target };
  assert.equal(measure(resumed, ['feature.a'], 1).cost.status, 'unknown', 'missing prior proof stays unknown');
});


test('checkpoint replay excludes diagnostic checks and preserves accepted state across rejected grades', () => {
  const target = run();
  measure(target, ['feature.a'], 1);
  measure(target, [], 2, false);
  const previous = target.checkpoints![0]!.checks;
  const checks = checkpointChecks(selected.map(check => ({ id: check.stableKey, points: check.points })),
    [...previous, { id: 'outside.scope', status: 'passed' }], { suites: {}, selection: { reportedChecks: [] } });
  assert.deepEqual(checks, [{ id: 'feature.a', status: 'passed' }, { id: 'feature.b', status: 'failed' }]);
  const checkpoint = recordRunCheckpoint(target, { level: 1, phase: 'repair',
    sourceSha256: 'a'.repeat(64), evidence: { path: 'grade.json', sha256: 'b'.repeat(64) },
    bundle: { suites: {}, selection: { sha256: 'c'.repeat(64), reportedChecks: [] } } });
  assert.equal(checkpoint.completion.rate, 0.5);
});
