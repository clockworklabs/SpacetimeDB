import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync }
  from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { loadTrack } from '../src/composition/tracks.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { artifactPayload, createArtifact, emptyArtifactIdentities, writeArtifact }
  from '../src/evidence/artifacts.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import type { CheckEvidenceStatus } from '../src/evidence/check-evidence.js';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import { compileDependencyPolicyInput, compileFeatureCatalogInput, compileProgressionInput }
  from '../src/progression/progression-definition.js';
import type { ProgressionInput } from '../src/progression/progression-definition.js';
import { createLiveProgressionExecution, type LiveProgressionExecution,
  type LiveProgressionStatus }
  from '../src/progression/live-progression.js';
import { auditProgressionReferenceRun }
  from '../src/progression/progression-reference-audit.js';
import type { ProgressionWorkAction } from '../src/progression/progression-engine.js';
import type { ProgressionRecipeAction, ProgressionRecipeSelections }
  from '../src/progression/progression-recipe-selection.js';
import type { ProgressionState } from '../src/progression/progression-state.js';
import type { ProgressionNodeState } from '../src/progression/progression-state.js';
import { validateCampaignRun } from '../src/campaigns/campaign-run-validation.js';
import type { RepairPlanInput } from '../src/progression/repair-plan.js';

interface FixtureDependency {
  id: string;
  reason: string;
}

interface FixtureNode {
  id: string;
  title: string;
  questline: string;
  dependencies: FixtureDependency[];
  featureRefs: string[];
  promptModules: string[];
  gradingChecks: Array<{ id: string; points: number; role: 'feature' | 'guarantee' }>;
}

interface FixtureQuestline {
  id: string;
  title: string;
  nodes?: string[];
}

interface FixtureDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  title: string;
  policy: string;
  repair: RepairPlanInput;
  workSelection?: 'feature' | 'progressive' | 'all-at-once';
  nodes: FixtureNode[];
  questlines: FixtureQuestline[];
}

type WorkSelection = { action: ProgressionWorkAction } & ProgressionRecipeSelections;

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function workSelection(selected: ProgressionRecipeAction): WorkSelection {
  assert('grader' in selected, 'expected a progression work action');
  return selected;
}

function executionState(execution: LiveProgressionExecution): ProgressionState {
  assert(execution.state, 'expected initialized progression state');
  return execution.state;
}

function nodeState(state: ProgressionState, id: string): ProgressionNodeState {
  const node = state.nodes[id];
  assert(node, `expected progression node ${id}`);
  return node;
}

function latestStatus(states: LiveProgressionStatus[]): LiveProgressionStatus {
  const status = states.at(-1);
  assert(status, 'expected a reported progression status');
  return status;
}

function workAction(action: ProgressionRecipeAction['action']): ProgressionWorkAction {
  assert.notEqual(action.type, 'terminal');
  return action as ProgressionWorkAction;
}

const definition = (): FixtureDefinition => ({
  schemaVersion: 7,
  kind: 'progression-mode',
  id: 'live-fixture',
  title: 'Live fixture',
  policy: 'dependency-graph',
  repair: { selection: 'feature', budget: { perFeature: 2 } },
  workSelection: 'progressive',
  nodes: [{
    id: 'accounts',
    title: 'Accounts',
    questline: 'identity',
    dependencies: [],
    featureRefs: ['ecommerce.feature.accounts'],
    promptModules: [],
    gradingChecks: [{ id: 'ecommerce.feature.accounts.accounts.1a', points: 1,
      role: 'feature' }],
  }],
  questlines: [{ id: 'identity', title: 'Identity' }],
});
const splitIdentities = (progression: ProgressionInput) => {
  const progressionDefinition = progression.definition as typeof progression.definition & {
    repair?: RepairPlanInput;
    workSelection?: 'feature' | 'progressive' | 'all-at-once';
  };
  const { policy: _policy, repair, unchangedFailureLimit: _limit, workSelection,
    ...catalogDefinition } = progressionDefinition;
  assert(repair);
  const featureCatalog = compileFeatureCatalogInput({ ...catalogDefinition,
    schemaVersion: 1, kind: 'feature-catalog' });
  const dependencyPolicy = compileDependencyPolicyInput(repair, featureCatalog,
    { workSelection });
  return { featureCatalog, dependencyPolicy,
    featureCatalogIdentity: featureCatalog.identity,
    dependencyPolicyIdentity: dependencyPolicy.identity };
};

