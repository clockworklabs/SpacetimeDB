import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync }
  from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { loadTrack } from '../src/composition/tracks.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { artifactPayload, createArtifact, emptyArtifactIdentities, writeArtifact }
  from '../src/evidence/artifacts.mjs';
import { createCheckEvidence } from '../src/evidence/check-evidence.mjs';
import { hashAppSource } from '../src/runtime/source-snapshot.mjs';
import { compileDependencyPolicyInput, compileFeatureCatalogInput, compileProgressionInput }
  from '../src/progression/progression-definition.mjs';
import { createLiveProgressionExecution }
  from '../src/progression/live-progression.mjs';
import { validateCampaignRun } from '../src/campaigns/campaign-runner.mjs';

const definition = () => ({
  schemaVersion: 3,
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
const splitIdentities = progression => {
  const { policy: _policy, strikes, unchangedFailureLimit: _limit,
    ...catalogDefinition } = progression.definition;
  const featureCatalog = compileFeatureCatalogInput({ ...catalogDefinition,
    schemaVersion: 1, kind: 'feature-catalog' });
  const dependencyPolicy = compileDependencyPolicyInput({ levels: strikes.levels }, featureCatalog);
  return { featureCatalog, dependencyPolicy,
    featureCatalogIdentity: featureCatalog.identity,
    dependencyPolicyIdentity: dependencyPolicy.identity };
};

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
    const split = splitIdentities(progression);
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
      experiment: { ...owner.campaign, state: 'draft' },
      agentAdapter: { id: owner.attempt.agentAdapter },
      stackAdapter: { id: owner.attempt.stack },
    });
    const runArtifact = createArtifact({
      kind: 'benchmark_run',
      id: 'run-1',
      attempt: { id: 'run-1', parentId: owner.attempt.id },
      identities,
      payload: {
        mode: { id: 'dependency', version: '2.1.0' },
        backend: owner.attempt.stack,
        model: owner.attempt.model,
        condition: { sha256: owner.attempt.conditionSha256 },
        featureCatalog: split.featureCatalogIdentity,
        dependencyPolicy: split.dependencyPolicyIdentity,
        progressionOwner: { schemaVersion: 1, campaign: owner.campaign, attempt: owner.attempt },
      },
    });
    const states = [];
    const execution = createLiveProgressionExecution({
      progression,
      ...split,
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
        stopReason: 'not-needed', strikeScope: 'feature', nodeStrikes: [
          { nodeId: 'accounts', initialBudget: 2, granted: 0, budget: 2, used: 0,
            remaining: 2, exhaustionReason: null },
        ] } });
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
      mode: { id: 'dependency', version: '2.1.0' },
      levels: [1],
      featureCatalog: split.featureCatalogIdentity,
      dependencyPolicy: split.dependencyPolicyIdentity,
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
      state: 'draft',
      contentSha256: owner.campaign.sha256,
      featureCatalog: split.featureCatalog,
      dependencyPolicy: split.dependencyPolicy,
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
          graded: true, score: 1, max: 1, outcome: { kind: 'passed' } }],
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

