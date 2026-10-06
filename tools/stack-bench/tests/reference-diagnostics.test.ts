import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { assertDiagnosticDependencies, auditDiagnosticGrade, auditDiagnosticTrials, diagnosticPopulation,
  diagnosticTrials, freezeDiagnosticPlan, validateDiagnosticGroup } from '../src/references/reference-diagnostics.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('diagnostic assignments are stable; missing, interrupted and repeated executions remain visible', () => {
  const plan = diagnosticTrials('source+protocol', [9800, 9801], 2);
  assert.deepEqual(plan, diagnosticTrials('source+protocol', [9800, 9801], 2));
  assert.equal(new Set(plan.map(trial => trial.key)).size, 4);
  assert.throws(() => diagnosticTrials('source', [9800, 9800], 2));
  const interrupted = { ...plan[0]!, executionId: 'first', state: 'interrupted' as const, startedAt: 'now' };
  const retry = { ...interrupted, executionId: 'retry', state: 'collected' as const };
  const population = diagnosticPopulation(plan, [interrupted, retry]);
  assert.deepEqual({ ...population, unstarted: population.unstarted.length }, {
    planned: 4, started: 1, collected: 1, executions: 2, interrupted: 1, unstarted: 3, checkOutcomes: {},
  });
  assert.throws(() => auditDiagnosticTrials(plan, [retry, retry]), /duplicate/);
  assert.throws(() => auditDiagnosticTrials(plan, [{ ...retry, feature: 999 }]), /unassigned/);
});

test('candidate dependencies must match the installed reference; runtime dependency files do not enter the source identity', () => {
  const root = mkdtempSync(join(tmpdir(), 'diagnostic-source-'));
  try {
    const reference = join(root, 'reference'), candidate = join(root, 'candidate');
    for (const path of [reference, candidate]) {
      mkdirSync(path); writeFileSync(join(path, 'package.json'), '{"name":"app"}');
    }
    mkdirSync(join(candidate, 'node_modules'));
    writeFileSync(join(candidate, 'node_modules', 'package.json'), '{"name":"ignored-runtime"}');
    assertDiagnosticDependencies(reference, candidate);
    writeFileSync(join(candidate, 'package.json'), '{"name":"changed"}');
    assert.throws(() => assertDiagnosticDependencies(reference, candidate), /retain reference dependency/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('diagnostics require the exact checks and declared defect, with inconclusives kept separate', () => {
  const grade = (...statuses: string[]) => ({ payload: { features: [{ id: 9800,
    criteria: statuses.map((status, index) => ({ id: `c${index}`, evidence: { status } })) }] } });
  auditDiagnosticGrade(grade('inconclusive', 'passed'), 9800, ['c0', 'c1'], []);
  auditDiagnosticGrade(grade('failed', 'passed'), 9800, ['c0', 'c1'], ['c0']);
  auditDiagnosticGrade(grade('failed', 'passed'), 9800, ['c0', 'c1'], [], true);
  assert.throws(() => auditDiagnosticGrade(grade('failed', 'passed'), 9800, ['c0', 'c1'], []), /unexpected/);
  assert.throws(() => auditDiagnosticGrade(grade('harness_failure', 'passed'), 9800, ['c0', 'c1'], [], true), /unexpected/);
  assert.throws(() => auditDiagnosticGrade(grade('passed', 'passed'), 9800, ['c0', 'c1'], ['c0']), /unexpected/);
  assert.throws(() => auditDiagnosticGrade(grade('inconclusive', 'passed'), 9800, ['c0', 'c1'], ['c0']), /unexpected/);
  assert.throws(() => auditDiagnosticGrade(grade('harness_failure', 'passed'), 9800, ['c0', 'c1'], []), /unexpected/);
  assert.throws(() => auditDiagnosticGrade(grade('passed'), 9800, ['c0', 'c1'], []), /assigned checks/);
});

test('diagnostic manifest freezes source and scenario, rejects duplicate trials and score-bearing checks', () => {
  const group = { backend: 'postgres', track: 'ecommerce', level: 3,
    recipe: 'ecommerce.progression-catalog', scenario: 'tracks/ecommerce/scenarios/diagnostic-checkout-database-crash.json',
    features: [9800, 9801], repetitions: 2 };
  const plan = freezeDiagnosticPlan({ schemaVersion: 1, groups: [group] }, STACK_BENCH_ROOT);
  assert.equal(plan.groups.length, 2);
  assert.equal(plan.groups[0]!.trials.length, 2);
  assert.match(plan.groups[0]!.referenceSha256, /^[a-f0-9]{64}$/);
  assert.throws(() => freezeDiagnosticPlan({ schemaVersion: 1, groups: [group, group] }, STACK_BENCH_ROOT), /duplicate/);
  assert.throws(() => freezeDiagnosticPlan({ schemaVersion: 1, groups: [{ ...group, features: [1] }] }, STACK_BENCH_ROOT), /zero-point/);
  assert.throws(() => freezeDiagnosticPlan({ schemaVersion: 1, groups: [{ ...group, expectedFailures: ['missing'] }] }, STACK_BENCH_ROOT), /expected failure/);
  const selected = plan.groups[0]!;
  const audit = { id: 'worker', planSha256: plan.sha256, groupIndex: 0, backend: 'postgres',
    sourceSha256: selected.referenceSha256, sourceAfter: selected.referenceSha256,
    scenarioSha256: selected.scenarioSha256, controllerImage: 'image', plannedTrials: selected.trials,
    trials: [], startedAt: 'now', finishedAt: 'later', released: true, phases: {} };
  assert.throws(() => validateDiagnosticGroup(plan, 0, audit, 'image', '/worker', true), /incomplete/);
  assert.throws(() => validateDiagnosticGroup(plan, 0, { ...audit, sourceSha256: 'wrong' }, 'image', '/worker', false), /identity/);
  const trials = selected.trials.map((trial, index) => ({ ...trial, executionId: String(index),
    state: 'collected' as const, startedAt: 'now', grade: join(STACK_BENCH_ROOT, `${index}.json`) }));
  assert.throws(() => validateDiagnosticGroup(plan, 0, { ...audit, trials }, 'image', STACK_BENCH_ROOT, true), /cannot read artifact/);
  assert.throws(() => validateDiagnosticGroup(plan, 0, { ...audit, trials: trials.map(trial => ({ ...trial,
    grade: join(STACK_BENCH_ROOT, 'reused-grade.json') })) }, 'image', STACK_BENCH_ROOT, true), /does not match its execution/);
});
