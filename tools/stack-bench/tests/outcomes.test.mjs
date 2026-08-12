import assert from 'node:assert/strict';
import test from 'node:test';
import { aggregateRunOutcome, classifyBundle, mutationControlEligible, runExitCode } from '../outcomes.mjs';
import { createCheckEvidence } from '../check-evidence.mjs';

const bundle = criteria => ({ totals: { score: 1, max: 2 }, suites: {
  feature: { features: [{ id: 'f', criteria }] }, lint: { pass: true },
} });

const typed = (id, status, summary) => {
  const evidence = createCheckEvidence({ status, code: status === 'passed' ? 'completed' : 'test_result',
    phase: 'assertion', summary, startedAtMs: 1, completedAtMs: 2 });
  return { id, points: 1, evidence };
};

test('missing output and explicit harness failure are not app failures', () => {
  assert.equal(classifyBundle(null).kind, 'ungraded');
  assert.equal(classifyBundle({ outcome: { kind: 'harness_failure', phase: 'reset', reason: 'down' } }).kind,
    'harness_failure');
});

test('ungraded and harness-failed runs return a failing process status', () => {
  assert.equal(runExitCode({ kind: 'harness_failure' }), 1);
  assert.equal(runExitCode({ kind: 'ungraded' }), 1);
  assert.equal(runExitCode({ kind: 'app_failure' }), 0);
  assert.equal(runExitCode({ kind: 'passed' }), 0);
});

test('app failures and inconclusive evidence remain separately visible', () => {
  const outcome = classifyBundle(bundle([
    typed('a', 'failed', 'not observed'),
    typed('b', 'inconclusive', 'not measurable'),
  ]));
  assert.equal(outcome.kind, 'app_failure');
  assert.deepEqual(outcome.appFailures, ['feature/f/a']);
  assert.deepEqual(outcome.inconclusive, ['feature/f/b']);
});

test('inconclusive-only grades are not reported as passing', () => {
  assert.equal(classifyBundle(bundle([typed('a', 'inconclusive', 'not measurable')])).kind,
    'inconclusive');
});

test('typed harness failures outrank prose and application failures', () => {
  const outcome = classifyBundle(bundle([
    typed('app', 'failed', 'INCONCLUSIVE: wording must not control status'),
    typed('harness', 'harness_failure', 'this sentence sounds harmless'),
  ]));
  assert.equal(outcome.kind, 'harness_failure');
  assert.deepEqual(outcome.harnessFailures, ['feature/f/harness']);
  assert.deepEqual(outcome.appFailures, ['feature/f/app']);
});

test('a fully reported zero-point selection is graded rather than mistaken for missing output', () => {
  const scoped = {
    totals: { score: 0, max: 0 },
    selection: {
      checks: [{ stableKey: 'pack.control.zero' }],
      attemptedChecks: ['pack.control.zero'],
      reportedChecks: ['pack.control.zero'],
      notRun: [],
    },
    suites: { control: { features: [{ id: 'f', criteria: [
      { ...typed('zero', 'passed', null), stableKey: 'pack.control.zero', points: 0 },
    ] }] }, lint: { pass: true } },
  };
  assert.equal(classifyBundle(scoped).kind, 'passed');
  scoped.selection.reportedChecks = [];
  assert.equal(classifyBundle(scoped).kind, 'ungraded');
});

test('mutation control runs only after a conclusive passing pristine grade', () => {
  assert.equal(mutationControlEligible({ kind: 'passed' }), true);
  for (const kind of ['app_failure', 'inconclusive', 'harness_failure', 'ungraded']) {
    assert.equal(mutationControlEligible({ kind }), false, kind);
  }
});

test('run aggregation preserves every level and prioritizes harness failure', () => {
  const outcome = aggregateRunOutcome([
    { level: 1, outcome: { kind: 'app_failure' } },
    { level: 2, outcome: { kind: 'harness_failure', phase: 'grading' } },
  ]);
  assert.equal(outcome.kind, 'harness_failure');
  assert.equal(outcome.levels['1'].kind, 'app_failure');
});
