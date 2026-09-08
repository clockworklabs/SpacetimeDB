import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { emptyArtifactIdentities, readArtifact, writeArtifact,
  writeRunJson } from '../src/evidence/artifacts.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import type { CampaignAttemptPlan, CompiledCampaignPlan }
  from '../src/campaigns/campaign-compiler.js';
import { buildCampaignReport, campaignReportCsv, exportCampaignReport, generateCampaignReport,
  campaignRunMetrics, campaignRunFirstBuildObservations, formatDurationMs, renderCampaignHtml,
  validateCampaignReport } from '../src/campaigns/campaign-report.js';
import type { BenchmarkRun, RunSelection } from '../src/campaigns/campaign-report.js';
import type { RunSessionRecord } from '../src/evidence/benchmark-run.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import { hashDirectory, sha256 } from '../src/evidence/provenance.js';
import { readCampaignAdmission, validateCampaignAdmission } from '../src/campaigns/campaign-admission.js';
import { claimNextAttempt, createCampaignState, finishCampaignExecution,
  initializeCampaignDirectory, writeCampaignState } from '../src/campaigns/campaign-scheduler.js';
import { assessStoppedProviderContinuation, providerWaitSummary } from '../src/agents/provider-continuation-audit.js';

const projectRoot = STACK_BENCH_ROOT;
const example = join(projectRoot, 'tests', 'fixtures', 'campaign.deterministic.json');
const created = '2026-08-12T00:00:00.000Z';
const compiledExample = compileCampaignFile(example);
const examplePlan = (): CompiledCampaignPlan => structuredClone(compiledExample);

function costSession(costUsd: number, exact = true): RunSessionRecord {
  return { sessionId: null, costUsd, costComplete: true, durationMs: 1,
    usage: { input: 10, output: 20, cacheWrite: 0, cacheRead: 0 },
    costReceipts: [{ invocation: 1, receipt: { costUsd, exact, complete: true,
      reconciled: true, error: null } }],
    providerThrottle: null, resources: null, tokens: 30, outputTokens: 20, turns: 1,
    promptBytes: 1, thinking: null, transcript: null, provenance: null, providerMetadata: null };
}

function run(id: string, attempt: { id: string; condition?: CampaignAttemptPlan['condition'] },
  { score = 8, max = 10, first = 5, cost = 2, durationSec = 30 }:
  { score?: number; max?: number; first?: number; cost?: number; durationSec?: number } = {},
): BenchmarkRun {
  const passed = score === max;
  const repairs = passed ? (score === first ? 0 : 1) : 3;
  const status = passed ? (repairs ? 'corrected' : 'not-needed') : 'budget-exhausted';
  const outcome = { kind: passed ? 'passed' : 'app_failure' };
  // A valid run artifact carries the exact planned selection for each level.
  const selection = structuredClone(attempt.condition?.requested?.levels
    ?.find(item => item.level === 1)?.selection ?? null) as RunSelection | null;
  return { id, parentAttemptId: attempt.id, outcome,
    levels: [{ level: 1, ...(selection ? { selection } : {}),
      firstBuild: { score: first, max, outcome }, score, max,
      buildSessions: [costSession(repairs ? cost / 2 : cost)],
      repairSessions: repairs ? [costSession(cost / 2)] : [],
      repairCostUsd: repairs ? cost / 2 : 0, repairs,
      repair: { status, limit: 3, used: repairs, stopReason: null }, outcome }],
    totals: { score, max, costUsd: cost, costComplete: true, durationSec, repairs } };
}

function writeFakePackageEvidence(output: string, level: NonNullable<BenchmarkRun['levels']>[number]): void {
  const source = join(output, 'source');
  mkdirSync(source, { recursive: true });
  writeFileSync(join(source, 'app.js'), 'export const ready = true;\n');
  const sourceHash = hashDirectory(source).sha256;
  const checks = level.selection?.scoredChecks ?? [];
  const checkKeys = checks.map(check => check.stableKey);
  writeArtifact(join(output, 'grading', 'bundle.json'), {
    kind: 'grade_bundle', id: `fake-grade-${level.level}`, payload: {
      observation: 'scored', source: { sha256: sourceHash }, suites: { fake: { features: [{
        id: 'fake', setupEvidence: createCheckEvidence({ status: 'passed', code: 'completed',
          phase: 'setup', startedAtMs: 1, completedAtMs: 2 }),
        criteria: checks.map(check => ({ id: check.stableKey, stableKey: check.stableKey,
          points: check.points, evidence: createCheckEvidence({ status: 'passed', code: 'completed',
            phase: 'assertion', startedAtMs: 1, completedAtMs: 2 }) })),
      }] } },
      totals: { score: level.score, max: level.max },
      selection: { sha256: level.selection?.sha256, checks,
        attemptedChecks: checkKeys, reportedChecks: checkKeys, notRun: [] },
    },
  });
}

