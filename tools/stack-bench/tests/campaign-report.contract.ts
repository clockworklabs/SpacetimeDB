import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { emptyArtifactIdentities, readArtifact, writeArtifact,
  writeRunJson } from '../src/evidence/artifacts.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import type { CampaignAttemptPlan, CompiledCampaignPlan }
  from '../src/campaigns/campaign-compiler.js';
import { buildCampaignReport, generateCampaignReport,
  campaignRunMetrics, campaignRunFirstBuildObservations, formatDurationMs, renderCampaignHtml,
  validateCampaignReport } from '../src/campaigns/campaign-report.js';
import type { BenchmarkRun, RunSelection } from '../src/campaigns/campaign-report.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import { hashDirectory, sha256 } from '../src/evidence/provenance.js';
import { runCampaignAdmission } from '../src/campaigns/campaign-admission.js';
import { claimNextAttempt, createCampaignState, finishCampaignExecution,
  initializeCampaignDirectory, writeCampaignState } from '../src/campaigns/campaign-scheduler.js';

const projectRoot = STACK_BENCH_ROOT;
const example = join(projectRoot, 'appliance', 'campaign.example.json');
const created = '2026-08-12T00:00:00.000Z';
const compiledExample = compileCampaignFile(example);
const examplePlan = (): CompiledCampaignPlan => structuredClone(compiledExample);

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
  assert.match(report.scope.grading.levels[0]!.evidenceSha256 ?? '', /^[a-f0-9]{64}$/);
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
    levels: [{ firstBuild: { score: 5, max: 10 }, repairCostUsd: 1.25 }], totals: {} });
  assert.equal(corrected.correctionSuccessRate, 1);
  assert.equal(corrected.correctionCostUsd, 1.25);
  assert.equal(corrected.correctionSpendUsd, 1.25);

  const unresolved = campaignRunMetrics({ outcome: { kind: 'app_failure' },
    levels: [{ firstBuild: { score: 5, max: 10 }, repairCostUsd: 2 }], totals: {} });
  assert.equal(unresolved.correctionSuccessRate, 0);
  assert.equal(unresolved.correctionCostUsd, null);
  assert.equal(unresolved.correctionSpendUsd, 2);

  const unaided = campaignRunMetrics({ outcome: { kind: 'passed' },
    levels: [{ firstBuild: { score: 10, max: 10 }, repairCostUsd: 0 }], totals: {} });
  assert.equal(unaided.correctionSuccessRate, null);
  assert.equal(unaided.correctionCostUsd, null);
  assert.equal(unaided.correctionSpendUsd, null);
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
      observed: ['ecommerce.spec.state-durability@1.0.0'] },
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
    ['ecommerce.spec.state-durability@1.0.0']);
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
  assert.match(html, /The build brief lists what the coding agent is asked to build/);
  assert.match(html, /Additional measurements are reported separately/);
  assert.match(html, /product-brief-quality/);
  assert.match(html, /ecommerce\.spec\.access-control@1\.2\.0/);
  assert.match(html, /ecommerce\.spec\.transactional-integrity@1\.3\.0/);
});

test('report generation is byte-for-byte reproducible and links immutable raw evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-report-'));
  try {
    const plan = examplePlan();
    const initialized = initializeCampaignDirectory(plan, root, { now: created });
    const admission = runCampaignAdmission(plan, root, { codingContainers: () => [],
      env: {}, now: created, uuid: () => 'report',
      preflight: request => ({ schemaVersion: 1, generatedAt: created,
        request: { backends: request.backends, track: request.track, levels: request.levelList,
          runIndex: request.runIndex, parallelism: request.parallelism,
          agentAdapter: request.agentAdapter,
          packs: request.packIds, checks: request.checkKeys, image: request.image,
          resultsDir: request.resultsDir, smoke: request.smoke },
        ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] }),
    });
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
    const first = generateCampaignReport(root);
    const json = readFileSync(first.reportPath);
    const html = readFileSync(first.htmlPath);
    rmSync(join(root, 'report'), { recursive: true });
    const second = generateCampaignReport(root);
    assert.deepEqual(readFileSync(second.reportPath), json);
    assert.deepEqual(readFileSync(second.htmlPath), html);
    const artifact = readArtifact(second.reportPath, { expectedKind: 'campaign_report' });
    assert.equal(artifact.payload.contentSha256, first.report.contentSha256);
    assert.match(readFileSync(second.htmlPath, 'utf8'), /\.\.\/attempts\//);
    assert.match(readFileSync(second.htmlPath, 'utf8'), /\.\.\/admissions\//);
  } finally { rmSync(root, { recursive: true, force: true }); }
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
  assert.match(html, /prescribed@1\.1\.0/);
  const malformedScope = structuredClone(report);
  (malformedScope.scope as typeof malformedScope.scope & { surprise: boolean }).surprise = true;
  const { contentSha256: _old, ...body } = malformedScope;
  assert.throws(() => validateCampaignReport({ ...body, contentSha256: 'a'.repeat(64) }),
    /scope\.surprise is unknown/);
  const malformedCondition = structuredClone(report);
  assert.ok(malformedCondition.conditions[0]);
  (malformedCondition.conditions[0] as unknown as { condition: null }).condition = null;
  assert.throws(() => validateCampaignReport(malformedCondition), /conditions\[0\]\.condition is invalid/);
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
  assert.match(html, /not an invoice/);

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
