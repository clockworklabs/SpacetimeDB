import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

import { compileCampaignFile } from '../src/campaigns/campaign-compiler.mjs';
import { campaignUsesNoExternalResources, runCampaignAdmission }
  from '../src/campaigns/campaign-runner.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { readCampaignState } from '../src/campaigns/campaign-scheduler.mjs';
import { readArtifact } from '../src/evidence/artifacts.mjs';
import { validateProgressionCampaignLevelScope } from '../commands/bench.mjs';
import { replayDependencyMode } from '../src/progression/dependency-mode.mjs';
import { dependencyRuntimeDefinition } from '../src/progression/progression-definition.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CAMPAIGN = join(ROOT, 'tests', 'fixtures', 'dependency-model-free-campaign.json');
const CLI = join(ROOT, 'commands', 'campaign-cli.mjs');

test('dependency campaign scope must match the graph-derived scope', () => {
  const plan = compileCampaignFile(CAMPAIGN);
  const declared = plan.conditions[0].requested.levels[0];
  const binding = resolveRecipeRelease(loadTrack(plan.definition.track), 1,
    `${declared.recipe.id}@${declared.recipe.version}`);
  assert.doesNotThrow(() => validateProgressionCampaignLevelScope(binding,
    plan.featureCatalog, declared, 1));
  const tampered = structuredClone(declared);
  tampered.selection.sha256 = '0'.repeat(64);
  assert.throws(() => validateProgressionCampaignLevelScope(binding,
    plan.featureCatalog, tampered, 1), /graph-derived scope changed/);
});

test('real stack campaigns retain full preflight admission', () => {
  const output = mkdtempSync(join(tmpdir(), 'stack-bench-standard-admission-'));
  try {
    const plan = compileCampaignFile(join(ROOT, 'appliance', 'campaign.example.json'));
    assert.equal(campaignUsesNoExternalResources(plan), false);
    let calls = 0;
    const admission = runCampaignAdmission(plan, output, {
      preflight: request => {
        calls += 1;
        return {
          schemaVersion: 1,
          request: {
            backends: request.backends,
            track: request.track,
            levels: request.levelList,
            runIndex: request.runIndex,
            agentAdapter: request.agentAdapter,
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
      cwd: ROOT,
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
    assert.equal(campaignAttempt.plan.mode.id, 'dependency');
    assert.equal(campaignAttempt.plan.stack, 'stub');
    assert.equal(campaignAttempt.plan.agentAdapter, 'deterministic');
    assert.equal(campaignAttempt.plan.model, 'deterministic-stall');
    assert.equal(campaignAttempt.executions.length, 1);
    assert.equal(campaignAttempt.executions[0].outcome, 'app_failure');

    const executionDirectory = join(output, campaignAttempt.executions[0].output);
    const run = readArtifact(join(executionDirectory, 'run.json'), {
      expectedKind: 'benchmark_run',
    });
    assert.equal(run.attempt.parentId, campaignAttempt.plan.id);
    assert.deepEqual(run.payload.featureCatalog, plan.featureCatalog.identity);
    assert.deepEqual(run.payload.dependencyPolicy, plan.dependencyPolicy.identity);
    assert.equal(run.payload.progressionStatus.phase, 'terminal');
    assert.equal(run.payload.progressionStatus.attempts, 3);
    assert.equal(run.payload.levels.length, 1);
    assert.equal(run.payload.levels[0].fixRounds, 2);
    assert.equal(run.payload.outcome.kind, 'app_failure');

    const progression = readArtifact(join(executionDirectory, 'progression-state.json'), {
      expectedKind: 'progression_state',
    });
    assert.equal(progression.attempt.id, campaignAttempt.plan.id);
    assert.equal(progression.identities.experiment.sha256, plan.contentSha256);
    assert.equal(progression.payload.events.length, 3);
    assert.deepEqual(progression.payload.events.map(event => event.type), [
      'attempt-recorded',
      'attempt-recorded',
      'attempt-recorded',
    ]);
    assert.equal(progression.payload.snapshot.phase, 'terminal');
    assert.equal(progression.payload.snapshot.attempts.length, 3);
    assert.equal(progression.payload.snapshot.nodes.accounts.strikes.used, 3);
    assert.equal(progression.payload.snapshot.nodes.accounts.exhaustionReason,
      'strikes-exhausted');
    const runtimeDefinition = dependencyRuntimeDefinition(
      plan.featureCatalog, plan.dependencyPolicy);
    const replayed = replayDependencyMode(runtimeDefinition,
      progression.payload.events);
    assert.deepEqual(replayed.attempts.map(attempt => attempt.attemptId), [
      `${run.id}-progression-1`,
      `${run.id}-progression-2`,
      `${run.id}-progression-3`,
    ]);
    assert(replayed.attempts.every(attempt => attempt.evidence.kind === 'grade_bundle'));
    assert(replayed.attempts.every(attempt => /^[a-f0-9]{64}$/.test(attempt.evidence.sha256)));
    const duplicate = structuredClone(progression.payload.events);
    duplicate[1].result.attemptId = duplicate[0].result.attemptId;
    assert.throws(() => replayDependencyMode(runtimeDefinition, duplicate),
      /owned progression sequence|duplicate attempt id/);

    for (let sequence = 1; sequence <= 3; sequence += 1) {
      const attempt = String(sequence).padStart(3, '0');
      const bundlePath = join(executionDirectory, 'progression', `attempt-${attempt}`, 'bundle.json');
      assert(existsSync(bundlePath), `missing progression evidence ${bundlePath}`);
      const bundle = readArtifact(bundlePath, { expectedKind: 'grade_bundle' });
      assert.equal(bundle.attempt.parentId, run.id);
      assert.equal(bundle.payload.selection.checks.length, 1);
      assert.equal(bundle.payload.selection.checks[0].stableKey,
        'ecommerce.feature.accounts.accounts.1a');
    }
    assert(existsSync(join(executionDirectory, 'level-l1-checkpoint.json')));
  } finally {
    rmSync(output, { recursive: true, force: true });
  }
});