test('report read model keeps invalid evidence separate and computes declared dispersion', () => {
  const plan = examplePlan();
  let state = createCampaignState(plan, { now: created });
  const runs = new Map();
  const admissionId = 'admission-1';
  let claimed = claimNextAttempt(state, { now: '2026-08-12T00:01:00.000Z', admissionId });
  assert.ok(claimed.claim);
  state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 1, run: null, retryAuthority: { transient: true, recoveryClean: true,
      budgetKnown: true,
      cause: 'test transient provider failure' } }, { retries: 1, retryOn: ['harness_failure'],
      now: '2026-08-12T00:02:00.000Z' });
  claimed = claimNextAttempt(state, { now: '2026-08-12T00:03:00.000Z', admissionId });
  assert.ok(claimed.claim);
  runs.set(claimed.claim.executionId, run('run-1', claimed.claim.attempt));
  state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 0, run: runs.get(claimed.claim.executionId) },
    { now: '2026-08-12T00:04:00.000Z' });
  const report = buildCampaignReport(plan, state, (_attempt, execution) => runs.get(execution.id));
  assert.equal(report.summary.completedAttempts, 1);
  assert.equal(report.summary.invalidAttempts, 0);
  assert.equal(report.summary.invalidExecutions, 1);
  const firstAttempt = report.attempts[0];
  const plannedAttempt = plan.attempts[0];
  assert.ok(firstAttempt);
  assert.ok(plannedAttempt);
  assert.equal(firstAttempt.executions.length, 2);
  const condition = report.conditions.find(item => item.stack === plannedAttempt.stack);
  assert.ok(condition);
  assert.equal(condition.metrics.firstBuildScoreRate?.center, 0.5);
  assert.equal(condition.metrics.finalScoreRate?.center, 0.8);
  assert.equal(condition.metrics.totalCostUsd?.center, 2);
  assert.equal(condition.sample.invalidExecutionRate, 0.5);
  assert.deepEqual(report.scope.bindings, plan.bindings);
  assert.equal(report.scope.grading.status, 'pending');
  assert(report.scope.grading.levels.every(level => level.status === 'pending' && level.reasons.length > 0));
  assert.equal(Object.hasOwn(report.scope.bindings[0]!, 'qualification'), false);
  assert(report.limitations.some(item => /qualification is pending/.test(item)));
  assert.deepEqual(report.scope.runtime, plan.definition.runtime);
  assert.deepEqual(report.scope.pricing, plan.definition.pricing);
  assert.deepEqual(report.scope.repetitionsByStack, plan.summary.repetitionsByStack);
  assert.equal(report.scope.parallelism, plan.summary.parallelism);
  assert.equal(firstAttempt.executions[0]?.admissionEvidence,
    'admissions/admission-1.json');
  assert.match(report.contentSha256, /^[a-f0-9]{64}$/);
  assert.throws(() => validateCampaignReport({ ...report,
    summary: { ...report.summary, completedAttempts: 99 } }),
  /summary does not match its attempts/);
  const inconsistent = structuredClone(report);
  inconsistent.summary.completedAttempts = 99;
  const { contentSha256: _oldIdentity, ...inconsistentBody } = inconsistent;
  assert.throws(() => validateCampaignReport({ ...inconsistentBody,
    contentSha256: sha256(canonicalDefinitionJson(inconsistentBody)) }),
  /summary does not match its attempts/);
  const unknownSummaryField = structuredClone(report) as typeof report & {
    summary: typeof report.summary & { nonsense: boolean };
  };
  unknownSummaryField.summary.nonsense = true;
  const { contentSha256: _oldUnknownIdentity, ...unknownSummaryBody } = unknownSummaryField;
  assert.throws(() => validateCampaignReport({ ...unknownSummaryBody,
    contentSha256: sha256(canonicalDefinitionJson(unknownSummaryBody)) }),
  /summary\.nonsense is unknown/);
});

test('correction metrics separate successful cost from unresolved spend', () => {
  const corrected = campaignRunMetrics({ outcome: { kind: 'passed' },
    levels: [{ firstBuild: { score: 5, max: 10 }, repairCostUsd: 1.25,
      repairSessions: [costSession(1.25)] }], totals: {} });
  assert.equal(corrected.correctionSuccessRate, 1);
  assert.equal(corrected.correctionCostUsd, 1.25);
  assert.equal(corrected.correctionSpendUsd, 1.25);

  const unresolved = campaignRunMetrics({ outcome: { kind: 'app_failure' },
    levels: [{ firstBuild: { score: 5, max: 10 }, repairCostUsd: 2,
      repairSessions: [costSession(2)] }], totals: {} });
  assert.equal(unresolved.correctionSuccessRate, 0);
  assert.equal(unresolved.correctionCostUsd, null);
  assert.equal(unresolved.correctionSpendUsd, 2);

  const unaided = campaignRunMetrics({ outcome: { kind: 'passed' },
    levels: [{ firstBuild: { score: 10, max: 10 }, repairCostUsd: 0 }], totals: {} });
  assert.equal(unaided.correctionSuccessRate, null);
  assert.equal(unaided.correctionCostUsd, null);
  assert.equal(unaided.correctionSpendUsd, null);
});

test('all-execution spend includes invalid work and never counts resumed inherited cost twice', () => {
  const plan = examplePlan();
  let state = createCampaignState(plan, { now: created });
  const first = claimNextAttempt(state, { now: created, admissionId: 'cost-admission' });
  assert(first.claim);
  const prior = run('prior', first.claim.attempt, { cost: 2 });
  prior.outcome = { kind: 'provider_failure' };
  state = finishCampaignExecution(first.state, first.claim.executionId,
    { exitCode: 1, run: null, retryAuthority: { transient: true, recoveryClean: true,
      budgetKnown: true, cause: 'test provider failure' } },
    { retries: 1, retryOn: ['harness_failure'], now: created });
  const second = claimNextAttempt(state, { now: created, admissionId: 'cost-admission' });
  assert(second.claim);
  const resumed = run('resumed', second.claim.attempt, { cost: 3 });
  resumed.progressionResume = { priorRunId: 'prior', priorRunSha256: 'a'.repeat(64),
    stateSha256: null, action: { type: 'build', level: 1 }, inheritedLevels: [], priorTotals: null };
  resumed.totals!.costUsd = 5;
  resumed.totals!.currentExecutionCostUsd = 3;
  state = finishCampaignExecution(second.state, second.claim.executionId, { exitCode: 0, run: resumed }, { now: created });
  const report = buildCampaignReport(plan, state, (_attempt, execution) =>
    execution.id === first.claim!.executionId ? prior : resumed);
  assert.equal(report.summary.spend.costUsd, 5);
  const attempt = report.attempts.find(item => item.id === first.claim!.attempt.id)!;
  assert.deepEqual(attempt.executions.map(execution => execution.cost.costUsd), [2, 3]);
  assert.equal(attempt.metrics?.totalCostUsd, 5);
  assert.equal(attempt.spend.costUsd, 5);
  assert.equal(attempt.executions[1]?.usage.input, 20);
  assert.equal(attempt.executions[1]?.usage.build.costUsd, 1.5);
  assert.equal(attempt.executions[1]?.usage.repair.costUsd, 1.5);
});

