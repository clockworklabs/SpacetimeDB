import assert from 'node:assert/strict';
import test from 'node:test';
import { aggregateRunOutcome, classifyBundle, ladderMayAdvance, ladderMayContinue,
  mutationControlEligible, runExitCode, runOutcomeKind,
  type OutcomeCriterion } from '../src/evidence/outcomes.js';
import { createCheckEvidence, type CheckEvidence,
  type CheckEvidenceStatus } from '../src/evidence/check-evidence.js';

interface TestFeature {
  cleanupEvidence?: { status: string; failures: readonly unknown[] };
  criteria: OutcomeCriterion[];
  id: string;
}

interface TestBundle {
  readonly totals: { score: number; max: number };
  readonly suites: {
    readonly feature: { readonly features: TestFeature[] };
    readonly lint: { readonly pass: boolean };
  };
}

const bundle = (criteria: OutcomeCriterion[]): TestBundle => ({
  totals: { score: 1, max: 2 },
  suites: { feature: { features: [{ id: 'f', criteria }] }, lint: { pass: true } },
});

const typed = (
  id: string,
  status: CheckEvidenceStatus,
  summary: string | null,
  points = 1,
): OutcomeCriterion & { readonly evidence: CheckEvidence; readonly id: string; readonly points: number } => {
  const evidence = createCheckEvidence({ status, code: status === 'passed' ? 'completed' : 'test_result',
    phase: 'assertion', summary, startedAtMs: 1, completedAtMs: 2 });
  return { id, points, evidence };
};

test('missing output and explicit harness failure are not app failures', () => {
  assert.equal(classifyBundle(null).kind, 'ungraded');
  assert.equal(classifyBundle({ outcome: { kind: 'harness_failure', phase: 'reset', reason: 'down' } }).kind,
    'harness_failure');
});

test('provider failures preserve their cause and stop the run', () => {
  const provider = { kind: 'provider_failure', phase: 'coding-session',
    reason: 'provider-connection-error', provider: { providerStatus: null } };
  assert.deepEqual(classifyBundle({ outcome: provider }), {
    ...provider, appFailures: [], inconclusive: [], harnessFailures: [],
  });
  const outcome = aggregateRunOutcome([{ level: 1, outcome: provider }]);
  assert.equal(outcome.kind, 'provider_failure');
  assert.equal(outcome.reason, 'provider-connection-error');
  assert.deepEqual(outcome.provider, { providerStatus: null });
  assert.equal(runExitCode(outcome), 1);
  assert.equal(ladderMayContinue(outcome), false);
});

test('ungraded and harness-failed runs return a failing process status', () => {
  assert.equal(runExitCode({ kind: 'harness_failure' }), 1);
  assert.equal(runExitCode({ kind: 'provider_failure' }), 1);
  assert.equal(runExitCode({ kind: 'ungraded' }), 1);
  assert.equal(runExitCode({ kind: 'app_failure' }), 0);
  assert.equal(runExitCode({ kind: 'passed' }), 0);
  assert.equal(runExitCode({ kind: 'inconclusive' }), 1);
  assert.equal(runExitCode({ kind: 'incomplete' }), 1);
});

test('a ladder stops after an ungraded or harness-failed level', () => {
  assert.equal(ladderMayContinue({ kind: 'app_failure' }), true);
  assert.equal(ladderMayContinue({ kind: 'inconclusive' }), false);
  assert.equal(ladderMayContinue({ kind: 'app_failure', inconclusive: ['feature/check'] }), false);
  assert.equal(ladderMayContinue({ kind: 'passed' }), true);
  assert.equal(ladderMayContinue({ kind: 'harness_failure' }), false);
  assert.equal(ladderMayContinue({ kind: 'provider_failure' }), false);
  assert.equal(ladderMayContinue({ kind: 'ungraded' }), false);
});

