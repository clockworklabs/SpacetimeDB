import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { emptyArtifactIdentities, readArtifact, writeRunJson } from '../artifacts.mjs';
import { compileCampaignFile } from '../campaign-compiler.mjs';
import { buildCampaignReport, generateCampaignReport,
  campaignRunMetrics, campaignRunFirstBuildObservations, formatDurationMs, renderCampaignHtml,
  validateCampaignReport } from '../campaign-report.mjs';
import { canonicalDefinitionJson } from '../definition-plan.mjs';
import { sha256 } from '../provenance.mjs';
import { runCampaignAdmission } from '../campaign-runner.mjs';
import { claimNextAttempt, createCampaignState, finishCampaignExecution,
  initializeCampaignDirectory, writeCampaignState } from '../campaign-scheduler.mjs';

const example = join(import.meta.dirname, '..', 'appliance', 'campaign.example.json');
const created = '2026-08-12T00:00:00.000Z';

function run(id, attempt, { score = 8, max = 10, first = 5, cost = 2, durationSec = 30 } = {}) {
  const passed = score === max;
  const fixRounds = passed ? (score === first ? 0 : 1) : 3;
  const status = passed ? (fixRounds ? 'corrected' : 'not-needed') : 'budget-exhausted';
  const outcome = { kind: passed ? 'passed' : 'app_failure' };
  return { id, parentAttemptId: attempt.id, outcome,
    levels: [{ level: 1, firstBuild: { score: first, max, outcome }, score, max,
      fixCostUsd: fixRounds ? cost / 2 : 0, fixRounds,
      repair: { status, budgetRounds: 3, roundsUsed: fixRounds, stopReason: null }, outcome }],
    totals: { score, max, costUsd: cost, durationSec, fixRounds } };
}

test('report read model keeps invalid evidence separate and computes declared dispersion', () => {
  const plan = compileCampaignFile(example);
  let state = createCampaignState(plan, { now: created });
  const runs = new Map();
  const admissionId = 'admission-1';
  let claimed = claimNextAttempt(state, { now: '2026-08-12T00:01:00.000Z', admissionId });
  state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 1, run: null }, { retries: 1, retryOn: ['harness_failure'],
      now: '2026-08-12T00:02:00.000Z' });
  claimed = claimNextAttempt(state, { now: '2026-08-12T00:03:00.000Z', admissionId });
  runs.set(claimed.claim.executionId, run('run-1', claimed.claim.attempt));
  state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 0, run: runs.get(claimed.claim.executionId) },
    { now: '2026-08-12T00:04:00.000Z' });
  const report = buildCampaignReport(plan, state, (_attempt, execution) => runs.get(execution.id));
  assert.equal(report.summary.completedAttempts, 1);
  assert.equal(report.summary.invalidAttempts, 0);
  assert.equal(report.summary.invalidExecutions, 1);
  assert.equal(report.attempts[0].executions.length, 2);
  const condition = report.conditions.find(item => item.stack === plan.attempts[0].stack);
  assert.equal(condition.metrics.firstBuildScoreRate.center, 0.5);
  assert.equal(condition.metrics.finalScoreRate.center, 0.8);
  assert.equal(condition.metrics.totalCostUsd.center, 2);
  assert.equal(condition.sample.invalidExecutionRate, 0.5);
  assert.deepEqual(report.scope.bindings, plan.bindings);
  assert.deepEqual(report.scope.runtime, plan.definition.runtime);
  assert.deepEqual(report.scope.pricing, plan.definition.pricing);
  assert.deepEqual(report.scope.repetitionsByStack, plan.summary.repetitionsByStack);
  assert.equal(report.scope.parallelism, plan.summary.parallelism);
  assert.equal(report.attempts[0].executions[0].admissionEvidence,
    'admissions/admission-1.json');
  assert.match(report.contentSha256, /^[a-f0-9]{64}$/);
  assert.throws(() => validateCampaignReport({ ...report,
    summary: { ...report.summary, completedAttempts: 99 } }), /content identity/);
});