test('time grants retain one efficacy result and expose the changed allowance', () => {
  const plan = examplePlan();
  const claimed = claimNextAttempt(createCampaignState(plan, { now: created }),
    { now: created, admissionId: 'time-admission' });
  assert(claimed.claim);
  const evidence = run('extended', claimed.claim.attempt);
  const state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 0, run: evidence }, { now: '2026-08-12T00:02:00.000Z' });
  const target = state.attempts.find(attempt => attempt.plan.id === claimed.claim!.attempt.id)!;
  const original = plan.definition.budgets.attemptTimeoutMinutes;
  target.timeGrants = [{ request: { campaignSha256: plan.contentSha256,
    attemptId: target.plan.id, executionId: claimed.claim.executionId,
    grantId: 'extra-time', minutes: 120, requestedAt: created }, disposition: 'accepted',
    acceptedAt: created, previousMinutes: original, effectiveMinutes: original + 120 }];
  const report = buildCampaignReport(plan, state, () => evidence);
  const result = report.attempts.find(attempt => attempt.id === target.plan.id)!;
  assert.equal(result.status, 'completed');
  assert.equal(report.summary.completedAttempts, 1);
  assert.equal(result.executions.length, 1);
  assert.equal(result.metrics?.totalCostUsd, 2);
  assert.equal(result.timeBudget?.effectiveMinutes, original + 120);
  assert.equal(result.timeBudget?.consumedMs, 120_000);
  assert.equal(result.timeBudget?.extensionCount, 1);
  assert.equal(plan.definition.budgets.attemptTimeoutMinutes, original);
  assert.match(campaignReportCsv(report)['attempts.csv']!, /effectiveTimeLimitMinutes/);
});

test('bounded and unknown cost remain explicit through report validation and HTML', () => {
  const plan = examplePlan();
  const claimed = claimNextAttempt(createCampaignState(plan, { now: created }), { now: created, admissionId: 'cost-admission' });
  assert(claimed.claim);
  const bounded = run('bounded', claimed.claim.attempt, { cost: 3 });
  bounded.levels![0]!.buildSessions = [costSession(1.5, false)];
  const state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 0, run: bounded }, { now: created });
  const report = buildCampaignReport(plan, state, () => bounded);
  const execution = report.attempts.find(attempt => attempt.executions.length)!.executions[0]!;
  assert.deepEqual(execution.cost, { status: 'upper-bound', costUsd: 3 });
  assert.equal(execution.metrics?.totalCostUsd, null);
  assert.equal(execution.metrics?.totalCostUpperBoundUsd, 3);
  assert.equal(report.summary.spend.status, 'upper-bound');
  assert.match(renderCampaignHtml(report), /≤ \$3/);
  const malformed = structuredClone(report);
  const mutable = malformed.attempts.find(attempt => attempt.executions.length)!.executions[0]!;
  mutable.cost = { status: 'unknown', costUsd: 0 } as unknown as typeof mutable.cost;
  assert.throws(() => validateCampaignReport(malformed));
  delete bounded.totals!.costUsd;
  const missing = buildCampaignReport(plan, state, () => bounded);
  assert.equal(missing.summary.spend.costUsd, null);
  assert.equal(missing.summary.spend.unknownExecutions, 1);
  const measured = missing.attempts.find(attempt => attempt.executions.length)!;
  assert.equal(measured.status, 'completed', 'cost uncertainty alone does not erase a valid outcome');
  assert.equal(measured.metrics?.finalScoreRate, 0.8);
  assert.equal(measured.metrics?.totalCostUsd, null);
  assert.match(renderCampaignHtml(missing), /Unknown/);
});

test('historical zero-usage actions remain ineligible and keep earlier bounded costs', () => {
  const plan = examplePlan();
  const claimed = claimNextAttempt(createCampaignState(plan, { now: created }),
    { now: created, admissionId: 'provider-admission' });
  assert(claimed.claim);
  const evidence = run('provider-stopped', claimed.claim.attempt, { cost: 3 });
  evidence.levels![0]!.buildSessions = [costSession(3, false)];
  const stopped = costSession(0);
  stopped.usage = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 };
  stopped.providerMetadata = { failureCode: 'provider-throttle-exhausted', invocations: 4,
    providerWaits: [{ waitedMs: 1000, disposition: 'continued' }, { waitedMs: 2000, disposition: 'stopped' }] };
  stopped.costReceipts = Array.from({ length: 4 }, (_, index) => ({ invocation: index + 1,
    receipt: { costUsd: 0, complete: true, reconciled: true, error: null, exact: true,
      usage: { input: 0, output: 0, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 } } }));
  evidence.levels![0]!.repairSessions = [stopped];
  const state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 1, run: evidence }, { now: created });
  const report = buildCampaignReport(plan, state, () => evidence);
  const execution = report.attempts.find(attempt => attempt.executions.length)!.executions[0]!;
  assert.equal(execution.providerContinuation?.work, 'zero-usage-candidate');
  assert.equal(execution.providerContinuation?.eligible, false);
  assert.deepEqual(execution.providerContinuation?.runCost, { status: 'upper-bound', costUsd: 3 });
  assert.deepEqual(execution.providerWaits, { waits: 2, waitedMs: 3000, continued: 1, stopped: 1 });
  assert.equal(providerWaitSummary({ ...evidence, progressionResume: { inheritedLevels: [1] } }), null);
  assert.match(renderCampaignHtml(report), /Provider continuation ineligible/);
  assert.match(campaignReportCsv(report)['executions.csv']!, /zero-usage-candidate/);
  assert.deepEqual(validateCampaignReport(report), report);
  const interrupted = buildCampaignReport(plan, state, () => { throw new Error('no final run'); },
    () => ({ waits: 1, waitedMs: 25, continued: 0, stopped: 0, waiting: 1, durationKind: 'lower-bound' }));
  const interruptedExecution = interrupted.attempts.find(attempt => attempt.executions.length)!.executions[0]!;
  assert.equal(interruptedExecution.providerWaits?.waiting, 1);
  assert.match(renderCampaignHtml(interrupted), /provider waits 1, at least/);
  assert.deepEqual(validateCampaignReport(interrupted), interrupted);

  // A zero final retry does not erase paid work in an earlier invocation.
  stopped.usage.output = 1;
  assert.equal(assessStoppedProviderContinuation(evidence)?.work, 'paid');
  stopped.usage.output = 0;
  stopped.costReceipts.pop();
  assert.equal(assessStoppedProviderContinuation(evidence)?.work, 'unknown');
  stopped.costReceipts = [];
  assert.equal(assessStoppedProviderContinuation(evidence)?.work, 'unknown');
  assert.equal(assessStoppedProviderContinuation({ levels: [] }), null);
});

