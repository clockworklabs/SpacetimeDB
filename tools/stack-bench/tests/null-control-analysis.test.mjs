import assert from 'node:assert/strict';
import test from 'node:test';
import { analyseNullReports } from '../null-control-analysis.mjs';
import { createCheckEvidence } from '../check-evidence.mjs';

const criterion = (id, points, status, phase = 'assertion', summary = null) => ({
  id, points, evidence: createCheckEvidence({ status,
    code: status === 'passed' ? 'completed' : 'test_result', phase, summary,
    startedAtMs: 1, completedAtMs: 2 }),
});

test('null analysis separates expected failures, vacuous passes and oracle gaps', () => {
  const result = analyseNullReports([{ track: 'shop', level: 1, id: 'features', scenario: '01.json',
    report: { features: [{ id: 1, name: 'Items', criteria: [
      criterion('1a', 2, 'failed', 'assertion', 'missing control'),
      criterion('1b', 3, 'passed'),
      criterion('1c', 4, 'inconclusive', 'assertion', 'harness could not issue action'),
      criterion('diagnostic', 0, 'passed'),
    ] }] } }]);

  assert.equal(result.ok, false);
  assert.deepEqual(result.summary, {
    criteria: 3,
    points: 9,
    expectedFailures: { criteria: 1, points: 2 },
    expectedFailureStages: {
      setup: { criteria: 0, points: 0 },
      assertion: { criteria: 1, points: 2 },
    },
    vacuousPasses: { criteria: 1, points: 3 },
    oracleGaps: { criteria: 1, points: 4 },
    unscored: { criteria: 1, passed: 1, failed: 0, inconclusive: 0 },
  });
  assert.deepEqual(result.criteria.map(item => item.status),
    ['expected_fail', 'vacuous_pass', 'oracle_gap']);
});

test('all conclusive failures make the null control pass', () => {
  const result = analyseNullReports([{ track: 'chat', level: 2, id: 'invariants', scenario: '02.json',
    report: { features: [{ id: 10, criteria: [
      criterion('10a', 1, 'failed'),
      criterion('10b', 2, 'failed', 'setup', 'sign in did not complete'),
      criterion('diagnostic', 0, 'inconclusive'),
    ] }] } }]);
  assert.equal(result.ok, true);
  assert.deepEqual(result.summary.expectedFailures, { criteria: 2, points: 3 });
  assert.deepEqual(result.summary.expectedFailureStages, {
    setup: { criteria: 1, points: 2 },
    assertion: { criteria: 1, points: 1 },
  });
  assert.deepEqual(result.summary.unscored,
    { criteria: 1, passed: 0, failed: 0, inconclusive: 1 });
});

test('typed null analysis gets failure phase from evidence, not detail wording', () => {
  const evidence = createCheckEvidence({ status: 'failed', code: 'application_failure', phase: 'setup',
    summary: 'wording without the historical prefix', startedAtMs: 1, completedAtMs: 2 });
  const result = analyseNullReports([{ track: 'shop', level: 1, id: 'features',
    report: { features: [{ id: 1, criteria: [
      { id: 'a', points: 1, evidence },
    ] }] } }]);
  assert.equal(result.criteria[0].failureStage, 'setup');
  assert.equal(result.ok, true);
});
