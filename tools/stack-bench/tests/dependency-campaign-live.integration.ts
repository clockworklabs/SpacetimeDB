import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { campaignUsesNoExternalResources, runCampaignAdmission }
  from '../src/campaigns/campaign-admission.js';
import { loadTrack } from '../src/composition/tracks.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { readCampaignState } from '../src/campaigns/campaign-scheduler.js';
import { readArtifact } from '../src/evidence/artifacts.js';
import type { BenchmarkRunRecord, GradeBundlePayload }
  from '../src/evidence/benchmark-run.js';
import { validateProgressionCampaignLevelScope }
  from '../src/progression/progression-recipe-selection.js';
import { replayDependencyMode } from '../src/progression/dependency-mode.js';
import { dependencyRuntimeDefinition } from '../src/progression/progression-definition.js';
import type { ProgressionStatePayload } from '../src/progression/progression-state.js';
import { STACK_BENCH_ROOT, compiledEntrypoint } from '../src/package-root.js';

const CAMPAIGN = join(STACK_BENCH_ROOT, 'tests', 'fixtures',
  'dependency-model-free-campaign.json');
const CLI = compiledEntrypoint('commands', 'campaign-cli.js');

test('dependency campaign scope must match the graph-derived scope', () => {
  const plan = compileCampaignFile(CAMPAIGN);
  const declared = plan.conditions[0].requested.levels[0];
  assert(declared);
  assert(plan.featureCatalog);
  const featureCatalog = plan.featureCatalog;
  const binding = resolveRecipeRelease(loadTrack(plan.definition.track), 1,
    `${declared.recipe.id}@${declared.recipe.version}`);
  assert.doesNotThrow(() => validateProgressionCampaignLevelScope(binding,
    featureCatalog, declared, 1));
  const tampered = structuredClone(declared);
  tampered.selection.sha256 = '0'.repeat(64);
  assert.throws(() => validateProgressionCampaignLevelScope(binding,
    featureCatalog, tampered, 1), /graph-derived scope changed/);
});

test('real stack campaigns retain full preflight admission', () => {
  const output = mkdtempSync(join(tmpdir(), 'stack-bench-standard-admission-'));
  try {
    const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'appliance',
      'campaign.example.json'));
    assert.equal(campaignUsesNoExternalResources(plan), false);
    let calls = 0;
    const admission = runCampaignAdmission(plan, output, {
      preflight: request => {
        calls += 1;
        return {
          schemaVersion: 1,
          generatedAt: new Date(0).toISOString(),
          request: {
            backends: request.backends,
            track: request.track,
            levels: request.levelList,
            runIndex: request.runIndex,
            parallelism: request.parallelism,
            agentAdapter: request.agentAdapter,
            guidance: request.guidance,
            packs: request.packIds,
            checks: request.checkKeys,
            recipe: request.recipe ?? null,
            requestedScopeCount: request.requestedScopes?.length ?? 0,
            image: request.image,
            resultsDir: request.resultsDir,
            agentSkills: request.agentSkills ?? null,
            smoke: request.smoke,
          },
          ok: true,
          summary: { passed: 1, failed: 0, warnings: 0 },
          checks: [{ id: 'full-preflight', status: 'pass', summary: 'Full preflight ran' }],
        };
      },
    });
    assert.equal(calls, plan.summary.parallelism);
    assert.equal(admission.payload.ok, true);
    assert(admission.payload.reports.every(report => report.request.guidance === 'prescribed'));
    assert(admission.payload.reports.every(report =>
      JSON.stringify(report.request.agentSkills) === JSON.stringify([
        'typescript-client', 'typescript-server',
      ])));
    assert(admission.payload.reports.every(report => report.request.smoke === true));
    assert(admission.payload.reports.every(report =>
      report.request.backends.some(backend => backend !== 'stub')));
  } finally {
    rmSync(output, { recursive: true, force: true });
  }
});