test('campaign metrics do not treat incomplete cost as comparable evidence', () => {
  const incomplete = campaignRunMetrics({ outcome: { kind: 'passed' }, levels: [],
    totals: { costUsd: 4.25, costComplete: false, durationSec: 30 } });
  assert.equal(incomplete.totalCostUsd, null);
  assert.equal(incomplete.totalDurationMs, 30_000);

  const zero = campaignRunMetrics({ outcome: { kind: 'passed' }, levels: [],
    totals: { costUsd: 0, costComplete: true } });
  assert.equal(zero.totalCostUsd, 0);
});

test('dependency campaign final score is passed points over all points, with the questline average beside it', () => {
  const metrics = campaignRunMetrics({
    progressionStatus: { phase: 'terminal', score: { questlineAveragePercentage: 62.5,
      uniqueChecks: { gradedPoints: 16, availablePoints: 20, percentage: 70 } } },
    levels: [{ firstBuild: { score: 4, max: 10 }, score: 10, max: 10 }],
    totals: { score: 30, max: 30 },
  });
  assert.equal(metrics.finalScoreRate, 0.7);
  assert.equal(metrics.questlineAverageRate, 0.625);
  assert.equal(metrics.finalCoverageRate, 0.8);

  const active = campaignRunMetrics({
    progressionStatus: { phase: 'active', score: { questlineAveragePercentage: null,
      uniqueChecks: { gradedPoints: 16, availablePoints: 20, percentage: null } } },
    levels: [{ firstBuild: { score: 4, max: 10 }, score: 10, max: 10 }],
    totals: { score: 30, max: 30 },
  });
  assert.equal(active.finalScoreRate, null);
  assert.equal(active.questlineAverageRate, null);
  assert.equal(active.finalCoverageRate, null);
});

test('completed process with inconclusive dependency grading has no final completion metric', () => {
  const plan = examplePlan();
  const claimed = claimNextAttempt(createCampaignState(plan), { admissionId: 'active-grade' });
  assert.ok(claimed.claim);
  const incomplete = run('active-grade', claimed.claim.attempt, { cost: 5.059995 });
  incomplete.levels!.push({ level: 2, graded: false,
    firstBuild: { score: 34, max: 70 }, outcome: { kind: 'app_failure',
      appFailures: ['session-reload'], inconclusive: ['no-session'] } });
  incomplete.totals!.ungraded = [2];
  incomplete.progressionStatus = { phase: 'active', score: {
    completion: { selected: 107, passed: 7, failed: 2, blocked: 0, unmeasured: 98, rate: 0.065421 },
    uniqueChecks: { gradedPoints: 12, availablePoints: 173, percentage: null },
  } };
  const state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 0, run: incomplete });
  assert.equal(state.attempts.find(item => item.plan.id === claimed.claim!.attempt.id)!.status, 'invalid');
  // Existing states keep their recorded claim; report correction must not rewrite it.
  const oldAttempt = state.attempts.find(item => item.plan.id === claimed.claim!.attempt.id)!;
  oldAttempt.status = 'completed';
  oldAttempt.executions[0]!.status = 'completed';
  oldAttempt.executions[0]!.outcome = incomplete.outcome!.kind as 'passed';
  const report = buildCampaignReport(plan, state, () => incomplete);
  const attempt = report.attempts.find(item => item.id === claimed.claim!.attempt.id)!;
  assert.equal(attempt.status, 'invalid');
  assert.equal(attempt.executions[0]!.outcome, 'incomplete');
  assert.equal(attempt.executions[0]!.recordedStatus, 'completed');
  assert.equal(attempt.executions[0]!.recordedOutcome, incomplete.outcome!.kind);
  assert.equal(oldAttempt.status, 'completed');
  assert.equal(attempt.executions[0]!.metrics?.checkCompletionRate, null);
  assert.equal(attempt.metrics, null);
  assert.equal(attempt.completion.passed, 7);
  assert.equal(attempt.completion.unmeasured, 98);
  assert.equal(attempt.spend.costUsd, 5.059995);
  const cohort = report.conditions.find(item => item.stack === attempt.stack)!;
  assert.equal(cohort.metrics.checkCompletionRate?.n, 0);
  assert.equal(cohort.metrics.checkCompletionRate?.center, null);
  const csv = campaignReportCsv(report)['attempts.csv']!;
  assert.match(csv, /"107","7","2","0","98","",""/);
  assert.doesNotMatch(csv, /0\.065421/);
  assert.match(csv, /sumOfAvailableExactCostsAndUpperBoundsUsd/);
  assert.match(report.limitations.join(' '), /must not be assumed to be a lower bound/);
});

