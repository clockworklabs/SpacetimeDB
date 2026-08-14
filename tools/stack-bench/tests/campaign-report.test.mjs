import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { emptyArtifactIdentities, readArtifact, writeRunJson } from '../artifacts.mjs';
import { compileCampaignFile } from '../campaign-compiler.mjs';
import { buildCampaignReport, generateCampaignReport,
  formatDurationMs, renderCampaignHtml, validateCampaignReport } from '../campaign-report.mjs';
import { runCampaignAdmission } from '../campaign-runner.mjs';
import { claimNextAttempt, createCampaignState, finishCampaignExecution,
  initializeCampaignDirectory, writeCampaignState } from '../campaign-scheduler.mjs';

const example = join(import.meta.dirname, '..', 'appliance', 'campaign.example.json');
const created = '2026-08-12T00:00:00.000Z';

function run(id, attempt, { score = 8, max = 10, first = 5, cost = 2, durationSec = 30 } = {}) {
  return { id, parentAttemptId: attempt.id, outcome: { kind: 'passed' },
    levels: [{ level: 1, firstBuild: { score: first, max }, score, max }],
    totals: { score, max, costUsd: cost, durationSec, fixRounds: score === first ? 0 : 1 } };
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
  assert.equal(report.attempts[0].executions[0].admissionEvidence,
    'admissions/admission-1.json');
  assert.match(report.contentSha256, /^[a-f0-9]{64}$/);
  assert.throws(() => validateCampaignReport({ ...report,
    summary: { ...report.summary, completedAttempts: 99 } }), /content identity/);
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
    const evidence = run('run-1', claimed.claim.attempt);
    const timestamp = '2026-08-12T00:01:30.000Z';
    const agent = plan.agents.find(item => item.adapter === claimed.claim.attempt.agentAdapter);
    const stack = plan.stacks.find(item => item.id === claimed.claim.attempt.stack);
    writeRunJson(join(output, 'run.json'), { ...evidence, startedAt: timestamp, completedAt: timestamp,
      track: plan.definition.track, backend: claimed.claim.attempt.stack,
      model: claimed.claim.attempt.model, guidance: claimed.claim.attempt.guidance,
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
  const malformedScope = structuredClone(report);
  malformedScope.scope.surprise = true;
  const { contentSha256: _old, ...body } = malformedScope;
  assert.throws(() => validateCampaignReport({ ...body, contentSha256: 'a'.repeat(64) }),
    /scope\.surprise is unknown/);
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
  assert.doesNotMatch(html, />4893s</);
});
