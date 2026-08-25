import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { loadTrack } from '../src/composition/tracks.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { artifactPayload, createArtifact, emptyArtifactIdentities, writeArtifact }
  from '../src/evidence/artifacts.mjs';
import { createCheckEvidence } from '../src/evidence/check-evidence.mjs';
import { hashAppSource } from '../src/runtime/source-snapshot.mjs';
import { compileProgressionInput } from '../src/progression/progression-definition.mjs';
import { createLiveProgressionExecution }
  from '../src/progression/live-progression.mjs';
import { validateCampaignRun } from '../src/campaigns/campaign-runner.mjs';

const definition = () => ({
  schemaVersion: 2,
  kind: 'progression-mode',
  id: 'live-fixture',
  version: '1.0.0',
  state: 'draft',
  title: 'Live fixture',
  policy: 'dependency-gated',
  strikes: { default: 2, levels: {} },
  nodes: [{
    id: 'accounts',
    title: 'Accounts',
    questline: 'identity',
    dependencies: [],
    featureRefs: ['ecommerce.feature.accounts@1.1.0'],
    promptModules: [],
    gradingChecks: [{ id: 'ecommerce.feature.accounts.accounts.1a', points: 1 }],
  }],
  questlines: [{ id: 'identity', title: 'Identity' }],
});

const evidence = status => createCheckEvidence({
  status,
  code: status === 'passed' ? 'completed' : 'test_result',
  phase: 'assertion',
  startedAtMs: 1,
  completedAtMs: 2,
});
const setupEvidence = () => createCheckEvidence({
  status: 'passed',
  code: 'completed',
  phase: 'setup',
  startedAtMs: 1,
  completedAtMs: 2,
});