test('reported duration excludes provider throttle waits and tokens travel with usage', () => {
  const metrics = campaignRunMetrics({
    outcome: { kind: 'passed' },
    levels: [{ score: 9, max: 9, sessionTotals: { sessions: 2, costUsd: 1, tokens: 2_400_000,
      outputTokens: 1, turns: 1, durationMs: 60_000, activeDurationMs: 45_000,
      providerThrottle: { waits: 1, waitedMs: 15_000 }, promptBytes: 0,
      usage: { input: 0, output: 0, cacheWrite: 0, cacheRead: 0 }, thinking: null } }],
    totals: { score: 9, max: 9, costUsd: 1, costComplete: true, tokens: 2_400_000,
      durationSec: 100 },
  });
  assert.equal(metrics.totalDurationMs, 85_000);
  assert.equal(metrics.totalTokens, 2_400_000);
});

test('score rates keep inconclusive points separate from measurement coverage', () => {
  const inconclusive = { kind: 'inconclusive', inconclusive: ['contention/203/203b'],
    harnessFailures: [] };
  const selection = { checks: [{ executionId: 'contention', featureId: 203,
    criterionId: '203b', points: 1 }] };
  const metrics = campaignRunMetrics({ outcome: inconclusive, levels: [{
    firstBuild: { score: 8, max: 9, outcome: inconclusive },
    score: 9, max: 9, selection, outcome: inconclusive, repairCostUsd: 1,
  }], totals: { score: 9, max: 9 } });
  assert.equal(metrics.firstBuildScoreRate, 0.888889);
  assert.equal(metrics.finalScoreRate, 1);
  assert.equal(metrics.firstBuildCoverageRate, 0.9);
  assert.equal(metrics.finalCoverageRate, 0.9);

  const unmapped = campaignRunMetrics({ outcome: inconclusive, levels: [{
    firstBuild: { score: 8, max: 9, outcome: inconclusive },
    score: 9, max: 9, selection: { checks: [] }, outcome: inconclusive,
  }], totals: { score: 9, max: 9 } });
  assert.equal(unmapped.firstBuildCoverageRate, null);
  assert.equal(unmapped.finalCoverageRate, null);
});

test('observed-only first-build behavior remains separate from scored results and repairs', () => {
  const evidence = run('probe-run', { id: 'probe-attempt' },
    { score: 14, max: 14, first: 14, cost: 0 });
  const evidenceLevel = evidence.levels?.[0];
  assert.ok(evidenceLevel);
  evidenceLevel.selection = {
    specifications: { requested: [], expected: [],
      observed: ['ecommerce.spec.state-durability'] },
    observedChecks: [
      { stableKey: 'durability/session', points: 1 },
      { stableKey: 'durability/cart', points: 2 },
    ],
  };
  assert.ok(evidenceLevel.firstBuild);
  evidenceLevel.firstBuild.source = { sha256: 'a'.repeat(64), files: 3 };
  evidenceLevel.firstBuild.observations = {
    sourceSha256: 'a'.repeat(64),
    selectionSha256: 'b'.repeat(64),
    selectedChecks: ['durability/session', 'durability/cart'],
    reportedChecks: ['durability/session', 'durability/cart'],
    passedPoints: 1,
    observedPoints: 3,
    scoreContribution: false,
    repairVisible: false,
    artifact: 'first-build-l1-observed/bundle.json',
    outcome: { kind: 'app_failure' },
  };

  const scored = campaignRunMetrics(evidence);
  const observed = campaignRunFirstBuildObservations(evidence);
  assert.ok(observed);
  assert.equal(scored.firstBuildScoreRate, 1);
  assert.equal(scored.finalScoreRate, 1);
  assert.equal(scored.correctionSuccessRate, null);
  assert.equal(observed.selectedPoints, 3);
  assert.equal(observed.observedPoints, 3);
  assert.equal(observed.passedPoints, 1);
  assert.equal(observed.passRate, 0.333333);
  assert.equal(observed.coverageRate, 1);
  assert.equal(observed.scoreContribution, false);
  assert.equal(observed.repairVisible, false);
  assert.deepEqual(observed.levels[0]?.specifications,
    ['ecommerce.spec.state-durability']);
});

test('campaign HTML labels observed-only behavior as zero-score first-build observations', () => {
  const plan = examplePlan();
  let state = createCampaignState(plan, { now: created });
  const claimed = claimNextAttempt(state, { now: created, admissionId: 'probe-admission' });
  assert.ok(claimed.claim);
  const claim = claimed.claim;
  const evidence = run('probe-run', claim.attempt,
    { score: 10, max: 10, first: 10, cost: 0 });
  const evidenceLevel = evidence.levels?.[0];
  assert.ok(evidenceLevel);
  evidenceLevel.selection = { specifications: { requested: [], expected: [],
    observed: ['durability@1'] },
  observedChecks: [{ stableKey: 'durability/session', points: 1 }] };
  assert.ok(evidenceLevel.firstBuild);
  evidenceLevel.firstBuild.observations = { sourceSha256: 'a'.repeat(64),
    selectionSha256: 'b'.repeat(64), selectedChecks: ['durability/session'],
    reportedChecks: ['durability/session'], passedPoints: 1, observedPoints: 1,
    scoreContribution: false, repairVisible: false,
    artifact: 'first-build-l1-observed/bundle.json', outcome: { kind: 'passed' } };
  state = finishCampaignExecution(claimed.state, claim.executionId,
    { exitCode: 0, run: evidence }, { now: '2026-08-12T00:02:00.000Z' });
  const report = buildCampaignReport(plan, state, () => evidence);
  const html = renderCampaignHtml(report);
  const condition = report.conditions.find(item => item.stack === claim.attempt.stack);
  assert.ok(condition?.firstBuildObservations);
  assert.equal(condition.firstBuildObservations.sample.selectedAttempts, 1);
  assert.equal(condition.firstBuildObservations.sample.measuredAttempts, 1);
  assert.equal(condition.firstBuildObservations.metrics.passRate.center, 1);
  assert.match(html, /Additional first-build measurements/);
  assert.match(html, /Provisional scores/);
  assert.match(html, /add no points to the score/);
  assert.match(html, /do not enter repair feedback/);
  assert.match(html, /first-build-l1-observed\/bundle\.json/);
});