const evidence = (status: CheckEvidenceStatus) => createCheckEvidence({
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

test('live progression binds and persists one exact accepted action', () => {
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
      'ecommerce.sequential-l1');
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
      stackAdapter: { id: owner.attempt.stack, version: null },
    });
    const runArtifact = createArtifact({
      kind: 'benchmark_run',
      id: 'run-1',
      attempt: { id: 'run-1', parentId: owner.attempt.id },
      identities,
      payload: {
        mode: { id: 'dependency' },
        backend: owner.attempt.stack,
        model: owner.attempt.model,
        condition: { contentSha256: owner.attempt.conditionSha256 },
        featureCatalog: split.featureCatalogIdentity,
        dependencyPolicy: split.dependencyPolicyIdentity,
        progressionOwner: { schemaVersion: 1, campaign: owner.campaign, attempt: owner.attempt },
      },
    });
    const states: LiveProgressionStatus[] = [];
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
    const selected = workSelection(execution.bind());
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
    const gradeArtifact = {
      kind: 'grade_bundle',
      id: 'grade-1',
      attempt: { id: 'grade-1', parentId: 'run-1' },
      identities: emptyArtifactIdentities({
        recipe: { id: recipe.id, sha256: recipe.contentSha256 },
        stackAdapter: { id: owner.attempt.stack },
      }),
      payload: grade,
    };
    writeArtifact(join(appDir, 'stack-bench', 'bundle.json'), gradeArtifact);
    mkdirSync(join(outputDir, 'grading'), { recursive: true });
    writeArtifact(join(outputDir, 'grading', 'bundle.json'), gradeArtifact);

    const next = execution.record({ selected,
      bundle: { ...grade, totals: { score: 0, max: 1, regression: null },
        outcome: { kind: 'app_failure' } }, level: 1 });
    assert(next);
    assert.equal(next.type, 'terminal');
    assert.equal(nodeState(executionState(execution), 'accounts').status, 'passed');
    assert.equal(latestStatus(states).score.questlineAveragePercentage, 100);
    assert(existsSync(join(outputDir, 'progression-state.json')));
    assert(existsSync(join(outputDir, 'progression', 'attempt-001', 'bundle.json')));
    assert(!existsSync(join(outputDir, 'level-l1-checkpoint.json')));
    assert(existsSync(join(outputDir, 'source', 'index.js')));
    assert.doesNotThrow(() => createArtifact({
      ...runArtifact,
      attempt: { id: 'run-1', parentId: owner.attempt.id },
      payload: { ...runArtifact.payload, progressionStatus: states.at(-1) },
    }));
    assert.throws(() => createArtifact({
      ...runArtifact,
      attempt: { id: 'run-1', parentId: owner.attempt.id },
      payload: { ...runArtifact.payload,
        progressionStatus: { ...states.at(-1), stateArtifact: '../state.json' } },
    }), /stateArtifact is invalid/);

    const attempt = {
      id: owner.attempt.id,
      mode: { id: 'dependency' },
      levels: [1],
      featureCatalog: split.featureCatalogIdentity,
      dependencyPolicy: split.dependencyPolicyIdentity,
      stack: owner.attempt.stack,
      agentAdapter: owner.attempt.agentAdapter,
      model: owner.attempt.model,
      guidance: 'neutral',
      condition: { contentSha256: owner.attempt.conditionSha256 },
      skills: [],
    };
    const agentIdentity = identities.agentAdapter;
    const stackIdentity = identities.stackAdapter;
    const engineIdentity = identities.engine;
    assert(agentIdentity);
    assert(stackIdentity);
    assert(engineIdentity);
    const plan = {
      id: owner.campaign.id,
      version: owner.campaign.version,
      state: 'draft',
      contentSha256: owner.campaign.sha256,
      featureCatalog: split.featureCatalog,
      dependencyPolicy: split.dependencyPolicy,
      definition: { track: owner.attempt.track, selection: { levels: [] },
        runtime: { buildImage: null },
        repair: { selection: 'feature' as const, budget: { perFeature: 2 },
          order: 'declared' as const },
        budgets: { maxCostUsdPerAttempt: null } },
      agents: [{ adapter: owner.attempt.agentAdapter, model: owner.attempt.model,
        costLimit: 'non-billable', identity: agentIdentity }],
      stacks: [{ id: owner.attempt.stack, version: null }],
      conditions: [attempt.condition],
      identities: { engine: engineIdentity },
    };
    const completedRun = artifactPayload(createArtifact({
      ...runArtifact,
      attempt: { id: 'run-1', parentId: owner.attempt.id },
      payload: {
        ...runArtifact.payload,
        track: owner.attempt.track,
        guidance: attempt.guidance,
        condition: attempt.condition,
        selectionRequest: plan.definition.selection,
        skills: attempt.skills,
        runtime: { buildImage: null },
        progressionStatus: states.at(-1),
        validation: { ladder: { policy: 'dependency-graph', requestedLevels: [1],
          completedLevels: [1], stoppedAfterLevel: null, blockedLevels: [] } },
        levels: [{ level: 1, selection: selected.grader.selection,
          graded: true, score: 1, max: 1, repairs: 0,
          repair: { status: 'not-needed', limit: 0, used: 0,
            stopReason: 'not-needed', nodeRepairs: [
              { nodeId: 'accounts', used: 0, exhaustionReason: null },
            ] },
          outcome: { kind: 'passed' } }],
        totals: { score: 1, max: 1, costUsd: 0 },
        outcome: { kind: 'passed' },
      },
    }));
    assert.equal(validateCampaignRun(plan, attempt, completedRun, { resultDir: outputDir }),
      completedRun);
    assert.throws(() => validateCampaignRun(plan, attempt, {
      ...completedRun,
      totals: { ...completedRun.totals, score: 2 },
    }, { resultDir: outputDir }), /totals\.score/);
    assert.throws(() => validateCampaignRun(plan, attempt, {
      ...completedRun,
      progressionStatus: { ...completedRun.progressionStatus, attempts: 0 },
    }, { resultDir: outputDir }), /progressionStatus/);
    assert.throws(() => validateCampaignRun(plan, attempt, {
      ...completedRun,
      levels: completedRun.levels?.map(level => ({ ...level,
        repair: { ...level.repair, nodeRepairs: level.repair?.nodeRepairs?.map(node => ({
          ...node, used: node.nodeId === 'accounts' ? 1 : node.used,
        })) } })),
    }, { resultDir: outputDir }), /levels\.L1\.repair\.nodeRepairs/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('live progression records provider interruptions without consuming a repair', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-live-provider-failure-'));
  try {
    const appDir = join(root, 'app');
    const outputDir = join(root, 'result');
    mkdirSync(appDir, { recursive: true });
    mkdirSync(outputDir, { recursive: true });
    const progression = compileProgressionInput(definition());
    const split = splitIdentities(progression);
    const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
      'ecommerce.sequential-l1');
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
        condition: { contentSha256: owner.attempt.conditionSha256 },
        featureCatalog: split.featureCatalogIdentity,
        dependencyPolicy: split.dependencyPolicyIdentity,
        progressionOwner: { schemaVersion: 1, campaign: owner.campaign, attempt: owner.attempt } } });
    const execution = createLiveProgressionExecution({ progression, ...split, owner,
      statePath: join(outputDir, 'progression-state.json'), runId: 'run-1',
      outputDir, appDir, track: owner.attempt.track, backend: owner.attempt.stack,
      identities, recipeBindings: new Map([[1, binding]]),
      getRunArtifact: () => runArtifact });
    execution.initialize();
    const selected = workSelection(execution.bind());
    execution.record({ selected, bundle: null, level: 1,
      failure: { kind: 'provider_failure', reason: 'provider-connection-error' } });
    const state = executionState(execution);
    const attempt = state.attempts[0];
    assert(attempt);
    assert.equal(state.attempts.length, 1);
    assert.equal(attempt.outcome, 'inconclusive');
    assert.equal(attempt.category, 'provider_failure');
    assert.equal(nodeState(state, 'accounts').repairs.used, 0);
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
    const accounts = value.nodes[0];
    assert(accounts);
    value.nodes = [
      accounts,
      { id: 'catalog', title: 'Catalog', questline: 'catalog', dependencies: [],
        featureRefs: ['ecommerce.feature.catalog-items'], promptModules: [],
        gradingChecks: [{ id: 'ecommerce.feature.catalog.catalog-values.2a', points: 1,
          role: 'feature' }] },
      { id: 'purchasing', title: 'Purchasing', questline: 'identity', dependencies: [
        { id: 'accounts', reason: 'Purchasing requires an account.' },
        { id: 'catalog', reason: 'Purchasing requires a catalog item.' },
      ], featureRefs: ['ecommerce.feature.purchasing'], promptModules: [],
      gradingChecks: [{ id: 'ecommerce.feature.purchasing.purchase-order.3c', points: 1,
        role: 'feature' }] },
    ];
    value.questlines = [
      { id: 'identity', title: 'Identity', nodes: ['accounts', 'purchasing'] },
      { id: 'catalog', title: 'Catalog', nodes: ['catalog'] },
    ];
    const progression = compileProgressionInput(value);
    const split = splitIdentities(progression);
    const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
      'ecommerce.sequential-l1');
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
        condition: { contentSha256: owner.attempt.conditionSha256 },
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
    const selected = workSelection(first.bind());
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
      identities: emptyArtifactIdentities({ recipe: { id: recipe.id,
        sha256: recipe.contentSha256 }, stackAdapter: { id: owner.attempt.stack } }),
      payload: grade,
    });
    const next = first.record({ selected, bundle: grade, level: 1 });
    assert(next);
    assert.equal(next.level, 2);
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
    const restoredAction = workAction(restored.action);
    assert(object(restoredAction.prompt));
    assert.deepEqual(restoredAction.prompt.nodeIds, ['purchasing']);
    const resumedState = executionState(resumed);
    assert.equal(resumedState.attempts.length, 1);
    assert.equal(nodeState(resumedState, 'accounts').status, 'passed');
    assert.equal(nodeState(resumedState, 'catalog').status, 'passed');
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