test('live progression binds, records, checkpoints, and persists one exact action', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-live-progression-'));
  try {
    const appDir = join(root, 'app');
    const outputDir = join(root, 'result');
    mkdirSync(join(appDir, 'stack-bench'), { recursive: true });
    mkdirSync(outputDir, { recursive: true });
    writeFileSync(join(appDir, 'index.js'), 'export const ready = true;\n');

    const progression = compileProgressionInput(definition());
    const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
      'ecommerce.l1-modular@2.5.0');
    const owner = {
      schemaVersion: 1,
      campaign: { id: 'campaign', version: '1.0.0', sha256: 'a'.repeat(64) },
      attempt: { id: 'campaign-r1', track: 'ecommerce', stack: 'postgres',
        agentAdapter: 'claude-code', model: 'test-model', conditionSha256: 'b'.repeat(64) },
      workspace: { appDirectory: 'source' },
    };
    const identities = emptyArtifactIdentities({
      agentAdapter: { id: owner.attempt.agentAdapter },
      stackAdapter: { id: owner.attempt.stack },
    });
    const runArtifact = createArtifact({
      kind: 'benchmark_run',
      id: 'run-1',
      attempt: { id: 'run-1', parentId: owner.attempt.id },
      identities,
      payload: {
        backend: owner.attempt.stack,
        model: owner.attempt.model,
        condition: { sha256: owner.attempt.conditionSha256 },
        progression: progression.identity,
        progressionOwner: { schemaVersion: 1, campaign: owner.campaign, attempt: owner.attempt },
      },
    });
    const states = [];
    const execution = createLiveProgressionExecution({
      progression,
      owner,
      statePath: join(outputDir, 'progression-state.json'),
      runId: 'run-1',
      outputDir,
      appDir,
      track: 'ecommerce',
      backend: owner.attempt.stack,
      identities,
      recipeBindings: new Map([[1, binding]]),
      getRunArtifact: () => runArtifact,
      onState: status => states.push(status),
    });
    execution.initialize();
    const selected = execution.bind(1);
    const source = hashAppSource(appDir);
    const key = selected.grader.checkKeys[0];
    const recipe = selected.grader.request.recipe;
    const grade = {
      observation: 'scored',
      source: { sha256: source.sha256 },
      selection: {
        sha256: selected.grader.selectionSha256,
        checks: [{ stableKey: key, points: 1 }],
        attemptedChecks: [key],
        reportedChecks: [key],
        notRun: [],
      },
      totals: { score: 1, max: 1, regression: null },
      suites: { application: { features: [{ setupEvidence: setupEvidence(), criteria: [
        { stableKey: key, points: 1, evidence: evidence('passed') },
      ] }] } },
      outcome: { kind: 'passed' },
    };
    writeArtifact(join(appDir, 'stack-bench', 'bundle.json'), {
      kind: 'grade_bundle',
      id: 'grade-1',
      attempt: { id: 'grade-1', parentId: 'run-1' },
      identities: emptyArtifactIdentities({
        recipe: { id: recipe.id, version: recipe.version, sha256: recipe.contentSha256 },
        stackAdapter: { id: owner.attempt.stack },
      }),
      payload: grade,
    });

    const next = execution.record({ selected, bundle: grade, level: 1,
      repair: { status: 'not-needed', budgetRounds: 1, roundsUsed: 0,
        stopReason: 'not-needed', strikeBudget: 2, strikesUsed: 0 } });
    assert.equal(next.type, 'terminal');
    assert.equal(execution.state.nodes.accounts.status, 'passed');
    assert.equal(states.at(-1).score.averagePercentage, 100);
    assert(existsSync(join(outputDir, 'progression-state.json')));
    assert(existsSync(join(outputDir, 'progression', 'attempt-001', 'bundle.json')));
    assert(existsSync(join(outputDir, 'level-l1-checkpoint.json')));
    assert(existsSync(join(outputDir, 'source', 'index.js')));
    assert.doesNotThrow(() => createArtifact({
      ...runArtifact,
      payload: { ...runArtifact.payload, progressionStatus: states.at(-1) },
    }));
    assert.throws(() => createArtifact({
      ...runArtifact,
      payload: { ...runArtifact.payload,
        progressionStatus: { ...states.at(-1), stateArtifact: '../state.json' } },
    }), /stateArtifact is invalid/);

    const attempt = {
      id: owner.attempt.id,
      mode: { id: 'dependency', version: '1.0.0' },
      levels: [1],
      progression: progression.identity,
      stack: owner.attempt.stack,
      agentAdapter: owner.attempt.agentAdapter,
      model: owner.attempt.model,
      guidance: 'neutral',
      condition: { sha256: owner.attempt.conditionSha256 },
      skills: [],
    };
    const plan = {
      id: owner.campaign.id,
      version: owner.campaign.version,
      contentSha256: owner.campaign.sha256,
      progression,
      definition: { track: owner.attempt.track, selection: { levels: [] },
        runtime: { buildImage: null }, budgets: { maxCostUsdPerAttempt: null } },
      agents: [{ adapter: owner.attempt.agentAdapter, model: owner.attempt.model,
        identity: identities.agentAdapter }],
      stacks: [{ id: owner.attempt.stack, version: identities.stackAdapter.version }],
      conditions: [attempt.condition],
      identities: { engine: identities.engine },
    };
    const completedRun = artifactPayload(createArtifact({
      ...runArtifact,
      payload: {
        ...runArtifact.payload,
        track: owner.attempt.track,
        guidance: attempt.guidance,
        condition: attempt.condition,
        selectionRequest: plan.definition.selection,
        skills: attempt.skills,
        runtime: { buildImage: null },
        progressionStatus: states.at(-1),
        validation: { ladder: { policy: 'dependency-gated', requestedLevels: [1],
          completedLevels: [1], stoppedAfterLevel: null, blockedLevels: [] } },
        levels: [{ level: 1, selection: selected.grader.selection,
          graded: true, score: 1, max: 1 }],
        totals: { costUsd: 0 },
        outcome: { kind: 'passed' },
      },
    }));
    assert.equal(validateCampaignRun(plan, attempt, completedRun, { resultDir: outputDir }),
      completedRun);
    assert.throws(() => validateCampaignRun(plan, attempt, {
      ...completedRun,
      progressionStatus: { ...completedRun.progressionStatus, attempts: 0 },
    }, { resultDir: outputDir }), /progressionStatus/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