test('campaign HTML states the build and evaluation setup in plain language', () => {
  const plan = compileCampaignFile(join(projectRoot, 'appliance',
    'campaign.product-brief-reference.json'));
  const state = createCampaignState(plan, { now: created });
  const report = buildCampaignReport(plan, state, () => {
    throw new Error('a pending campaign must not read run evidence');
  });
  const html = renderCampaignHtml(report);
  assert.match(html, /What this run asks for and tests/);
  assert.match(html, /Cost and measured completion/);
  assert.doesNotMatch(html, /Cost and verified completion/);
  assert.match(html, /Reference-fixture attempts use hand-written apps and make no model calls/);
  assert.match(html, /do not measure model implementation ability or comparative token efficiency/);
  assert.match(html, /The build brief lists what the coding agent is asked to build/);
  assert.match(html, /Additional measurements are reported separately/);
  assert.match(html, /product-brief-quality/);
  assert.match(html, /ecommerce\.spec\.access-control/);
  assert.match(html, /ecommerce\.spec\.transactional-integrity/);
});

test('report generation is byte-for-byte reproducible and links immutable raw evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-report-'));
  const exported = `${root}-export`;
  try {
    const plan = examplePlan();
    const initialized = initializeCampaignDirectory(plan, root, { now: created });
    const admission = { id: 'report-admission' };
    writeArtifact(join(root, 'admissions', `${admission.id}.json`), {
      kind: 'campaign_admission', id: admission.id,
      identities: emptyArtifactIdentities({ experiment: { id: plan.id, version: plan.version,
        sha256: plan.contentSha256, state: plan.state } }),
      payload: { schemaVersion: 1, campaignId: plan.id, campaignSha256: plan.contentSha256,
        createdAt: created, ok: true, runtime: plan.definition.runtime, conditions: plan.conditions,
        agents: plan.agents.map(agent => ({ adapter: agent.adapter, model: agent.model, identity: agent.identity })),
        reports: plan.agents.flatMap(agent => Array.from({ length: plan.summary.parallelism }, (_, runIndex) => ({
          schemaVersion: 1, generatedAt: created, ok: true,
          request: { backends: plan.stacks.map(stack => stack.id), track: plan.definition.track,
            levels: plan.definition.levels, runIndex, parallelism: plan.summary.parallelism,
            agentAdapter: agent.adapter, packs: plan.definition.selection.packs ?? [],
            checks: plan.definition.selection.checks ?? [], image: plan.definition.runtime.buildImage ?? 'test-image',
            resultsDir: '/var/lib/docker/volumes/original/results/campaign', smoke: false },
          summary: { passed: 0, failed: 0, warnings: 0 }, checks: [],
        }))),
      },
    });
    assert.throws(() => readCampaignAdmission(root, admission.id, plan), /compiled scope/);
    const historical = readCampaignAdmission(root, admission.id, plan, { allowRelocatedEvidence: true });
    for (const report of historical.reports) report.request.resultsDir = 'C:\\original\\campaign';
    assert.doesNotThrow(() => validateCampaignAdmission(historical, plan, root, { allowRelocatedEvidence: true }));
    historical.reports[0]!.request.resultsDir = 'relative-origin';
    assert.throws(() => validateCampaignAdmission(historical, plan, root,
      { allowRelocatedEvidence: true }), /compiled scope/);
    const claimed = claimNextAttempt(initialized.state,
      { now: '2026-08-12T00:01:00.000Z', admissionId: admission.id });
    assert.ok(claimed.claim);
    const claim = claimed.claim;
    const output = join(root, claim.output);
    mkdirSync(output, { recursive: true });
    const evidence = run('run-1', claim.attempt,
      { score: 58, max: 58, first: 58, cost: 2, durationSec: 30 });
    const timestamp = '2026-08-12T00:01:30.000Z';
    const agent = plan.agents.find(item => item.adapter === claim.attempt.agentAdapter);
    const stack = plan.stacks.find(item => item.id === claim.attempt.stack);
    assert.ok(agent);
    assert.ok(stack);
    const level = evidence.levels?.[0];
    assert.ok(level);
    writeFakePackageEvidence(output, level);
    writeRunJson(join(output, 'run.json'), { ...evidence, startedAt: timestamp, completedAt: timestamp,
      track: plan.definition.track, backend: claim.attempt.stack,
      model: claim.attempt.model, guidance: claim.attempt.guidance,
      condition: claim.attempt.condition, mode: claim.attempt.mode, pricing: claim.attempt.pricing,
      skills: claim.attempt.skills, selectionRequest: plan.definition.selection,
      runtime: { buildImage: null }, identities: emptyArtifactIdentities({
        engine: plan.identities.engine, experiment: { id: plan.id, version: plan.version,
          sha256: plan.contentSha256, state: plan.state },
        agentAdapter: agent.identity, stackAdapter: stack,
      }) });
    const state = finishCampaignExecution(claimed.state, claim.executionId,
      { exitCode: 0, run: evidence }, { now: '2026-08-12T00:02:00.000Z' });
    writeCampaignState(initialized.paths.state, plan, state);
    mkdirSync(join(root, '.private'), { recursive: true });
    writeFileSync(join(root, '.private', 'authority.json'), '{"credential":"do-not-export"}');
    writeFileSync(join(root, 'transcript.json'), '{"authorization":"do-not-export"}');
    const first = generateCampaignReport(root);
    const manifest = JSON.parse(readFileSync(first.exportManifestPath, 'utf8'));
    assert.ok(manifest.files.some((file: { path: string }) => file.path === 'plan.json'));
    assert.ok(manifest.files.some((file: { path: string }) => file.path === 'report/report.html'));
    assert.ok(manifest.files.every((file: { path: string }) => !file.path.includes('.private') && !file.path.includes('transcript')));
    assert.equal(manifest.campaignSha256, plan.contentSha256);
    assert.match(manifest.reconstruction, /Partial/);
    const manifestBytes = readFileSync(first.exportManifestPath);
    const json = readFileSync(first.reportPath);
    const html = readFileSync(first.htmlPath);
    rmSync(join(root, 'report'), { recursive: true });
    const second = generateCampaignReport(root);
    assert.deepEqual(readFileSync(second.reportPath), json);
    assert.deepEqual(readFileSync(second.htmlPath), html);
    assert.deepEqual(readFileSync(second.exportManifestPath), manifestBytes);
    const artifact = readArtifact(second.reportPath, { expectedKind: 'campaign_report' });
    assert.equal(artifact.payload.contentSha256, first.report.contentSha256);
    assert.match(readFileSync(second.htmlPath, 'utf8'), /\.\.\/attempts\//);
    assert.match(readFileSync(second.htmlPath, 'utf8'), /\.\.\/admissions\//);
    const invalid = finishCampaignExecution(claimed.state, claim.executionId,
      { exitCode: 1, run: null }, { now: '2026-08-12T00:02:00.000Z' });
    writeCampaignState(initialized.paths.state, plan, invalid);
    const invalidReport = generateCampaignReport(root).report;
    assert.equal(invalidReport.summary.invalidExecutions, 1);
    assert.equal(invalidReport.summary.spend.knownCostUsd, 2, 'valid receipts survive an invalid outcome in the persisted artifact');
    assert.equal(invalidReport.summary.spend.unknownExecutions, 0);
    assert.throws(() => exportCampaignReport(root, join(root, 'nested-export')), /outside/);
    assert.equal(exportCampaignReport(root, exported), exported);
    const portable = JSON.parse(readFileSync(join(exported, 'export-manifest.json'), 'utf8'));
    for (const file of portable.files) {
      const bytes = readFileSync(join(exported, file.path));
      assert.equal(bytes.length, file.bytes);
      assert.equal(sha256(bytes), file.sha256);
    }
    assert.ok(existsSync(join(exported, 'report/report.html')));
    assert.ok(existsSync(join(exported, 'plan.json')));
    assert.ok(!existsSync(join(exported, '.private')));
    assert.ok(!existsSync(join(exported, claim.output, 'source')));
    assert.ok(!existsSync(join(exported, 'transcript.json')));
    assert.deepEqual(readFileSync(join(exported, 'report/report.html')), readFileSync(join(root, 'report/report.html')));
    const csv = campaignReportCsv(invalidReport);
    assert.equal(readFileSync(join(exported, 'executions.csv'), 'utf8'), csv['executions.csv']);
    assert.match(csv['executions.csv']!, /"invalid","harness_failure","invalid","harness_failure","exact","2"/);
    assert.match(readFileSync(join(exported, 'README.txt'), 'utf8'), /Partial research export/);
    assert.throws(() => exportCampaignReport(root, exported), /must not exist/);
  } finally {
    rmSync(root, { recursive: true, force: true });
    rmSync(exported, { recursive: true, force: true });
  }
});