test('an interrupted repair resumes with its failed evidence and regression feedback', () => {
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
    const input = definition();
    input.repair.budget.perFeature = 3;
    const progression = compileProgressionInput(input);
    const split = splitIdentities(progression);
    const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
      'ecommerce.sequential-l1');
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
        condition: { contentSha256: owner.attempt.conditionSha256 },
        featureCatalog: split.featureCatalogIdentity,
        dependencyPolicy: split.dependencyPolicyIdentity,
        progressionOwner: { schemaVersion: 1, campaign: owner.campaign, attempt: owner.attempt } } });
    const first = createLiveProgressionExecution({ progression, ...split, owner,
      statePath: join(firstOutput, 'progression-state.json'), runId: 'run-1',
      outputDir: firstOutput, appDir: firstApp, track: owner.attempt.track,
      backend: owner.attempt.stack, identities, recipeBindings: new Map([[1, binding]]),
      getRunArtifact: () => runArtifact });
    first.initialize();
    const selected = workSelection(first.bind());
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
      identities: emptyArtifactIdentities({ recipe: { id: recipe.id,
        sha256: recipe.contentSha256 }, stackAdapter: { id: owner.attempt.stack } }),
      payload: grade,
    });
    const next = first.record({ selected, bundle: grade, level: 1 });
    assert(next);
    assert.equal(next.type, 'repair');
    const repairSelected = workSelection(first.bind());
    const repairRecipe = repairSelected.grader.request.recipe;
    const repairGrade = { ...grade, selection: {
      ...grade.selection, sha256: repairSelected.grader.selectionSha256,
    } };
    writeArtifact(join(firstApp, 'stack-bench', 'bundle.json'), {
      kind: 'grade_bundle', id: 'grade-regression',
      attempt: { id: 'grade-regression', parentId: 'run-1' },
      identities: emptyArtifactIdentities({ recipe: { id: repairRecipe.id,
        sha256: repairRecipe.contentSha256 },
      stackAdapter: { id: owner.attempt.stack } }),
      payload: repairGrade,
    });
    const afterRegression = first.record({ selected: repairSelected, bundle: repairGrade, level: 1,
      completedRepair: true,
      repairRegression: { ownerNodeIds: ['accounts'],
        report: '# Previous repair regression\n\nThe earlier account behavior stopped working.\n' } });
    assert(afterRegression);
    assert.equal(afterRegression.type, 'repair');
    writeArtifact(join(firstOutput, 'run.json'), { ...runArtifact,
      payload: { ...runArtifact.payload, progressionStatus: first.status(),
        totals: { costUsd: 1.25, costComplete: true }, levels: [] } });
    const audit = auditProgressionReferenceRun({ outputDir: firstOutput, progression,
      ...split, owner, recipeBindings: new Map([[1, binding]]), release: binding.release });
    assert.equal(audit.actions.length, 2);

    const resumed = createLiveProgressionExecution({ progression, ...split, owner,
      statePath: join(secondOutput, 'progression-state.json'), runId: 'run-2',
      outputDir: secondOutput, appDir: secondApp, track: owner.attempt.track,
      backend: owner.attempt.stack, identities, resumeFrom: firstOutput,
      recipeBindings: new Map([[1, binding]]),
      getRunArtifact: () => { throw new Error('grading is not part of this resume check'); } });
    mkdirSync(join(secondApp, 'stack-bench'), { recursive: true });
    writeFileSync(join(secondApp, 'stack-bench', 'stale.json'), '{}\n');
    const restored = resumed.initialize();
    assert.equal(restored.action.type, 'repair');
    assert.equal(restored.action.level, 1);
    assert.equal(executionState(resumed).attempts.length, 2);
    assert.deepEqual(executionState(resumed).attempts.at(-1)?.repairRegression,
      { ownerNodeIds: ['accounts'],
        report: '# Previous repair regression\n\nThe earlier account behavior stopped working.\n' });
    assert.equal(readFileSync(join(secondApp, 'index.js'), 'utf8'),
      'export const broken = true;\n');
    assert(existsSync(join(secondApp, 'stack-bench', 'bundle.json')));
    assert.equal(existsSync(join(secondApp, 'stack-bench', 'stale.json')), false);
    assert(existsSync(join(secondOutput, 'progression', 'attempt-002', 'bundle.json')));
    assert(restored.priorRun);
    const totals = restored.priorRun.payload.totals;
    assert(object(totals));
    assert.equal(totals.costUsd, 1.25);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