test('live progression records provider interruptions without consuming a strike', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-live-provider-failure-'));
  try {
    const appDir = join(root, 'app');
    const outputDir = join(root, 'result');
    mkdirSync(appDir, { recursive: true });
    mkdirSync(outputDir, { recursive: true });
    const progression = compileProgressionInput(definition());
    const split = splitIdentities(progression);
    const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
      'ecommerce.l1-modular@2.5.0');
    const owner = { schemaVersion: 1,
      campaign: { id: 'campaign', version: '1.0.0', sha256: 'a'.repeat(64) },
      attempt: { id: 'campaign-r1', track: 'ecommerce', stack: 'postgres',
        agentAdapter: 'claude-code', model: 'test-model', conditionSha256: 'b'.repeat(64) },
      workspace: { appDirectory: 'source' } };
    const identities = emptyArtifactIdentities({
      agentAdapter: { id: owner.attempt.agentAdapter },
      stackAdapter: { id: owner.attempt.stack },
    });
    const runArtifact = createArtifact({ kind: 'benchmark_run', id: 'run-1',
      attempt: { id: 'run-1', parentId: owner.attempt.id }, identities,
      payload: { backend: owner.attempt.stack, model: owner.attempt.model,
        condition: { sha256: owner.attempt.conditionSha256 },
        featureCatalog: split.featureCatalogIdentity,
        dependencyPolicy: split.dependencyPolicyIdentity,
        progressionOwner: { schemaVersion: 1, campaign: owner.campaign, attempt: owner.attempt } } });
    const execution = createLiveProgressionExecution({ progression, ...split, owner,
      statePath: join(outputDir, 'progression-state.json'), runId: 'run-1',
      outputDir, appDir, track: owner.attempt.track, backend: owner.attempt.stack,
      identities, recipeBindings: new Map([[1, binding]]),
      getRunArtifact: () => runArtifact });
    execution.initialize();
    const selected = execution.bind(1);
    execution.record({ selected, bundle: null, level: 1,
      failure: { kind: 'provider_failure', reason: 'provider-connection-error' },
      repair: { status: 'ungraded', budgetRounds: 1, roundsUsed: 0,
        stopReason: 'agent-session-failure' } });
    assert.equal(execution.state.attempts.length, 1);
    assert.equal(execution.state.attempts[0].outcome, 'inconclusive');
    assert.equal(execution.state.attempts[0].category, 'provider_failure');
    assert.equal(execution.state.nodes.accounts.strikes.used, 0);
    assert.equal(existsSync(join(outputDir, 'progression', 'attempt-001')), false);
    assert.equal(existsSync(join(outputDir, 'progression-state.json')), true);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('an interrupted execution restores the saved source and resumes the next graph action', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-live-resume-'));
  try {
    const firstOutput = join(root, 'execution-1');
    const secondOutput = join(root, 'execution-2');
    const firstApp = join(root, 'first-app');
    const secondApp = join(root, 'second-app');
    for (const path of [firstOutput, secondOutput, firstApp, secondApp]) {
      mkdirSync(path, { recursive: true });
    }
    writeFileSync(join(firstApp, 'index.js'), 'export const level = 1;\n');
    writeFileSync(join(secondApp, 'index.js'), 'export const interrupted = true;\n');

    const value = definition();
    value.nodes = [
      value.nodes[0],
      { id: 'catalog', title: 'Catalog', questline: 'catalog', dependencies: [],
        featureRefs: ['ecommerce.feature.catalog@1.1.0'], promptModules: [],
        gradingChecks: [{ id: 'ecommerce.feature.catalog.catalog.2a', points: 1 }] },
      { id: 'purchasing', title: 'Purchasing', questline: 'identity', dependencies: [
        { id: 'accounts', reason: 'Purchasing requires an account.' },
        { id: 'catalog', reason: 'Purchasing requires a catalog item.' },
      ], featureRefs: ['ecommerce.feature.purchasing@1.1.0'], promptModules: [],
      gradingChecks: [{ id: 'ecommerce.feature.purchasing.purchase-order.3c', points: 1 }] },
    ];
    value.questlines = [
      { id: 'identity', title: 'Identity', nodes: ['accounts', 'purchasing'] },
      { id: 'catalog', title: 'Catalog', nodes: ['catalog'] },
    ];
    const progression = compileProgressionInput(value);
    const split = splitIdentities(progression);
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
      kind: 'benchmark_run', id: 'run-1',
      attempt: { id: 'run-1', parentId: owner.attempt.id }, identities,
      payload: { backend: owner.attempt.stack, model: owner.attempt.model,
        condition: { sha256: owner.attempt.conditionSha256 },
        featureCatalog: split.featureCatalogIdentity,
        dependencyPolicy: split.dependencyPolicyIdentity,
        progressionOwner: { schemaVersion: 1, campaign: owner.campaign, attempt: owner.attempt } },
    });
    const first = createLiveProgressionExecution({ progression, ...split, owner,
      statePath: join(firstOutput, 'progression-state.json'), runId: 'run-1',
      outputDir: firstOutput, appDir: firstApp, track: owner.attempt.track,
      backend: owner.attempt.stack, identities,
      recipeBindings: new Map([[1, binding], [2, binding]]),
      getRunArtifact: () => runArtifact });
    assert.equal(first.initialize().resumed, false);
    const selected = first.bind(1);
    const source = hashAppSource(firstApp);
    const checks = selected.grader.checkKeys.map(stableKey => ({ stableKey, points: 1 }));
    const recipe = selected.grader.request.recipe;
    const grade = {
      observation: 'scored', source: { sha256: source.sha256 },
      selection: { sha256: selected.grader.selectionSha256, checks,
        attemptedChecks: checks.map(check => check.stableKey),
        reportedChecks: checks.map(check => check.stableKey), notRun: [] },
      totals: { score: checks.length, max: checks.length, regression: null },
      suites: { application: { features: [{ setupEvidence: setupEvidence(), criteria:
        checks.map(check => ({ ...check, evidence: evidence('passed') })) }] } },
      outcome: { kind: 'passed' },
    };
    mkdirSync(join(firstApp, 'stack-bench'), { recursive: true });
    writeArtifact(join(firstApp, 'stack-bench', 'bundle.json'), {
      kind: 'grade_bundle', id: 'grade-1',
      attempt: { id: 'grade-1', parentId: 'run-1' },
      identities: emptyArtifactIdentities({ recipe: { id: recipe.id, version: recipe.version,
        sha256: recipe.contentSha256 }, stackAdapter: { id: owner.attempt.stack } }),
      payload: grade,
    });
    assert.equal(first.record({ selected, bundle: grade, level: 1,
      repair: { status: 'not-needed', budgetRounds: 1, roundsUsed: 0,
        stopReason: 'not-needed', strikeScope: 'feature', nodeStrikes: [
          { nodeId: 'accounts', initialBudget: 2, granted: 0, budget: 2, used: 0,
            remaining: 2, exhaustionReason: null },
          { nodeId: 'catalog', initialBudget: 2, granted: 0, budget: 2, used: 0,
            remaining: 2, exhaustionReason: null },
        ] } }).level, 2);
    writeArtifact(join(firstOutput, 'run.json'), {
      ...runArtifact,
      payload: { ...runArtifact.payload, progressionStatus: first.status() },
    });
    writeFileSync(join(firstApp, 'index.js'), 'export const interrupted = true;\n');

    const resumed = createLiveProgressionExecution({ progression, ...split, owner,
      statePath: join(secondOutput, 'progression-state.json'), runId: 'run-2',
      outputDir: secondOutput, appDir: secondApp, track: owner.attempt.track,
      backend: owner.attempt.stack, identities, resumeFrom: firstOutput,
      recipeBindings: new Map([[1, binding], [2, binding]]),
      getRunArtifact: () => { throw new Error('grading is not part of this resume check'); } });
    const restored = resumed.initialize();
    assert.equal(restored.resumed, true);
    assert.equal(restored.action.type, 'build');
    assert.equal(restored.action.level, 2);
    assert.deepEqual(restored.action.prompt.nodeIds, ['purchasing']);
    assert.equal(resumed.state.attempts.length, 1);
    assert.equal(resumed.state.nodes.accounts.status, 'passed');
    assert.equal(resumed.state.nodes.catalog.status, 'passed');
    assert.equal(readFileSync(join(secondApp, 'index.js'), 'utf8'),
      'export const level = 1;\n');
    assert(existsSync(join(secondOutput, 'progression-state.json')));

    writeFileSync(join(secondOutput, 'source', 'index.js'), 'tampered\n');
    const rejected = createLiveProgressionExecution({ progression, ...split, owner,
      statePath: join(secondOutput, 'progression-state.json'), runId: 'run-2',
      outputDir: secondOutput, appDir: secondApp, track: owner.attempt.track,
      backend: owner.attempt.stack, identities,
      recipeBindings: new Map([[1, binding], [2, binding]]),
      getRunArtifact: () => runArtifact });
    assert.throws(() => rejected.initialize(), /source does not match its state/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('an interrupted repair resumes as repair with its exact failed evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-live-repair-resume-'));
  try {
    const firstOutput = join(root, 'execution-1');
    const secondOutput = join(root, 'execution-2');
    const firstApp = join(root, 'first-app');
    const secondApp = join(root, 'second-app');
    for (const path of [firstOutput, secondOutput, firstApp, secondApp]) {
      mkdirSync(path, { recursive: true });
    }
    writeFileSync(join(firstApp, 'index.js'), 'export const broken = true;\n');
    const progression = compileProgressionInput(definition());
    const split = splitIdentities(progression);
    const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
      'ecommerce.l1-modular@2.5.0');
    const owner = { schemaVersion: 1,
      campaign: { id: 'campaign', version: '1.0.0', sha256: 'a'.repeat(64) },
      attempt: { id: 'campaign-r1', track: 'ecommerce', stack: 'postgres',
        agentAdapter: 'claude-code', model: 'test-model', conditionSha256: 'b'.repeat(64) },
      workspace: { appDirectory: 'source' } };
    const identities = emptyArtifactIdentities({
      agentAdapter: { id: owner.attempt.agentAdapter },
      stackAdapter: { id: owner.attempt.stack },
    });
    const runArtifact = createArtifact({ kind: 'benchmark_run', id: 'run-1',
      attempt: { id: 'run-1', parentId: owner.attempt.id }, identities,
      payload: { backend: owner.attempt.stack, model: owner.attempt.model,
        condition: { sha256: owner.attempt.conditionSha256 },
        featureCatalog: split.featureCatalogIdentity,
        dependencyPolicy: split.dependencyPolicyIdentity,
        progressionOwner: { schemaVersion: 1, campaign: owner.campaign, attempt: owner.attempt } } });
    const first = createLiveProgressionExecution({ progression, ...split, owner,
      statePath: join(firstOutput, 'progression-state.json'), runId: 'run-1',
      outputDir: firstOutput, appDir: firstApp, track: owner.attempt.track,
      backend: owner.attempt.stack, identities, recipeBindings: new Map([[1, binding]]),
      getRunArtifact: () => runArtifact });
    first.initialize();
    const selected = first.bind(1);
    const source = hashAppSource(firstApp);
    const key = selected.grader.checkKeys[0];
    const recipe = selected.grader.request.recipe;
    const grade = { observation: 'scored', source: { sha256: source.sha256 },
      selection: { sha256: selected.grader.selectionSha256,
        checks: [{ stableKey: key, points: 1 }], attemptedChecks: [key],
        reportedChecks: [key], notRun: [] },
      totals: { score: 0, max: 1, regression: null },
      suites: { application: { features: [{ setupEvidence: setupEvidence(), criteria: [
        { stableKey: key, points: 1, evidence: evidence('failed') },
      ] }] } } };
    mkdirSync(join(firstApp, 'stack-bench'), { recursive: true });
    writeArtifact(join(firstApp, 'stack-bench', 'bundle.json'), {
      kind: 'grade_bundle', id: 'grade-failed',
      attempt: { id: 'grade-failed', parentId: 'run-1' },
      identities: emptyArtifactIdentities({ recipe: { id: recipe.id, version: recipe.version,
        sha256: recipe.contentSha256 }, stackAdapter: { id: owner.attempt.stack } }),
      payload: grade,
    });
    assert.equal(first.record({ selected, bundle: grade, level: 1,
      repair: { status: 'incomplete', budgetRounds: 1, roundsUsed: 0,
        stopReason: null, strikeScope: 'feature', nodeStrikes: [
          { nodeId: 'accounts', initialBudget: 2, granted: 0, budget: 2, used: 1,
            remaining: 1, exhaustionReason: null },
        ] } }).type, 'repair');
    writeArtifact(join(firstOutput, 'run.json'), { ...runArtifact,
      payload: { ...runArtifact.payload, progressionStatus: first.status(),
        totals: { costUsd: 1.25, costComplete: true }, levels: [] } });

    const resumed = createLiveProgressionExecution({ progression, ...split, owner,
      statePath: join(secondOutput, 'progression-state.json'), runId: 'run-2',
      outputDir: secondOutput, appDir: secondApp, track: owner.attempt.track,
      backend: owner.attempt.stack, identities, resumeFrom: firstOutput,
      recipeBindings: new Map([[1, binding]]),
      getRunArtifact: () => { throw new Error('grading is not part of this resume check'); } });
    const restored = resumed.initialize();
    assert.equal(restored.action.type, 'repair');
    assert.equal(restored.action.level, 1);
    assert.equal(resumed.state.attempts.length, 1);
    assert.equal(readFileSync(join(secondApp, 'index.js'), 'utf8'),
      'export const broken = true;\n');
    assert(existsSync(join(secondApp, 'stack-bench', 'bundle.json')));
    assert(existsSync(join(secondOutput, 'progression', 'attempt-001', 'bundle.json')));
    assert.equal(restored.priorRun.payload.totals.costUsd, 1.25);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