test('dependency HTML distinguishes accepted completion from raw grade outcomes', () => {
  const plan = examplePlan();
  const state = createCampaignState(plan);
  for (const attempt of state.attempts) attempt.plan.mode = { ...attempt.plan.mode, id: 'dependency' };
  const report = buildCampaignReport(plan, state, () => { throw new Error('pending'); });
  const completion = structuredClone(report.attempts[0]!.completion);
  const html = renderCampaignHtml(report);
  assert.ok(html.includes(`0/${completion.selected}`));
  assert.ok(html.includes(`${completion.unmeasured} without an accepted outcome`));
  assert.match(html, /does not separate prerequisite deferral from missing conclusive evidence/);
  assert.deepEqual(report.attempts[0]!.completion, completion);
});

test('CSV preserves blank unknowns, zero costs, denominators and inert spreadsheet labels', () => {
  const plan = examplePlan();
  const report = buildCampaignReport(plan, createCampaignState(plan), () => { throw new Error('pending'); });
  const attempt = report.attempts[0]!;
  attempt.id = '=SUM(1,2)';
  attempt.completion = { selected: 9, passed: 2, failed: 3, blocked: 1, unmeasured: 3, rate: 2 / 9 };
  attempt.spend = { status: 'unknown', costUsd: null, knownCostUsd: 0,
    unknownExecutions: 1, boundedExecutions: 0 };
  const csv = campaignReportCsv(report)['attempts.csv']!;
  assert.match(csv, /"'=SUM\(1,2\)"/);
  assert.match(csv, /"9","2","3","1","3"/);
  assert.match(csv, /"unknown","","0","1","0"/);
  assert.match(csv, /"diagnosticPassedChecks"/);
  assert.match(csv, /"9","2","3","1","3","",""/,
    'pending attempts have no outcome rate despite retained diagnostic counts');
  attempt.status = 'invalid';
  assert.match(campaignReportCsv(report)['attempts.csv']!, /"9","2","3","1","3","",""/,
    'invalid retained checkpoint counts do not become eligible outcome metrics');
  attempt.status = 'completed';
  attempt.metrics = { checkCompletionRate: 2 / 9 };
  assert.match(campaignReportCsv(report)['attempts.csv']!, /"9","2","3","1","3","0\.2222222222222222"/);
  attempt.spend = { status: 'upper-bound', costUsd: 4, knownCostUsd: 4,
    unknownExecutions: 0, boundedExecutions: 1 };
  assert.match(campaignReportCsv(report)['attempts.csv']!, /"upper-bound","4","4","0","1"/);
});