test('a real model-free campaign persists dependency repairs and evidence', { timeout: 300_000 }, () => {
  const output = mkdtempSync(join(tmpdir(), 'stack-bench-dependency-live-'));
  try {
    const result = spawnSync(process.execPath, [CLI, 'trial', CAMPAIGN, '--out', output], {
      cwd: STACK_BENCH_ROOT,
      encoding: 'utf8',
      timeout: 240_000,
      maxBuffer: 32 * 1024 * 1024,
    });
    assert.equal(result.error, undefined, result.error?.message);
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);

    const { plan, state } = readCampaignState(output);
    assert.equal(state.status, 'completed');
    assert.equal(state.summary.completed, 1);
    assert.equal(state.summary.invalid, 0);
    assert.equal(state.attempts.length, 1);
    const campaignAttempt = state.attempts[0];
    assert(campaignAttempt);
    assert.equal(campaignAttempt.plan.mode.id, 'dependency');
    assert.equal(campaignAttempt.plan.stack, 'stub');
    assert.equal(campaignAttempt.plan.agentAdapter, 'deterministic');
    assert.equal(campaignAttempt.plan.model, 'deterministic-stall');
    assert.equal(campaignAttempt.executions.length, 1);
    const execution = campaignAttempt.executions[0];
    assert(execution);
    assert.equal(execution.outcome, 'app_failure');

    const executionDirectory = join(output, execution.output);
    const run = readArtifact<BenchmarkRunRecord>(join(executionDirectory, 'run.json'), {
      expectedKind: 'benchmark_run',
    });
    assert.equal(run.attempt.parentId, campaignAttempt.plan.id);
    assert(plan.featureCatalog);
    assert(plan.dependencyPolicy);
    assert.deepEqual(run.payload.featureCatalog, plan.featureCatalog.identity);
    assert.deepEqual(run.payload.dependencyPolicy, plan.dependencyPolicy.identity);
    assert(run.payload.progressionStatus);
    assert.equal(run.payload.progressionStatus.phase, 'terminal');
    assert.equal(run.payload.progressionStatus.attempts, 5);
    assert.equal(run.payload.levels.length, 1);
    const level = run.payload.levels[0];
    assert(level);
    assert.equal(level.fixRounds, 4);
    assert(run.payload.outcome);
    assert.equal(run.payload.outcome.kind, 'app_failure');

    const progression = readArtifact<ProgressionStatePayload>(
      join(executionDirectory, 'progression-state.json'), {
      expectedKind: 'progression_state',
      });
    assert.equal(progression.attempt.id, campaignAttempt.plan.id);
    assert(progression.identities.experiment);
    assert.equal(progression.identities.experiment.sha256, plan.contentSha256);
    assert.equal(progression.payload.events.length, 5);
    assert.deepEqual(progression.payload.events.map(event => event.type), [
      'attempt-recorded',
      'attempt-recorded',
      'attempt-recorded',
      'attempt-recorded',
      'attempt-recorded',
    ]);
    const runtimeDefinition = dependencyRuntimeDefinition(
      plan.featureCatalog, plan.dependencyPolicy);
    const replayed = replayDependencyMode(runtimeDefinition,
      progression.payload.events);
    assert.equal(replayed.phase, 'terminal');
    assert.equal(replayed.attempts.length, 5);
    const accounts = replayed.nodes.accounts;
    assert(accounts);
    assert.equal(accounts.strikes.used, 3);
    assert.equal(accounts.exhaustionReason, 'strikes-exhausted');
    const catalog = replayed.nodes.catalog;
    assert(catalog);
    assert.equal(catalog.strikes.used, 3);
    assert.equal(catalog.exhaustionReason, 'strikes-exhausted');
    assert.deepEqual(replayed.attempts.map(attempt => attempt.attemptId),
      Array.from({ length: 5 }, (_, index) => `${run.id}-progression-${index + 1}`));
    assert(replayed.attempts.every(attempt => attempt.evidence?.kind === 'grade_bundle'));
    assert(replayed.attempts.every(attempt => attempt.evidence !== undefined
      && /^[a-f0-9]{64}$/.test(attempt.evidence.sha256)));
    const duplicate = structuredClone(progression.payload.events);
    const firstEvent = duplicate[0];
    const secondEvent = duplicate[1];
    assert(firstEvent?.result);
    assert(secondEvent?.result);
    secondEvent.result.attemptId = firstEvent.result.attemptId;
    assert.throws(() => replayDependencyMode(runtimeDefinition, duplicate),
      /owned progression sequence|duplicate attempt id/);

    const expectedChecks = [
      [
        'ecommerce.feature.accounts.accounts.1a',
        'ecommerce.feature.catalog.catalog.2a',
      ],
      ['ecommerce.feature.accounts.accounts.1a'],
      ['ecommerce.feature.accounts.accounts.1a'],
      ['ecommerce.feature.catalog.catalog.2a'],
      ['ecommerce.feature.catalog.catalog.2a'],
    ];
    for (let sequence = 1; sequence <= 5; sequence += 1) {
      const attempt = String(sequence).padStart(3, '0');
      const bundlePath = join(executionDirectory, 'progression', `attempt-${attempt}`, 'bundle.json');
      assert(existsSync(bundlePath), `missing progression evidence ${bundlePath}`);
      const bundle = readArtifact<GradeBundlePayload>(bundlePath, {
        expectedKind: 'grade_bundle',
      });
      assert.equal(bundle.attempt.parentId, run.id);
      assert(bundle.payload.selection);
      assert(bundle.payload.selection.checks);
      assert.deepEqual(bundle.payload.selection.checks.map(check => check.stableKey),
        expectedChecks[sequence - 1]);
    }
    assert(existsSync(join(executionDirectory, 'level-l1-checkpoint.json')));
  } finally {
    rmSync(output, { recursive: true, force: true });
  }
});