test('correction metrics separate successful cost from unresolved spend', () => {
  const corrected = campaignRunMetrics({ outcome: { kind: 'passed' },
    levels: [{ firstBuild: { score: 5, max: 10 }, fixCostUsd: 1.25 }], totals: {} });
  assert.equal(corrected.correctionSuccessRate, 1);
  assert.equal(corrected.correctionCostUsd, 1.25);
  assert.equal(corrected.correctionSpendUsd, 1.25);

  const unresolved = campaignRunMetrics({ outcome: { kind: 'app_failure' },
    levels: [{ firstBuild: { score: 5, max: 10 }, fixCostUsd: 2 }], totals: {} });
  assert.equal(unresolved.correctionSuccessRate, 0);
  assert.equal(unresolved.correctionCostUsd, null);
  assert.equal(unresolved.correctionSpendUsd, 2);

  const unaided = campaignRunMetrics({ outcome: { kind: 'passed' },
    levels: [{ firstBuild: { score: 10, max: 10 }, fixCostUsd: 0 }], totals: {} });
  assert.equal(unaided.correctionSuccessRate, null);
  assert.equal(unaided.correctionCostUsd, null);
  assert.equal(unaided.correctionSpendUsd, null);
});

test('score rates keep inconclusive points separate from measurement coverage', () => {
  const inconclusive = { kind: 'inconclusive', inconclusive: ['contention/203/203b'],
    harnessFailures: [] };
  const selection = { checks: [{ executionId: 'contention', featureId: 203,
    criterionId: '203b', points: 1 }] };
  const metrics = campaignRunMetrics({ outcome: inconclusive, levels: [{
    firstBuild: { score: 8, max: 9, outcome: inconclusive },
    score: 9, max: 9, selection, outcome: inconclusive, fixCostUsd: 1,
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
  evidence.levels[0].selection = {
    specifications: { requested: [], expected: [],
      observed: ['ecommerce.spec.state-durability@1.0.0'] },
    observedChecks: [
      { stableKey: 'durability/session', points: 1 },
      { stableKey: 'durability/cart', points: 2 },
    ],
  };
  evidence.levels[0].firstBuild.source = { sha256: 'a'.repeat(64), files: 3 };
  evidence.levels[0].firstBuild.observations = {
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
  assert.deepEqual(observed.levels[0].specifications,
    ['ecommerce.spec.state-durability@1.0.0']);
});

test('campaign HTML labels observed-only behavior as zero-score first-build observations', () => {
  const plan = compileCampaignFile(example);
  let state = createCampaignState(plan, { now: created });
  const claimed = claimNextAttempt(state, { now: created, admissionId: 'probe-admission' });
  const evidence = run('probe-run', claimed.claim.attempt,
    { score: 10, max: 10, first: 10, cost: 0 });
  evidence.levels[0].selection = { specifications: { requested: [], expected: [],
    observed: ['durability@1'] },
  observedChecks: [{ stableKey: 'durability/session', points: 1 }] };
  evidence.levels[0].firstBuild.observations = { sourceSha256: 'a'.repeat(64),
    selectionSha256: 'b'.repeat(64), selectedChecks: ['durability/session'],
    reportedChecks: ['durability/session'], passedPoints: 1, observedPoints: 1,
    scoreContribution: false, repairVisible: false,
    artifact: 'first-build-l1-observed/bundle.json', outcome: { kind: 'passed' } };
  state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 0, run: evidence }, { now: '2026-08-12T00:02:00.000Z' });
  const report = buildCampaignReport(plan, state, () => evidence);
  const html = renderCampaignHtml(report);
  const condition = report.conditions.find(item => item.stack === claimed.claim.attempt.stack);
  assert.equal(condition.firstBuildObservations.sample.selectedAttempts, 1);
  assert.equal(condition.firstBuildObservations.sample.measuredAttempts, 1);
  assert.equal(condition.firstBuildObservations.metrics.passRate.center, 1);
  assert.match(html, /Additional first-build measurements/);
  assert.match(html, /add no points to the score/);
  assert.match(html, /do not enter repair feedback/);
  assert.match(html, /first-build-l1-observed\/bundle\.json/);
});

test('campaign HTML states the build and evaluation setup in plain language', () => {
  const plan = compileCampaignFile(join(import.meta.dirname, '..', 'appliance',
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
  assert.match(html, /ecommerce\.spec\.access-control@1\.1\.0/);
  assert.match(html, /ecommerce\.spec\.transactional-integrity@1\.2\.0/);
});

test('report generation is byte-for-byte reproducible and links immutable raw evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-report-'));
  try {
    const plan = compileCampaignFile(example);
    const initialized = initializeCampaignDirectory(plan, root, { now: created });
    const admission = runCampaignAdmission(plan, root, {
      env: {}, now: created, uuid: () => 'report',
      preflight: request => ({ schemaVersion: 1, generatedAt: created,
        request: { backends: request.backends, track: request.track, levels: request.levelList,
          runIndex: request.runIndex, agentAdapter: request.agentAdapter,
          packs: request.packIds, checks: request.checkKeys, image: request.image,
          resultsDir: request.resultsDir, smoke: request.smoke },
        ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] }),
    });
    const claimed = claimNextAttempt(initialized.state,
      { now: '2026-08-12T00:01:00.000Z', admissionId: admission.id });
    const output = join(root, claimed.claim.output);
    mkdirSync(output, { recursive: true });
    const evidence = run('run-1', claimed.claim.attempt,
      { score: 58, max: 58, first: 58, cost: 2, durationSec: 30 });
    const timestamp = '2026-08-12T00:01:30.000Z';
    const agent = plan.agents.find(item => item.adapter === claimed.claim.attempt.agentAdapter);
    const stack = plan.stacks.find(item => item.id === claimed.claim.attempt.stack);
    writeRunJson(join(output, 'run.json'), { ...evidence, startedAt: timestamp, completedAt: timestamp,
      track: plan.definition.track, backend: claimed.claim.attempt.stack,
      model: claimed.claim.attempt.model, guidance: claimed.claim.attempt.guidance,
      condition: claimed.claim.attempt.condition,
      skills: claimed.claim.attempt.skills, selectionRequest: plan.definition.selection,
      runtime: { buildImage: null }, identities: emptyArtifactIdentities({
        engine: plan.identities.engine, agentAdapter: agent.identity, stackAdapter: stack,
      }) });
    const state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
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
  const plan = compileCampaignFile(example);
  const state = createCampaignState(plan, { now: created });
  const report = buildCampaignReport({ ...plan, title: '<script>' }, state, () => null);
  const html = renderCampaignHtml(report);
  assert.doesNotMatch(html, /<script>/);
  assert.match(html, /&lt;script&gt;/);
  assert.match(html, /Study condition/);
  assert.match(html, /prescribed@1\.0\.0/);
  const malformedScope = structuredClone(report);
  malformedScope.scope.surprise = true;
  const { contentSha256: _old, ...body } = malformedScope;
  assert.throws(() => validateCampaignReport({ ...body, contentSha256: 'a'.repeat(64) }),
    /scope\.surprise is unknown/);
  const malformedCondition = structuredClone(report);
  malformedCondition.conditions[0].condition = null;
  assert.throws(() => validateCampaignReport(malformedCondition), /conditions\[0\]\.condition is invalid/);
});

test('human reports format normalized usage and elapsed time for people', () => {
  assert.equal(formatDurationMs(4_893_000), '1h 21m 33s');
  assert.equal(formatDurationMs(125_000), '2m 5s');
  assert.equal(formatDurationMs(9_000), '9s');
  const plan = compileCampaignFile(example);
  let state = createCampaignState(plan, { now: created });
  const claimed = claimNextAttempt(state, { now: created, admissionId: 'admission-format' });
  const evidence = run('run-format', claimed.claim.attempt,
    { cost: 19.899, durationSec: 4_893 });
  state = finishCampaignExecution(claimed.state, claimed.claim.executionId,
    { exitCode: 0, run: evidence }, { now: '2026-08-12T01:21:33.000Z' });
  const html = renderCampaignHtml(buildCampaignReport(plan, state, () => evidence));
  assert.match(html, /\$19\.899 normalized usage/);
  assert.match(html, /1h 21m 33s/);
  assert.match(html, /First-build score/);
  assert.match(html, /100% coverage/);
  assert.doesNotMatch(html, />4893s</);

  const { contentSha256: _identity, ...costBody } = buildCampaignReport(plan, state,
    () => evidence);
  costBody.policy.primaryMetric = 'totalCostUsd';
  const costHtml = renderCampaignHtml({ ...costBody,
    contentSha256: sha256(canonicalDefinitionJson(costBody)) });
  assert.match(costHtml, /<th>totalCostUsd<\/th>/);
  assert.match(costHtml, />\$19\.899<br><small>\$19\.899 normalized usage/);
  assert.doesNotMatch(costHtml, /% coverage/);
});