test('HTML escapes caller-controlled labels and reports exact scope', () => {
  const plan = examplePlan();
  const state = createCampaignState(plan, { now: created });
  const report = buildCampaignReport({ ...plan, title: '<script>' }, state, () => {
    throw new Error('a pending campaign must not read run evidence');
  });
  const html = renderCampaignHtml(report);
  assert.doesNotMatch(html, /<script>/);
  assert.match(html, /&lt;script&gt;/);
  assert.match(html, /Study condition/);
  assert.match(html, /<td>prescribed<\/td>/);
  const malformedScope = structuredClone(report);
  (malformedScope.scope as typeof malformedScope.scope & { surprise: boolean }).surprise = true;
  const { contentSha256: _old, ...body } = malformedScope;
  assert.throws(() => validateCampaignReport({ ...body, contentSha256: 'a'.repeat(64) }),
    /scope\.surprise is unknown/);
  const malformedCondition = structuredClone(report);
  assert.ok(malformedCondition.conditions[0]);
  (malformedCondition.conditions[0] as unknown as { condition: null }).condition = null;
  assert.throws(() => validateCampaignReport(malformedCondition), /conditions\[0\]\.condition/);
});

test('human reports format normalized usage and elapsed time for people', () => {
  assert.equal(formatDurationMs(4_893_000), '1h 21m 33s');
  assert.equal(formatDurationMs(125_000), '2m 5s');
  assert.equal(formatDurationMs(9_000), '9s');
  const plan = examplePlan();
  let state = createCampaignState(plan, { now: created });
  const claimed = claimNextAttempt(state, { now: created, admissionId: 'admission-format' });
  assert.ok(claimed.claim);
  const evidence = run('run-format', claimed.claim.attempt,
    { cost: 19.899, durationSec: 4_893 });
  state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 0, run: evidence }, { now: '2026-08-12T01:21:33.000Z' });
  const html = renderCampaignHtml(buildCampaignReport(plan, state, () => evidence));
  assert.match(html, /\$19\.899 API-equivalent usage/);
  assert.doesNotMatch(html, /normalized usage/);
  assert.match(html, /1h 21m 33s/);
  assert.match(html, /First-build score/);
  assert.match(html, /\(n=1\)/);
  assert.match(html, /100% coverage/);
  assert.doesNotMatch(html, />4893s</);
  assert.match(html, /spread is reported only from three or more/i);
  assert.match(html, /not (?:an invoice|invoices)/);

  const costPlan = structuredClone(plan);
  costPlan.definition.analysis.primaryMetric = 'totalCostUsd';
  const costHtml = renderCampaignHtml(buildCampaignReport(costPlan, state, () => evidence));
  assert.match(costHtml, /<th>totalCostUsd<\/th>/);
  assert.match(costHtml, />\$19\.899 \(n=1\)<br><small>\$19\.899 API-equivalent usage/);
  assert.doesNotMatch(costHtml, /% coverage/);
});

test('a spread needs three completed attempts', () => {
  // Balanced rotation hands every stack and condition its k-th attempt before
  // any receives its (k+1)-th, so completing two rounds leaves each group at
  // n=2 and the third round takes each to n=3.
  const plan = examplePlan();
  let state = createCampaignState(plan, { now: created });
  const evidence = new Map<string, ReturnType<typeof run>>();
  const completions = new Map<string, number>();
  const groups = plan.stacks.length * plan.conditions.length;
  const complete = (ordinal: number) => {
    const claimed = claimNextAttempt(state, { now: created, admissionId: `admission-${ordinal}` });
    assert.ok(claimed.claim);
    const { attempt } = claimed.claim;
    const group = `${attempt.stack} ${attempt.condition.id}`;
    const cost = (completions.get(group) ?? 0) + 1;
    completions.set(group, cost);
    const record = run(`run-spread-${ordinal}`, attempt, { cost });
    evidence.set(claimed.claim.executionId, record);
    state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
      { exitCode: 0, run: record }, { now: created });
  };
  const report = () => buildCampaignReport(plan, state, (_attempt, execution) => {
    const record = evidence.get(execution.id);
    assert.ok(record, `evidence for ${execution.id}`);
    return record;
  });

  for (let ordinal = 0; ordinal < groups * 2; ordinal += 1) complete(ordinal);
  for (const condition of report().conditions) {
    assert.equal(condition.metrics.totalCostUsd?.n, 2);
    assert.equal(condition.metrics.totalCostUsd?.spread, null);
    assert.deepEqual([condition.metrics.totalCostUsd?.min, condition.metrics.totalCostUsd?.max], [1, 2]);
  }

  for (let ordinal = groups * 2; ordinal < groups * 3; ordinal += 1) complete(ordinal);
  for (const condition of report().conditions) {
    assert.equal(condition.metrics.totalCostUsd?.n, 3);
    assert.notEqual(condition.metrics.totalCostUsd?.spread, null);
  }
});

test('variant cohorts stay separate even when their stack and condition match', () => {
  const plan = examplePlan();
  const state = createCampaignState(plan, { now: created });
  const baseline = buildCampaignReport(plan, state, () => { throw new Error('pending'); });
  const first = state.attempts[0]!;
  const repeat = state.attempts.find(attempt => attempt.plan.id !== first.plan.id
    && attempt.plan.stack === first.plan.stack && attempt.plan.condition.id === first.plan.condition.id)!;
  assert.ok(repeat);
  repeat.plan.levels = [1, 2];
  const varied = buildCampaignReport(plan, state, () => { throw new Error('pending'); });
  assert.equal(varied.conditions.length, baseline.conditions.length + 1);
});

test('report output cannot cross a symbolic link inside the campaign directory', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-report-link-'));
  const outside = mkdtempSync(join(tmpdir(), 'stack-bench-report-outside-'));
  try {
    initializeCampaignDirectory(examplePlan(), root, { now: created });
    const output = join(root, 'linked-report');
    symlinkSync(outside, output, process.platform === 'win32' ? 'junction' : 'dir');
    assert.throws(() => generateCampaignReport(root, { output }), /symbolic link/);
    assert.throws(() => generateCampaignReport(root, { output: join(output, 'nested') }), /symbolic link/);
    const alias = join(outside, 'campaign-link');
    symlinkSync(root, alias, process.platform === 'win32' ? 'junction' : 'dir');
    assert.throws(() => exportCampaignReport(root, join(alias, 'export')), /outside/);
  } finally {
    rmSync(root, { recursive: true, force: true });
    rmSync(outside, { recursive: true, force: true });
  }
});