test('a ladder advances only from a level that actually passed', () => {
  assert.equal(ladderMayAdvance({ kind: 'passed' }), true);
  assert.equal(ladderMayAdvance({ kind: 'app_failure' }), false);
  assert.equal(ladderMayAdvance({ kind: 'inconclusive' }), false);
  assert.equal(ladderMayAdvance({ kind: 'harness_failure' }), false);
  assert.equal(ladderMayAdvance({ kind: 'ungraded' }), false);
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

test('zero-point evidence cannot fail or invalidate a scored benchmark outcome', () => {
  const outcome = classifyBundle(bundle([
    typed('scored', 'passed', null),
    typed('candidate', 'failed', 'candidate behavior failed', 0),
    typed('control', 'inconclusive', 'supporting control unavailable', 0),
  ]));
  assert.equal(outcome.kind, 'passed');
  assert.deepEqual(outcome.appFailures, []);
  assert.deepEqual(outcome.inconclusive, []);
});

test('an explicitly selected zero-only scope still has a qualification outcome', () => {
  const scoped = {
    totals: { score: 0, max: 0 },
    selection: { checks: [{ stableKey: 'candidate', points: 0 }], attemptedChecks: ['candidate'],
      reportedChecks: ['candidate'], notRun: [] },
    suites: { development: { features: [{ id: 'f', criteria: [
      { ...typed('candidate', 'failed', 'candidate behavior failed', 0), stableKey: 'candidate' },
    ] }] }, lint: { pass: true } },
  };
  assert.equal(classifyBundle(scoped).kind, 'app_failure');
  assert.deepEqual(classifyBundle(scoped).appFailures, ['development/f/candidate']);
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

test('completed check evidence outranks a declared application failure', () => {
  const criterion = { ...typed('check', 'harness_failure',
    'grader could not observe the result'), stableKey: 'feature.check' };
  const scoped = {
    ...bundle([criterion]),
    totals: { score: 0, max: 1 },
    selection: { checks: [{ stableKey: 'feature.check', points: 1 }],
      attemptedChecks: ['feature.check'], reportedChecks: ['feature.check'], notRun: [] },
    outcome: { kind: 'app_failure', phase: 'grading', appFailures: ['declared-failure'] },
  };
  const outcome = classifyBundle(scoped);
  assert.equal(outcome.kind, 'harness_failure');
  assert.deepEqual(outcome.harnessFailures, ['feature/f/check']);
});

test('grader cleanup failures invalidate the bundle', () => {
  const scoped = bundle([typed('works', 'passed', null)]);
  const feature = scoped.suites.feature.features[0];
  assert(feature);
  feature.cleanupEvidence = {
    status: 'harness_failure', failures: [{ stage: 'context-close' }],
  };
  const outcome = classifyBundle(scoped);
  assert.equal(outcome.kind, 'harness_failure');
  assert.equal(outcome.phase, 'grading-cleanup');
  assert.deepEqual(outcome.harnessFailures, ['feature/f/cleanup']);
});

test('a fully reported zero-point selection is graded rather than mistaken for missing output', () => {
  const scoped = {
    totals: { score: 0, max: 0 },
    selection: {
      checks: [{ stableKey: 'pack.control.zero', points: 0 }],
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

test('recipe-bound scores must exactly match their check evidence', () => {
  const passed = { ...typed('works', 'passed', null, 2), stableKey: 'pack.feature.works' };
  const scoped = {
    totals: { score: 1, max: 2, regression: null },
    selection: {
      checks: [{ stableKey: 'pack.feature.works', points: 2 }],
      attemptedChecks: ['pack.feature.works'],
      reportedChecks: ['pack.feature.works'],
      notRun: [],
    },
    suites: { feature: { features: [{ id: 'f', criteria: [passed] }] }, lint: { pass: true } },
  };
  const outcome = classifyBundle(scoped);
  assert.equal(outcome.kind, 'harness_failure');
  assert.equal(outcome.phase, 'grading');
  assert.match(outcome.reason ?? '', /reported score 1\/2 disagrees with check evidence 2\/2/);

  scoped.totals.score = 2;
  assert.equal(classifyBundle(scoped).kind, 'passed');

  scoped.selection.checks.push({ stableKey: 'pack.feature.other', points: 2 });
  scoped.selection.reportedChecks.push('pack.feature.other');
  scoped.selection.attemptedChecks.push('pack.feature.other');
  const feature = scoped.suites.feature.features[0];
  assert(feature);
  feature.criteria.push({ ...passed, id: 'duplicate' });
  scoped.totals = { score: 4, max: 4, regression: null };
  assert.equal(classifyBundle(scoped).kind, 'harness_failure',
    'duplicating one check must not stand in for a different selected check');
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
  assert.equal(outcome.levels['1']?.kind, 'app_failure');
});

test('an incomplete selected scope is never accepted through a positive denominator', () => {
  const scoped = {
    totals: { score: 1, max: 2 },
    selection: {
      checks: [{ stableKey: 'pack.feature.a', points: 1 },
        { stableKey: 'pack.feature.b', points: 1 }],
      reportedChecks: ['pack.feature.a'],
      notRun: [{ stableKey: 'pack.feature.b' }],
    },
    suites: { feature: { features: [{ id: 'f', criteria: [
      { ...typed('a', 'passed', null), stableKey: 'pack.feature.a' },
    ] }] }, lint: { pass: true } },
  };
  assert.equal(classifyBundle(scoped).kind, 'ungraded');
});

test('unknown outcome names fail closed', () => {
  assert.equal(runExitCode({ kind: 'typo' }), 1);
  assert.equal(ladderMayContinue({ kind: 'typo' }), false);
  assert.throws(() => runOutcomeKind('typo'), /invalid run outcome kind/);
});
