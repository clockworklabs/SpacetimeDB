import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync,
  writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import type { CampaignAttemptPlan, CompiledCampaignPlan }
  from '../src/campaigns/campaign-compiler.js';
import { emptyArtifactIdentities, readArtifact,
  writeArtifact, writeRunJson } from '../src/evidence/artifacts.js';
import { runCampaignAdmission } from '../src/campaigns/campaign-admission.js';
import { campaignChildPath } from '../src/campaigns/campaign-path.js';
import { attemptArgv, campaignRetryAuthority,
  executeCampaign, reconcileCampaign,
  processFailureDetail, remainingAttemptCostBudget } from '../src/campaigns/campaign-runner.js';
import { expectedDependencyRunOutcomeKind, validateCampaignRun }
  from '../src/campaigns/campaign-run-validation.js';
import { hashDirectory } from '../src/evidence/provenance.js';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../src/progression/progression-definition.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import { liveProgressionStatus } from '../src/progression/live-progression.js';
import { writeProgressionState } from '../src/progression/progression-state.js';
import { claimNextAttempt, initializeCampaignDirectory,
  writeCampaignState } from '../src/campaigns/campaign-scheduler.js';

type UnknownRecord = Record<string, unknown>;
interface MutableSelection extends UnknownRecord {
  sha256: string;
  scoredPoints: number;
  specifications?: { requested: string[]; expected: string[]; observed: string[] };
  scoredChecks?: Array<UnknownRecord & { stableKey: string; points: number }>;
  observedChecks?: Array<UnknownRecord & { stableKey: string; points: number }>;
}

test('campaign paths reject escapes and existing symbolic-link segments', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-path-'));
  const outside = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-path-outside-'));
  try {
    assert.equal(campaignChildPath(root, join('attempts', 'one'), 'attempt output'),
      join(root, 'attempts', 'one'));
    assert.throws(() => campaignChildPath(root, '..', 'attempt output'),
      /not a child of the campaign directory/);
    symlinkSync(outside, join(root, 'linked'), 'junction');
    assert.throws(() => campaignChildPath(root, join('linked', 'one'), 'attempt output'),
      /contains a symbolic link/);
  } finally {
    rmSync(root, { recursive: true, force: true });
    rmSync(outside, { recursive: true, force: true });
  }
});
type CampaignExecute = NonNullable<NonNullable<Parameters<typeof executeCampaign>[2]>['execute']>;
type ExecuteCall = {
  command: Parameters<CampaignExecute>[0];
  argv: Parameters<CampaignExecute>[1];
  options: Parameters<CampaignExecute>[2];
};
type AdmissionOptions = NonNullable<Parameters<typeof runCampaignAdmission>[2]>;
type PreflightRequest = Parameters<NonNullable<AdmissionOptions['preflight']>>[0];
interface MutableOutcome extends UnknownRecord {
  kind: string;
  phase?: string;
  reason?: string | null;
  appFailures?: string[];
  inconclusive?: unknown[];
  harnessFailures?: unknown[];
}
interface MutableLevel extends UnknownRecord {
  level: number;
  selection?: MutableSelection;
  score?: number | null;
  max?: number | null;
  fixRounds?: number;
  graded?: boolean;
  error?: string;
  contractPass?: boolean | null;
  regression?: { score: number; max: number } | null;
  firstBuild?: UnknownRecord & { score?: number; max?: number; outcome?: MutableOutcome;
    observations?: UnknownRecord & { sourceSha256: string; selectionSha256: string;
      selectedChecks: string[]; reportedChecks: string[]; passedPoints: number;
      observedPoints: number; scoreContribution: boolean; repairVisible: boolean } };
  buildSession?: UnknownRecord & { costReceipts?: unknown[] };
  repair?: { status: string; budgetRounds: number; roundsUsed: number;
    stopReason: string | null };
  outcome?: MutableOutcome;
}
interface MutableRun extends UnknownRecord {
  artifactEnvelope: { attempt: { parentId: string }; identities: UnknownRecord };
  levels: MutableLevel[];
  outcome: MutableOutcome;
  totals?: { costUsd?: number; costComplete?: boolean };
  validation?: { ladder: UnknownRecord & { blockedLevels: number[] } };
}

const APPLIANCE_ROOT = resolve(STACK_BENCH_ROOT, 'appliance');
const example = join(APPLIANCE_ROOT, 'campaign.example.json');
const productBrief = join(APPLIANCE_ROOT,
  'campaign.product-brief-reference.json');
const dependencyModelFree = resolve(STACK_BENCH_ROOT, 'tests', 'fixtures',
  'dependency-model-free-campaign.json');

// A valid run artifact carries the exact planned selection for each level; the
// modular example selection made this mandatory for level-1 fixtures.
const plannedSelection = (attempt: CampaignAttemptPlan, level: number): MutableSelection =>
  structuredClone(attempt.condition.requested.levels.find(item => item.level === level)!
    .selection) as MutableSelection;
const experimentIdentity = (plan: CompiledCampaignPlan) => ({ id: plan.id, version: plan.version,
  sha256: plan.contentSha256, state: plan.state });

function writeFakePackageEvidence(output: string,
  level: MutableLevel & { selection: MutableSelection }): void {
  const source = join(output, 'source');
  mkdirSync(source, { recursive: true });
  writeFileSync(join(source, 'app.js'), 'export const ready = true;\n');
  const sourceHash = hashDirectory(source).sha256;
  writeArtifact(join(output, 'grading', 'bundle.json'), {
    kind: 'grade_bundle', id: `fake-grade-${level.level}`, payload: {
      observation: 'scored', source: { sha256: sourceHash }, suites: {},
      totals: { score: level.score, max: level.max },
      selection: { sha256: level.selection.sha256 },
    },
  });
}

test('campaign failures prefer the last explicit error over stack and exit noise', () => {
  const stderrTail = [
    'Error: Command failed: node agent.mjs --large-private-request',
    '[reference-agent] starting',
    'Error: reference source contains an unsupported generated link',
    '    at prepareReferenceSource (reference-agent.js:102:3)',
    '[reference-agent] beforeExit code=1',
    '[reference-agent] exit code=1',
  ].join('\n');
  assert.equal(processFailureDetail({ stderrTail }),
    'Error: reference source contains an unsupported generated link');
});

test('dependency completion does not hide a whole-app failure', () => {
  assert.equal(expectedDependencyRunOutcomeKind([
    { outcome: { kind: 'passed' } },
    { outcome: { kind: 'app_failure' } },
    { outcome: { kind: 'passed' } },
  ], { kind: 'passed' }), 'app_failure');
  assert.equal(expectedDependencyRunOutcomeKind([
    { outcome: { kind: 'passed' } },
  ], { kind: 'passed' }), 'passed');
  assert.equal(expectedDependencyRunOutcomeKind([
    { outcome: { kind: 'passed' } },
  ], { kind: 'partial' }), null);
});

test('attempt argv is derived completely from the compiled campaign plan', () => {
  const plan = compileCampaignFile(example);
  const argv = attemptArgv(plan, plan.attempts[0], '/campaign/attempt', 0,
    '/campaign/plan.json');
  assert.deepEqual(argv.slice(1), [
    '--campaign-file', resolve('/campaign/plan.json'),
    '--campaign-attempt-id', plan.attempts[0].id,
    '--run-index', '0', '--out', '/campaign/attempt',
  ]);
  assert.throws(() => attemptArgv(plan, { ...plan.attempts[0], condition: {
    ...plan.attempts[0].condition, guidance: { ...plan.attempts[0].condition.guidance,
      documents: {} },
  } }, '/campaign/attempt', 0, '/campaign/plan.json'), /has no guidance document/);
  assert.throws(() => attemptArgv(plan, { ...plan.attempts[0], pricing: {
    ...plan.attempts[0].pricing,
    rates: { ...plan.attempts[0].pricing.rates, input: 1 },
  } }, '/campaign/attempt', 0, '/campaign/plan.json'), /pricing does not match/);
  assert.throws(() => attemptArgv(plan, plan.attempts[0], '/campaign/attempt'), /requires a run slot/);
  const admitted = attemptArgv(plan, plan.attempts[0], '/campaign/attempt', 0,
    '/campaign/plan.json', null, 'admission-1');
  assert.equal(admitted[admitted.indexOf('--campaign-admission-id') + 1], 'admission-1');
  const capped = structuredClone(plan);
  capped.definition.budgets.maxCostUsdPerAttempt = 10;
  const resumed = attemptArgv(capped, capped.attempts[0], '/campaign/attempt', 0,
    '/campaign/plan.json', null, 'admission-1', 6.5);
  assert.equal(resumed[resumed.indexOf('--max-budget-usd') + 1], '6.5');
  assert.throws(() => attemptArgv(capped, capped.attempts[0], '/campaign/attempt', 0,
    '/campaign/plan.json', null, 'admission-1', 10.5), /invalid remaining cost budget/);
});

test('campaign retry budget subtracts every prior execution cost', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-cost-'));
  try {
    const output = join(root, 'attempts', 'one', 'execution-1');
    mkdirSync(output, { recursive: true });
    writeArtifact(join(output, 'run.json'), { kind: 'benchmark_run', id: 'prior-run',
      payload: { totals: { costUsd: 3.25, costComplete: true } } });
    const campaign = { definition: { budgets: { maxCostUsdPerAttempt: 10 } } };
    const claim = { attempt: { id: 'one' }, priorOutputs: ['attempts/one/execution-1'] };
    assert.equal(remainingAttemptCostBudget(campaign, claim, root), 6.75);

    const artifact = readArtifact<{ totals: { costUsd: number; costComplete: boolean } }>(
      join(output, 'run.json'));
    artifact.payload.totals.costComplete = false;
    writeFileSync(join(output, 'run.json'), `${JSON.stringify(artifact)}\n`);
    assert.throws(() => remainingAttemptCostBudget(campaign, claim, root),
      /prior provider spend is unknown/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('dependency attempts pass separate catalog and policy identities with no level range', () => {
  const dependencyPlan = compileCampaignFile(dependencyModelFree);
  const attempt = dependencyPlan.attempts[0];
  const argv = attemptArgv(dependencyPlan, attempt, '/campaign/dependency', 0,
    '/campaign/plan.json');
  assert.equal(argv.includes('--levels'), false);
  const index = argv.indexOf('--campaign-file');
  assert(index > 0);
  assert.equal(argv[index + 1], resolve('/campaign/plan.json'));
  assert.equal(argv[argv.indexOf('--campaign-attempt-id') + 1], attempt.id);
  for (const option of ['--guidance-document-json', '--condition-json', '--selection-json',
    '--skills-json']) assert.equal(argv.includes(option), false);
  const resumedArgv = attemptArgv(dependencyPlan, attempt, '/campaign/dependency-2', 0,
    '/campaign/plan.json', '/campaign/dependency-1');
  assert.equal(resumedArgv[resumedArgv.indexOf('--progression-resume-from') + 1],
    resolve('/campaign/dependency-1'));
  assert.throws(() => attemptArgv(dependencyPlan, attempt, '/campaign/dependency', 0),
    /requires its compiled campaign plan path/);
  assert.throws(() => attemptArgv(dependencyPlan, { ...attempt,
    mode: { id: 'sequential', version: '1.0.0' },
  }, '/campaign/dependency', 0, '/campaign/plan.json'),
  /mode and dependency policy do not match/);
});

test('campaign validation accepts only an explicit pass-before-next-level application gate', () => {
  const plan = compileCampaignFile(example);
  const attempt = plan.attempts.find(item => item.levels.length > 1) ?? { ...plan.attempts[0], levels: [1, 2] };
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter)!;
  const stack = plan.stacks.find(item => item.id === attempt.stack)!;
  const run: MutableRun = { artifactEnvelope: { attempt: { parentId: attempt.id },
    identities: emptyArtifactIdentities({ engine: plan.identities.engine,
      experiment: experimentIdentity(plan), agentAdapter: agent.identity, stackAdapter: stack }) },
  mode: attempt.mode, track: plan.definition.track, backend: attempt.stack, model: attempt.model,
  pricing: attempt.pricing,
  guidance: attempt.guidance, condition: attempt.condition,
  selectionRequest: plan.definition.selection, skills: attempt.skills,
  runtime: { buildImage: 'test-build-image' }, totals: { costUsd: 0 },
  levels: [{ level: 1, selection: plannedSelection(attempt, 1) }],
  outcome: { kind: 'harness_failure', reason: 'provider-session-error' } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  assert.throws(() => validateCampaignRun(plan, attempt,
    { ...run, mode: { id: 'dependency', version: '3.0.0' } },
    { buildImage: 'test-build-image' }), /does not match.*mode/);
  assert.throws(() => validateCampaignRun(plan, attempt, { ...run,
    artifactEnvelope: { ...run.artifactEnvelope, identities: {
      ...run.artifactEnvelope.identities,
      experiment: { ...(run.artifactEnvelope.identities.experiment as UnknownRecord),
        sha256: 'a'.repeat(64) },
    } } }, { buildImage: 'test-build-image' }), /does not match.*identities\.experiment/);
  assert.throws(() => validateCampaignRun(plan, attempt,
    { ...run, levels: [{ level: 1 }] }, { buildImage: 'test-build-image' }), error => {
    assert(error instanceof Error);
    assert.match(error.message, /levels\.L1\.selection/);
    assert.doesNotMatch(error.message, /canonical JSON data/);
    return true;
  });
  assert.throws(() => validateCampaignRun(plan, attempt,
    { ...run, outcome: { kind: 'app_failure' } }, { buildImage: 'test-build-image' }),
  /does not match.*levels/);
  const appFailure = { kind: 'app_failure', phase: 'grading', reason: null,
    appFailures: ['restock-race/202/202a'], inconclusive: [], harnessFailures: [] };
  const gated: MutableRun = { ...run,
    validation: { ladder: { policy: 'pass-before-next-level', requestedLevels: [1, 2],
      completedLevels: [1], stoppedAfterLevel: 1, blockedLevels: [2] } },
    levels: [{ level: 1, selection: plannedSelection(attempt, 1),
      graded: true, score: 57, max: 58, fixRounds: 3,
      firstBuild: { score: 31, max: 58, outcome: appFailure },
      repair: { status: 'budget-exhausted', budgetRounds: 3, roundsUsed: 3,
        stopReason: 'budget-exhausted' }, outcome: appFailure }],
    outcome: { kind: 'app_failure', levels: { 1: appFailure } } };
  assert.equal(validateCampaignRun(plan, attempt, gated,
    { buildImage: 'test-build-image' }), gated);
  assert.throws(() => validateCampaignRun(plan, attempt, { ...gated,
    validation: { ladder: { ...gated.validation!.ladder, blockedLevels: [] } } },
  { buildImage: 'test-build-image' }), /does not match.*levels/);
});

test('campaign validation accepts a zero-level interrupted run without invented cost totals', () => {
  const compiled = compileCampaignFile(example);
  const plan = { ...compiled, definition: { ...compiled.definition,
    budgets: { ...compiled.definition.budgets, maxCostUsdPerAttempt: 100 } } };
  const attempt = { ...plan.attempts[0], levels: [1, 2] };
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter)!;
  const stack = plan.stacks.find(item => item.id === attempt.stack)!;
  const run: MutableRun = { artifactEnvelope: { attempt: { parentId: attempt.id },
    identities: emptyArtifactIdentities({ engine: plan.identities.engine,
      experiment: experimentIdentity(plan), agentAdapter: agent.identity, stackAdapter: stack }) },
  mode: attempt.mode, track: plan.definition.track, backend: attempt.stack, model: attempt.model,
  pricing: attempt.pricing,
  guidance: attempt.guidance, condition: attempt.condition,
  selectionRequest: plan.definition.selection, skills: attempt.skills,
  runtime: { buildImage: 'test-build-image' }, levels: [],
  outcome: { kind: 'ungraded', reason: 'coding session did not run' } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  assert.throws(() => validateCampaignRun(plan, attempt,
    { ...run, outcome: { kind: 'app_failure' } }, { buildImage: 'test-build-image' }),
  /does not match.*levels.*totals\.costUsd/);
  assert.throws(() => validateCampaignRun(plan, attempt,
    { ...run, backend: 'wrong' }, { buildImage: 'test-build-image' }),
  /does not match.*backend/);
});

test('paid campaign validation requires complete receipt-backed cost evidence', () => {
  const plan = compileCampaignFile(example);
  const attempt = plan.attempts.find(item => item.levels.length > 1)
    ?? { ...plan.attempts[0], levels: [1, 2] };
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter)!;
  agent.costLimit = 'native';
  const stack = plan.stacks.find(item => item.id === attempt.stack)!;
  const receipt = { complete: true, reconciled: true, error: null, costUsd: 1.25 };
  const run: MutableRun = {
    artifactEnvelope: { attempt: { parentId: attempt.id },
      identities: emptyArtifactIdentities({ engine: plan.identities.engine,
        experiment: experimentIdentity(plan), agentAdapter: agent.identity, stackAdapter: stack }) },
    mode: attempt.mode, track: plan.definition.track, backend: attempt.stack, model: attempt.model,
    pricing: attempt.pricing,
    guidance: attempt.guidance, condition: attempt.condition,
    selectionRequest: plan.definition.selection, skills: attempt.skills,
    runtime: { buildImage: 'test-build-image' },
    totals: { costUsd: 1.25, costComplete: true },
    levels: [{ level: 1, selection: plannedSelection(attempt, 1),
      buildSession: { costUsd: 1.25, costComplete: true,
        costReceipts: [{ invocation: 1, receipt }] } }],
    outcome: { kind: 'harness_failure', reason: 'provider-session-error' },
  };

  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  assert.throws(() => validateCampaignRun(plan, attempt, {
    ...run, totals: { ...run.totals, costComplete: false },
  }, { buildImage: 'test-build-image' }), /totals\.costComplete.*costEvidence/);
  assert.throws(() => validateCampaignRun(plan, attempt, {
    ...run, levels: [{ ...run.levels[0]!, buildSession: {
      ...run.levels[0]!.buildSession, costReceipts: [],
    } }],
  }, { buildImage: 'test-build-image' }), /costEvidence/);
  assert.throws(() => validateCampaignRun(plan, attempt, {
    ...run, levels: [{ ...run.levels[0]!, buildSession: {
      ...run.levels[0]!.buildSession, costReceipts: [{ invocation: 1,
        receipt: { ...receipt, reconciled: false, error: 'not reconciled' } }],
    } }],
  }, { buildImage: 'test-build-image' }), /costEvidence/);
});

test('dependency validation keeps a conclusive grade when its repair session is interrupted', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dependency-interrupted-'));
  try {
    const plan = compileCampaignFile(dependencyModelFree);
    const attempt = plan.attempts[0];
    const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter)!;
    const stack = plan.stacks.find(item => item.id === attempt.stack)!;
    const owner = { schemaVersion: 1,
      campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
      attempt: { id: attempt.id, track: plan.definition.track, stack: attempt.stack,
        agentAdapter: attempt.agentAdapter, model: attempt.model,
        conditionSha256: attempt.condition.sha256 },
      workspace: { appDirectory: 'source' } };
    const progression = compileProgressionInput(dependencyRuntimeDefinition(
      plan.featureCatalog!, plan.dependencyPolicy!));
    let state = progressionEngine.initialize(progression.definition);
    const selection = progressionEngine.gradingSelection(state);
    state = progressionEngine.recordResult(state, {
      attemptId: 'grade-before-agent-failure', outcome: 'conclusive',
      nodes: selection.nodeIds.map(id => ({ id,
        checks: selection.checks.filter(check => check.nodeId === id)
          .map(check => ({ id: check.id, outcome: 'fail' })) })),
    });
    writeProgressionState(join(root, 'progression-state.json'), {
      progression, featureCatalogIdentity: plan.featureCatalog!.identity,
      dependencyPolicyIdentity: plan.dependencyPolicy!.identity, owner, state,
    });
    const run: MutableRun = {
      artifactEnvelope: { attempt: { parentId: attempt.id },
        identities: emptyArtifactIdentities({ engine: plan.identities.engine,
          experiment: experimentIdentity(plan), agentAdapter: agent.identity, stackAdapter: stack }) },
      mode: attempt.mode, track: plan.definition.track, backend: attempt.stack, model: attempt.model,
      pricing: attempt.pricing,
      guidance: attempt.guidance, condition: attempt.condition,
      selectionRequest: plan.definition.selection, featureCatalog: attempt.featureCatalog,
      dependencyPolicy: attempt.dependencyPolicy,
      progressionOwner: { schemaVersion: 1, campaign: owner.campaign, attempt: owner.attempt },
      progressionStatus: liveProgressionStatus(state), skills: attempt.skills,
      runtime: { buildImage: 'test-build-image' },
      validation: { ladder: { policy: 'dependency-gated', requestedLevels: attempt.levels,
        completedLevels: [1], stoppedAfterLevel: null, blockedLevels: [1] } },
      levels: [], outcome: { kind: 'harness_failure', phase: 'agent-fix',
        reason: 'repair process exited before completion' },
    };
    assert.equal(validateCampaignRun(plan, attempt, run, {
      buildImage: 'test-build-image', resultDir: root,
    }), run);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('dependency validation requires a matching pre-grade failure attempt', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dependency-pre-grade-failure-'));
  try {
    const plan = compileCampaignFile(dependencyModelFree);
    const attempt = plan.attempts[0];
    const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter)!;
    const stack = plan.stacks.find(item => item.id === attempt.stack)!;
    const owner = { schemaVersion: 1,
      campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
      attempt: { id: attempt.id, track: plan.definition.track, stack: attempt.stack,
        agentAdapter: attempt.agentAdapter, model: attempt.model,
        conditionSha256: attempt.condition.sha256 },
      workspace: { appDirectory: 'source' } };
    const progression = compileProgressionInput(dependencyRuntimeDefinition(
      plan.featureCatalog!, plan.dependencyPolicy!));
    const emptyState = progressionEngine.initialize(progression.definition);
    const state = progressionEngine.recordResult(emptyState, {
      attemptId: 'pre-grade-harness-failure', outcome: 'inconclusive',
      category: 'harness_failure', reason: 'coding-session-failed',
    });
    writeProgressionState(join(root, 'progression-state.json'), {
      progression, featureCatalogIdentity: plan.featureCatalog!.identity,
      dependencyPolicyIdentity: plan.dependencyPolicy!.identity, owner, state,
    });
    const run: MutableRun = {
      artifactEnvelope: { attempt: { parentId: attempt.id },
        identities: emptyArtifactIdentities({ engine: plan.identities.engine,
          experiment: experimentIdentity(plan), agentAdapter: agent.identity,
          stackAdapter: stack }) },
      mode: attempt.mode, track: plan.definition.track, backend: attempt.stack,
      model: attempt.model, pricing: attempt.pricing, guidance: attempt.guidance,
      condition: attempt.condition, selectionRequest: plan.definition.selection,
      featureCatalog: attempt.featureCatalog, dependencyPolicy: attempt.dependencyPolicy,
      progressionOwner: { schemaVersion: 1, campaign: owner.campaign, attempt: owner.attempt },
      progressionStatus: liveProgressionStatus(state), skills: attempt.skills,
      runtime: { buildImage: 'test-build-image' },
      validation: { ladder: { policy: 'dependency-gated', requestedLevels: attempt.levels,
        completedLevels: [], stoppedAfterLevel: null, blockedLevels: [] } },
      levels: [{ level: state.level, graded: false, score: null, max: null,
        error: 'coding-session-failed',
        outcome: { kind: 'harness_failure', phase: 'coding-session',
          reason: 'coding-session-failed' } }],
      outcome: { kind: 'harness_failure', phase: 'coding-session',
        reason: 'coding-session-failed' },
    };
    assert.equal(validateCampaignRun(plan, attempt, run, {
      buildImage: 'test-build-image', resultDir: root,
    }), run);
    writeProgressionState(join(root, 'progression-state.json'), {
      progression, featureCatalogIdentity: plan.featureCatalog!.identity,
      dependencyPolicyIdentity: plan.dependencyPolicy!.identity, owner, state: emptyState,
    });
    const withoutAttempt = { ...run, progressionStatus: liveProgressionStatus(emptyState) };
    assert.throws(() => validateCampaignRun(plan, attempt, withoutAttempt, {
      buildImage: 'test-build-image', resultDir: root,
    }), /levels\.L1\.progressionAttempt/);

    const gradingState = progressionEngine.recordResult(emptyState, {
      attemptId: 'inconclusive-grading', outcome: 'inconclusive',
      category: 'inconclusive_evidence', reason: 'one selected check could not be measured',
    });
    writeProgressionState(join(root, 'progression-state.json'), {
      progression, featureCatalogIdentity: plan.featureCatalog!.identity,
      dependencyPolicyIdentity: plan.dependencyPolicy!.identity, owner, state: gradingState,
    });
    const gradingRun = structuredClone(run);
    gradingRun.progressionStatus = liveProgressionStatus(gradingState);
    gradingRun.levels[0] = { level: gradingState.level, graded: false,
      score: null, max: null, outcome: { kind: 'inconclusive', phase: 'grading',
        reason: 'bundle contains inconclusive evidence' } };
    gradingRun.outcome = { kind: 'inconclusive', phase: 'grading',
      reason: 'bundle contains inconclusive evidence' };
    assert.equal(validateCampaignRun(plan, attempt, gradingRun, {
      buildImage: 'test-build-image', resultDir: root,
    }), gradingRun);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('campaign validation accepts an explicit repeated-findings pause but rejects an unexplained stop', () => {
  const plan = compileCampaignFile(example);
  const attempt = plan.attempts[0];
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter)!;
  const stack = plan.stacks.find(item => item.id === attempt.stack)!;
  const run: MutableRun = { artifactEnvelope: { attempt: { parentId: attempt.id },
    identities: emptyArtifactIdentities({ engine: plan.identities.engine,
      experiment: experimentIdentity(plan), agentAdapter: agent.identity, stackAdapter: stack }) },
  mode: attempt.mode, track: plan.definition.track, backend: attempt.stack, model: attempt.model,
  pricing: attempt.pricing,
  guidance: attempt.guidance, condition: attempt.condition,
  selectionRequest: plan.definition.selection, skills: attempt.skills,
  runtime: { buildImage: 'test-build-image' }, totals: { costUsd: 0 },
  levels: [{ level: 1, selection: plannedSelection(attempt, 1), score: 0, max: 58, fixRounds: 1,
    firstBuild: { score: 0, max: 58, outcome: { kind: 'app_failure' } },
    repair: { status: 'incomplete', budgetRounds: 3, roundsUsed: 1, stopReason: null },
    outcome: { kind: 'app_failure' } }], outcome: { kind: 'app_failure' } };
  assert.throws(() => validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }),
    /levels\.L1\.repair/);
  run.levels[0] = { ...run.levels[0]!,
    repair: { status: 'incomplete', budgetRounds: 3, roundsUsed: 1,
      stopReason: 'repeated-findings' } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  run.levels[0] = { ...run.levels[0]!,
    repair: { status: 'incomplete', budgetRounds: 3, roundsUsed: 1,
      stopReason: 'no-source-change' } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  run.levels[0] = { ...run.levels[0]!, fixRounds: 3,
    repair: { status: 'incomplete', budgetRounds: 3, roundsUsed: 3,
      stopReason: 'no-source-change' } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  run.levels[0] = { ...run.levels[0]!, fixRounds: 3,
    repair: { status: 'budget-exhausted', budgetRounds: 3, roundsUsed: 3, stopReason: null } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  run.levels[0] = { ...run.levels[0]!, score: 58 };
  assert.throws(() => validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }),
    /levels\.L1\.score/);
  run.levels[0] = { ...run.levels[0]!, contractPass: false,
    outcome: { kind: 'app_failure', appFailures: ['contract-lint'] } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run,
    'a behaviorally perfect app may still fail the separately reported contract lint');
  run.levels[0] = { ...run.levels[0]!, regression: { score: 0, max: 1 } };
  assert.throws(() => validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }),
    /levels\.L1\.regression/,
    'L1 has no earlier level and cannot report an inherited regression');
  run.levels[0] = { ...run.levels[0]!, regression: null, contractPass: null,
    outcome: { kind: 'app_failure', appFailures: ['systems/diagnostic'] } };
  assert.throws(() => validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }),
    /levels\.L1\.score/,
    'test-development evidence cannot turn a perfect scored result into an application failure');
  run.levels[0] = { ...run.levels[0]!, score: 0, outcome: { kind: 'passed' },
    repair: { status: 'corrected', budgetRounds: 3, roundsUsed: 3, stopReason: null } };
  assert.throws(() => validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }),
    /outcome\.kind/);
});

test('campaign validation requires complete first-build and final measurement coverage', () => {
  const plan = compileCampaignFile(example);
  const attempt = plan.attempts[0];
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter)!;
  const stack = plan.stacks.find(item => item.id === attempt.stack)!;
  const outcome = { kind: 'passed', inconclusive: [], harnessFailures: [] };
  const run: MutableRun = { artifactEnvelope: { attempt: { parentId: attempt.id },
    identities: emptyArtifactIdentities({ engine: plan.identities.engine,
      experiment: experimentIdentity(plan), agentAdapter: agent.identity, stackAdapter: stack }) },
  mode: attempt.mode, track: plan.definition.track, backend: attempt.stack, model: attempt.model,
  pricing: attempt.pricing,
  guidance: attempt.guidance, condition: attempt.condition,
  selectionRequest: plan.definition.selection, skills: attempt.skills,
  runtime: { buildImage: 'test-build-image' }, totals: { costUsd: 0 },
  levels: [{ level: 1, graded: true, selection: plannedSelection(attempt, 1),
    score: 58, max: 58, fixRounds: 0,
    firstBuild: { score: 58, max: 58, outcome },
    repair: { status: 'not-needed', budgetRounds: 3, roundsUsed: 0, stopReason: null },
    outcome }], outcome: { kind: 'passed' } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  const missingPackage = mkdtempSync(join(tmpdir(), 'stack-bench-missing-package-'));
  try {
    assert.throws(() => validateCampaignRun(plan, attempt, run, {
      buildImage: 'test-build-image', resultDir: missingPackage,
    }), /packageEvidence\.source.*packageEvidence\.grading/);
  } finally { rmSync(missingPackage, { recursive: true, force: true }); }

  const missingFirstBuildPoint = structuredClone(run);
  missingFirstBuildPoint.levels[0]!.firstBuild!.score = 57;
  missingFirstBuildPoint.levels[0]!.firstBuild!.max = 57;
  assert.throws(() => validateCampaignRun(plan, attempt, missingFirstBuildPoint,
    { buildImage: 'test-build-image' }), /firstBuild\.max/);

  const missingFinalPoint = structuredClone(run);
  missingFinalPoint.levels[0]!.max = 57;
  missingFinalPoint.levels[0]!.score = 57;
  assert.throws(() => validateCampaignRun(plan, attempt, missingFinalPoint,
    { buildImage: 'test-build-image' }), /levels\.L1\.max/);

  const inconclusiveFirstBuild = structuredClone(run);
  inconclusiveFirstBuild.levels[0]!.firstBuild!.outcome = { kind: 'app_failure',
    appFailures: ['feature/accounts'],
    inconclusive: ['ecommerce.spec.concurrency-safety.duplicate-checkout.203b'] };
  assert.throws(() => validateCampaignRun(plan, attempt, inconclusiveFirstBuild,
    { buildImage: 'test-build-image' }), /firstBuild\.outcome\.inconclusive/);

  const inconclusiveFinal = structuredClone(run);
  inconclusiveFinal.levels[0]!.outcome = { kind: 'app_failure',
    appFailures: ['feature/accounts'],
    inconclusive: ['ecommerce.spec.concurrency-safety.duplicate-checkout.203b'] };
  inconclusiveFinal.outcome = { kind: 'app_failure' };
  inconclusiveFinal.levels[0]!.score = 57;
  inconclusiveFinal.levels[0]!.fixRounds = 3;
  inconclusiveFinal.levels[0]!.repair = { status: 'budget-exhausted', budgetRounds: 3,
    roundsUsed: 3, stopReason: null };
  assert.throws(() => validateCampaignRun(plan, attempt, inconclusiveFinal,
    { buildImage: 'test-build-image' }), /final\.outcome\.inconclusive/);
});

test('campaign validation binds observed-only evidence to its exact first-build selection', () => {
  const compiled = compileCampaignFile(example);
  const sourceSha256 = 'a'.repeat(64);
  const selectionSha256 = 'b'.repeat(64);
  const scoredChecks = [
    { stableKey: 'feature/accounts', points: 1, treatment: 'requested' as const },
    { stableKey: 'authorization/session', points: 1, treatment: 'expected' as const },
  ];
  const observedChecks = [
    { stableKey: 'durability/session', points: 1, treatment: 'observed' as const },
  ];
  const selectedObservedKeys = observedChecks.map(check => check.stableKey);
  const specifications = { requested: [], expected: ['authorization@1.0.0'],
    observed: ['durability@1.0.0'] };
  const baseAttempt = compiled.attempts[0];
  const baseLevel = baseAttempt.condition.requested.levels[0];
  const requestedLevels: CampaignAttemptPlan['condition']['requested']['levels'] = [{
    ...baseLevel, level: 1, selection: { ...baseLevel.selection, schemaVersion: 3,
      sha256: selectionSha256, scoredPoints: 2, specifications, scoredChecks, observedChecks } }];
  const condition = { ...baseAttempt.condition, requested: {
    ...baseAttempt.condition.requested, levels: requestedLevels } };
  const plan = { ...compiled, conditions: compiled.conditions.map(item =>
    item.sha256 === condition.sha256 ? condition : item) } as CompiledCampaignPlan;
  const attempt: CampaignAttemptPlan = { ...baseAttempt, condition };
  const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter)!;
  const stack = plan.stacks.find(item => item.id === attempt.stack)!;
  const run: MutableRun = { artifactEnvelope: { attempt: { parentId: attempt.id },
    identities: emptyArtifactIdentities({ engine: plan.identities.engine,
      experiment: experimentIdentity(plan), agentAdapter: agent.identity, stackAdapter: stack }) },
  mode: attempt.mode, track: plan.definition.track, backend: attempt.stack, model: attempt.model,
  pricing: attempt.pricing,
  guidance: attempt.guidance, condition: attempt.condition,
  selectionRequest: plan.definition.selection, skills: attempt.skills,
  runtime: { buildImage: 'test-build-image' }, totals: { costUsd: 0 },
  levels: [{ level: 1, score: 0, max: 2, fixRounds: 3,
    selection: { schemaVersion: 3, sha256: selectionSha256, scoredPoints: 2,
      specifications,
      scoredChecks, observedChecks },
    firstBuild: { score: 0, max: 2, outcome: { kind: 'app_failure' },
      source: { sha256: sourceSha256, files: 1 }, observations: {
      sourceSha256, selectionSha256, selectedChecks: selectedObservedKeys,
      reportedChecks: selectedObservedKeys, passedPoints: 1, observedPoints: 1,
      scoreContribution: false, repairVisible: false,
      artifact: 'first-build-l1-observed/bundle.json', outcome: { kind: 'passed' },
    } },
    repair: { status: 'budget-exhausted', budgetRounds: 3, roundsUsed: 3,
      stopReason: null }, outcome: { kind: 'app_failure' } }],
  outcome: { kind: 'app_failure' } };
  assert.equal(validateCampaignRun(plan, attempt, run, { buildImage: 'test-build-image' }), run);
  const wrongExpected = structuredClone(run);
  wrongExpected.levels[0]!.selection!.specifications!.expected = [];
  assert.throws(() => validateCampaignRun(plan, attempt, wrongExpected,
    { buildImage: 'test-build-image' }), /selection\.specifications/);
  const visibleToRepair = structuredClone(run);
  visibleToRepair.levels[0]!.firstBuild!.observations!.repairVisible = true;
  assert.throws(() => validateCampaignRun(plan, attempt, visibleToRepair,
    { buildImage: 'test-build-image' }), /firstBuild\.observations\.repairVisible/);
  const wrongSource = structuredClone(run);
  wrongSource.levels[0]!.firstBuild!.observations!.sourceSha256 = 'c'.repeat(64);
  assert.throws(() => validateCampaignRun(plan, attempt, wrongSource,
    { buildImage: 'test-build-image' }), /firstBuild\.observations\.sourceSha256/);
  const wrongDenominator = structuredClone(run);
  wrongDenominator.levels[0]!.firstBuild!.observations!.observedPoints = 2;
  assert.throws(() => validateCampaignRun(plan, attempt, wrongDenominator,
    { buildImage: 'test-build-image' }), /firstBuild\.observations\.observedPoints/);
  const wrongWeight = structuredClone(run);
  wrongWeight.levels[0]!.selection!.observedChecks![0]!.points = 2;
  assert.throws(() => validateCampaignRun(plan, attempt, wrongWeight,
    { buildImage: 'test-build-image' }), /selection\.observedChecks/);
  const wrongScoreDenominator = structuredClone(run);
  wrongScoreDenominator.levels[0]!.max = 1;
  assert.throws(() => validateCampaignRun(plan, attempt, wrongScoreDenominator,
    { buildImage: 'test-build-image' }), /levels\.L1\.max/);
  const wrongPlannedSelection = structuredClone(run);
  wrongPlannedSelection.levels[0]!.selection!.observedChecks![0]!.stableKey = 'durability/cart';
  wrongPlannedSelection.levels[0]!.firstBuild!.observations!.selectedChecks = ['durability/cart'];
  wrongPlannedSelection.levels[0]!.firstBuild!.observations!.reportedChecks = ['durability/cart'];
  assert.throws(() => validateCampaignRun(plan, attempt, wrongPlannedSelection,
    { buildImage: 'test-build-image' }), /selection\.observedChecks/);
});

test('ordinary campaign execution refuses draft plans', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runner-draft-'));
  try {
    await assert.rejects(() => executeCampaign(example, root, { execute: async () => {
      throw new Error('must not launch');
    } }), /requires a frozen plan/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('campaign trials accept only non-billable draft plans with zero pricing', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runner-trial-policy-'));
  try {
    await assert.rejects(() => executeCampaign(example, root, {
      mode: 'model-free-trial',
      admit: () => ({ id: 'failed-admission', payload: { ok: false } }),
      execute: async () => { throw new Error('must not launch'); },
    }), /admission failed/);

    const paid = JSON.parse(readFileSync(example, 'utf8'));
    paid.agents = [{ adapter: 'claude-code', adapterVersion: '1.17.2', model: 'claude-sonnet-5' }];
    paid.pricing.models = { 'claude-sonnet-5': {
      input: 0, output: 0, cacheWrite5m: 0, cacheWrite1h: 0, cacheRead: 0,
    } };
    const paidPath = join(root, 'paid.json');
    writeFileSync(paidPath, `${JSON.stringify(paid, null, 2)}\n`);
    await assert.rejects(() => executeCampaign(paidPath, join(root, 'paid'), {
      mode: 'model-free-trial',
      execute: async () => { throw new Error('must not launch'); },
    }), /requires non-billable agent adapters/);

    const priced = JSON.parse(readFileSync(example, 'utf8'));
    priced.pricing.models.deterministic.input = 1;
    const pricedPath = join(root, 'priced.json');
    writeFileSync(pricedPath, `${JSON.stringify(priced, null, 2)}\n`);
    await assert.rejects(() => executeCampaign(pricedPath, join(root, 'priced'), {
      mode: 'model-free-trial',
      execute: async () => { throw new Error('must not launch'); },
    }), /requires zero pricing/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('failed campaign admission leaves every attempt pending and unclaimed', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runner-admission-fail-'));
  try {
    await assert.rejects(() => executeCampaign(example, root, { mode: 'model-free-trial',
      admit: () => ({ id: 'failed-admission', payload: { ok: false } }),
      execute: async () => { throw new Error('must not launch'); },
    }), /admission failed/);
    const { readCampaignState } = await import('../src/campaigns/campaign-scheduler.js');
    const state = readCampaignState(root).state;
    assert.equal(state.status, 'prepared');
    assert.equal(state.summary.executions, 0);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('only explicit transient provider failures receive campaign retry authority', () => {
  const clean = { recoveryClean: true };
  assert.deepEqual(campaignRetryAuthority({ outcome: { kind: 'harness_failure',
    phase: 'coding-session', reason: 'coding-session-failed',
    provider: { providerStatus: 503 } } }, clean), {
    transient: false, recoveryClean: true, budgetKnown: true, cause: null,
  });
  assert.equal(campaignRetryAuthority({ outcome: { kind: 'harness_failure',
    phase: 'coding-session', reason: 'coding-session-failed',
    provider: { providerStatus: 503 } } }, { ...clean, requireCostReceipt: true }).budgetKnown, false);
  assert.equal(campaignRetryAuthority({ outcome: { kind: 'harness_failure',
    phase: 'coding-session', reason: 'coding-session-failed', provider: { providerStatus: 503 } },
    totals: { costUsd: 1.25, costComplete: true } },
  { ...clean, requireCostReceipt: true }).budgetKnown, true);
  assert.deepEqual(campaignRetryAuthority({ outcome: { kind: 'provider_failure',
    phase: 'coding-session', reason: 'provider-connection-error',
    provider: { providerStatus: null } } }, clean), {
    transient: true, recoveryClean: true, budgetKnown: true,
    cause: 'provider-connection-error',
  });
  for (const outcome of [
    { kind: 'harness_failure', phase: 'coding-session', reason: 'coding-process-killed',
      provider: { providerStatus: null } },
    { kind: 'harness_failure', phase: 'preflight', reason: 'configuration invalid' },
    { kind: 'harness_failure', phase: 'coding-session', reason: 'provider-throttle-exhausted',
      provider: { providerStatus: 529 } },
  ]) {
    assert.equal(campaignRetryAuthority({ outcome }, clean).transient, false);
  }
});

test('model-free campaign execution checkpoints an authorized retry and every completed attempt', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runner-'));
  const calls: ExecuteCall[] = [];
  const planned = compileCampaignFile(example);
  try {
    const state = await executeCampaign(example, root, { mode: 'model-free-trial',
      admit: () => ({ id: 'admission-1', payload: { ok: true } }),
      execute: async (command, argv, options) => {
        assert(options.env);
        calls.push({ command, argv, options });
        const output = argv[argv.indexOf('--out') + 1]!;
        assert.equal(existsSync(output), true,
          'every execution, including a retry, must have a preflight-mountable output directory');
        assert.deepEqual(options.logs, {
          stdout: join(output, 'process.stdout.log'), stderr: join(output, 'process.stderr.log'),
        });
        const parent = argv[argv.indexOf('--campaign-attempt-id') + 1]!;
        const { emptyArtifactIdentities, writeRunJson } = await import('../src/evidence/artifacts.js');
        const completedAt = new Date().toISOString();
        const attempt = planned.attempts.find(item => item.id === parent)!;
        const agent = planned.agents.find(item => item.adapter === attempt.agentAdapter)!;
        const stack = planned.stacks.find(item => item.id === attempt.stack)!;
        if (calls.length === 1) {
          writeRunJson(join(output, 'run.json'), { id: 'fake-provider-failure',
            parentAttemptId: parent, startedAt: completedAt, completedAt,
            identities: emptyArtifactIdentities({ experiment: experimentIdentity(planned),
              agentAdapter: agent.identity,
              stackAdapter: stack }),
            mode: attempt.mode, track: planned.definition.track,
            backend: attempt.stack, model: attempt.model, pricing: attempt.pricing,
            guidance: attempt.guidance, condition: attempt.condition,
            selectionRequest: planned.definition.selection,
            skills: attempt.skills, runtime: { buildImage: options.env.STACK_BENCH_IMAGE },
            backendLease: { runId: 'fake-provider-failure', backend: attempt.stack,
              state: 'released' },
            levels: [], outcome: { kind: 'provider_failure', phase: 'coding-session',
              reason: 'coding-session-failed', provider: { providerStatus: 503 } } });
          writeArtifact(join(output, 'recovery.json'), { kind: 'recovery',
            id: 'fake-provider-failure-recovery',
            attempt: { id: 'fake-provider-failure-recovery', parentId: 'fake-provider-failure' },
            identities: emptyArtifactIdentities({ stackAdapter: { id: attempt.stack } }),
            payload: { schemaVersion: 1, status: 'clean', runId: 'fake-provider-failure',
              backend: attempt.stack, reason: null,
              cleanup: { succeeded: true, retained: false },
              resources: { backendState: 'released', buildContainer: { running: false },
                listenerProcesses: [], locks: [{ key: 'slot:test', released: true }] },
              instructions: ['No recovery action is required.'] } });
          return { code: 3, timedOut: false };
        }
        const levels = attempt.levels.map(level => {
          const selection = plannedSelection(attempt, level);
          const max = selection.scoredPoints;
          const outcome = { kind: 'passed', inconclusive: [], harnessFailures: [] };
          return { level, graded: true, selection, score: max, max, fixRounds: 0,
            firstBuild: { score: max, max, outcome },
            repair: { status: 'not-needed', budgetRounds: 3, roundsUsed: 0, stopReason: null },
            outcome };
        });
        writeFakePackageEvidence(output, levels.at(-1)!);
        writeRunJson(join(output, 'run.json'), { id: `fake-${calls.length}`,
          parentAttemptId: parent, startedAt: completedAt, completedAt,
          identities: emptyArtifactIdentities({ experiment: experimentIdentity(planned),
            agentAdapter: agent.identity,
            stackAdapter: stack }),
          mode: attempt.mode, track: planned.definition.track,
          backend: attempt.stack, model: attempt.model, pricing: attempt.pricing,
          guidance: attempt.guidance, condition: attempt.condition,
          selectionRequest: planned.definition.selection,
          skills: attempt.skills, runtime: { buildImage: options.env.STACK_BENCH_IMAGE },
          totals: { costUsd: 0 }, levels,
          outcome: { kind: 'passed' } });
        return { code: 0, timedOut: false };
      } });
    assert.equal(state.status, 'completed');
    assert.equal(state.summary.completed, 9);
    assert.equal(state.summary.executions, 10);
    assert.deepEqual(state.attempts[0]!.executions.map(item => item.status), ['invalid', 'completed']);
    assert.equal(calls.every(call => call.options.timeoutMs === 240 * 60_000), true);
    const processArtifact = readArtifact(join(root,
      state.attempts[0]!.executions[0]!.output, 'process.json'),
      { expectedKind: 'campaign_process' });
    assert.equal(processArtifact.payload.executionId, state.attempts[0]!.executions[0]!.id);
    assert.equal(processArtifact.payload.exitCode, 3);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('campaign cancellation stops new claims and reaches the active process tree', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-cancel-'));
  const cancellation = new AbortController();
  let calls = 0;
  try {
    const state = await executeCampaign(example, root, { mode: 'model-free-trial',
      signal: cancellation.signal,
      admit: () => ({ id: 'cancel-admission', payload: { ok: true } }),
      execute: async (_command, _argv, options) => {
        calls += 1;
        assert.equal(options.signal, cancellation.signal);
        cancellation.abort();
        return { code: null, signal: 'SIGTERM', timedOut: false, cancelled: true };
      },
    });
    assert.equal(calls, 1);
    assert.equal(state.summary.executions, 1);
    assert.equal(state.summary.running, 0);
    assert.equal(state.summary.invalid, 1);
    assert.equal(state.attempts[0]!.executions[0]!.outcome, 'scheduler_interrupted');
    assert.match(state.attempts[0]!.executions[0]!.reason!, /cancellation requested/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('one campaign runs multiple attempts of the same stack concurrently in isolated slots', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-parallel-'));
  try {
    const definition = JSON.parse(readFileSync(example, 'utf8'));
    definition.id = 'postgres-parallel-proof';
    definition.repetitions = 1;
    definition.parallelism = 3;
    definition.stacks = [definition.stacks.find((stack: UnknownRecord) => stack.id === 'postgres')];
    definition.stacks[0].repetitions = 3;
    const campaignPath = join(root, 'campaign.json');
    const results = join(root, 'results');
    writeFileSync(campaignPath, `${JSON.stringify(definition, null, 2)}\n`);
    const planned = compileCampaignFile(campaignPath);
    let active = 0;
    let maxActive = 0;
    const started: Array<{ parent: string; runIndex: number }> = [];
    let releaseFirstWave: (() => void) | null = null;
    const firstWave = new Promise<void>(resolve => { releaseFirstWave = resolve; });
    const state = await executeCampaign(campaignPath, results, { mode: 'model-free-trial',
      admit: () => ({ id: 'parallel-admission', payload: { ok: true } }),
      execute: async (_command, argv, options) => {
        assert(options.env);
        const runIndex = Number(argv[argv.indexOf('--run-index') + 1]);
        const parent = argv[argv.indexOf('--campaign-attempt-id') + 1]!;
        const output = argv[argv.indexOf('--out') + 1]!;
        active += 1;
        maxActive = Math.max(maxActive, active);
        started.push({ parent, runIndex });
        if (started.length === 3) releaseFirstWave?.();
        await firstWave;
        const attempt = planned.attempts.find(item => item.id === parent)!;
        const agent = planned.agents.find(item => item.adapter === attempt.agentAdapter)!;
        const stack = planned.stacks.find(item => item.id === attempt.stack)!;
        const completedAt = new Date().toISOString();
        const { writeRunJson } = await import('../src/evidence/artifacts.js');
        const levels = attempt.levels.map(level => {
          const selection = plannedSelection(attempt, level);
          const max = selection.scoredPoints;
          const outcome = { kind: 'passed', inconclusive: [], harnessFailures: [] };
          return { level, graded: true, selection, score: max, max, fixRounds: 0,
            firstBuild: { score: max, max, outcome },
            repair: { status: 'not-needed', budgetRounds: 3, roundsUsed: 0, stopReason: null },
            outcome };
        });
        writeFakePackageEvidence(output, levels.at(-1)!);
        writeRunJson(join(output, 'run.json'), { id: `parallel-${runIndex}`,
          parentAttemptId: parent, startedAt: completedAt, completedAt,
          identities: emptyArtifactIdentities({ experiment: experimentIdentity(planned),
            agentAdapter: agent.identity,
            stackAdapter: stack }),
          mode: attempt.mode, track: planned.definition.track,
          backend: attempt.stack, model: attempt.model, pricing: attempt.pricing,
          guidance: attempt.guidance, condition: attempt.condition,
          selectionRequest: planned.definition.selection, skills: attempt.skills,
          runtime: { buildImage: options.env.STACK_BENCH_IMAGE }, totals: { costUsd: 0 },
          levels, outcome: { kind: 'passed' } });
        active -= 1;
        return { code: 0, timedOut: false };
      } });
    assert.equal(maxActive, 3);
    assert.deepEqual(started.map(item => item.runIndex).sort((a, b) => a - b), [0, 1, 2]);
    assert.equal(new Set(started.map(item => item.parent)).size, 3);
    assert(state.attempts.every(attempt => attempt.status === 'completed'));
    assert(state.attempts.every(attempt => attempt.executions[0]!.runIndex >= 0
      && attempt.executions[0]!.runIndex < 3));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('failed cleanup leaves supervisor authority reconcilable instead of finalizing the attempt', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-cleanup-'));
  try {
    const definition = JSON.parse(readFileSync(example, 'utf8'));
    definition.repetitions = 1;
    definition.parallelism = 1;
    definition.stacks = [definition.stacks[0]];
    definition.agents = [definition.agents[0]];
    definition.conditions = [definition.conditions[0]];
    const campaignPath = join(root, 'campaign.json');
    const results = join(root, 'results');
    writeFileSync(campaignPath, `${JSON.stringify(definition, null, 2)}\n`);
    const state = await executeCampaign(campaignPath, results, { mode: 'model-free-trial',
      admit: (plan, directory) => runCampaignAdmission(plan, directory, {
        env: {}, now: '2026-08-12T00:00:30.000Z', uuid: () => 'cleanup',
        preflight: request => ({ schemaVersion: 1, generatedAt: '2026-08-12T00:00:30.000Z',
          request: { backends: request.backends, track: request.track, levels: request.levelList,
            runIndex: request.runIndex, parallelism: request.parallelism,
            agentAdapter: request.agentAdapter,
            packs: request.packIds, checks: request.checkKeys, image: request.image,
            resultsDir: request.resultsDir, smoke: request.smoke },
          ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] }),
      }),
      execute: async (_command, _argv, options) => {
        assert(options.env);
        mkdirSync(join(results, '.private'), { recursive: true });
        writeFileSync(options.env.STACK_BENCH_SUPERVISOR_STATE!, '{}');
        return { code: 1, timedOut: false };
      },
      rescue: () => { throw new Error('runtime still owns a process'); },
    });
    assert.equal(state.status, 'running');
    assert.equal(state.summary.running, 1);
    assert.equal(state.summary.invalid, 0);

    const rescued: unknown[] = [];
    const reconciled = reconcileCampaign(campaignPath, results, {
      rescue: supervisor => { rescued.push(supervisor); },
    });
    assert.equal(rescued.length, 1);
    assert.equal(reconciled.summary.running, 0);
    assert.equal(reconciled.summary.invalid, 1);
    assert.equal(reconciled.attempts[0]!.executions[0]!.outcome, 'scheduler_interrupted');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('interrupted parallel work advances only after every exact cleanup is proven', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-reconcile-'));
  try {
    const definition = JSON.parse(readFileSync(example, 'utf8'));
    definition.parallelism = 2;
    definition.repetitions = 1;
    const campaignPath = join(root, 'parallel.json');
    writeFileSync(campaignPath, `${JSON.stringify(definition, null, 2)}\n`);
    const plan = compileCampaignFile(campaignPath);
    const initialized = initializeCampaignDirectory(plan, root,
      { now: '2026-08-12T00:00:00.000Z' });
    const admission = runCampaignAdmission(plan, root, {
      env: {}, now: '2026-08-12T00:00:30.000Z', uuid: () => 'reconcile',
      preflight: request => ({ schemaVersion: 1, generatedAt: '2026-08-12T00:00:30.000Z',
        request: { backends: request.backends, track: request.track, levels: request.levelList,
          runIndex: request.runIndex, parallelism: request.parallelism,
          agentAdapter: request.agentAdapter,
          packs: request.packIds, checks: request.checkKeys, image: request.image,
          resultsDir: request.resultsDir, smoke: request.smoke },
        ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] }),
    });
    let running = claimNextAttempt(initialized.state, { now: '2026-08-12T00:01:00.000Z',
      admissionId: admission.id }).state;
    running = claimNextAttempt(running, { now: '2026-08-12T00:01:01.000Z',
      admissionId: admission.id }).state;
    writeCampaignState(initialized.paths.state, plan, running);
    assert.throws(() => reconcileCampaign(campaignPath, root, { rescue: () => {} }),
      /neither private supervisor authority nor public clean recovery proof/);
    const privateDir = join(root, '.private');
    mkdirSync(privateDir);
    const executions = running.attempts.filter(attempt => attempt.status === 'running')
      .map(attempt => attempt.executions.at(-1));
    for (const execution of executions) {
      assert(execution);
      writeFileSync(join(privateDir, `${execution.id}.supervisor.json`), '{}');
    }
    const rescued: unknown[] = [];
    const state = reconcileCampaign(campaignPath, root,
      { rescue: supervisor => { rescued.push(supervisor); } });
    assert.equal(rescued.length, 2);
    assert.equal(state.summary.invalid, 2);
    assert(state.attempts.filter(attempt => attempt.status === 'invalid')
      .every(attempt => attempt.executions[0]!.outcome === 'scheduler_interrupted'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reconciliation accepts the clean public proof left by authenticated recovery', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-recovered-'));
  try {
    const plan = compileCampaignFile(example);
    const initialized = initializeCampaignDirectory(plan, root,
      { now: '2026-08-12T00:00:00.000Z' });
    const admission = runCampaignAdmission(plan, root, {
      env: {}, now: '2026-08-12T00:00:30.000Z', uuid: () => 'recovered',
      preflight: request => ({ schemaVersion: 1, generatedAt: '2026-08-12T00:00:30.000Z',
        request: { backends: request.backends, track: request.track, levels: request.levelList,
          runIndex: request.runIndex, parallelism: request.parallelism,
          agentAdapter: request.agentAdapter,
          packs: request.packIds, checks: request.checkKeys, image: request.image,
          resultsDir: request.resultsDir, smoke: request.smoke },
        ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] }),
    });
    const running = claimNextAttempt(initialized.state, { now: '2026-08-12T00:01:00.000Z',
      admissionId: admission.id }).state;
    writeCampaignState(initialized.paths.state, plan, running);
    const execution = running.attempts[0]!.executions[0]!;
    const output = join(root, execution.output);
    mkdirSync(output, { recursive: true });
    const attempt = running.attempts[0]!;
    writeRunJson(join(output, 'run.json'), { id: 'recovered-run',
      parentAttemptId: attempt.plan.id, backend: attempt.plan.stack,
      backendLease: { runId: 'recovered-run', backend: attempt.plan.stack,
        state: 'released' } });
    writeArtifact(join(output, 'process.json'), { kind: 'campaign_process',
      id: `${execution.id}-process`,
      attempt: { id: execution.id, parentId: attempt.plan.id },
      identities: emptyArtifactIdentities(),
      payload: { schemaVersion: 1, executionId: execution.id, runIndex: execution.runIndex,
        exitCode: 3, signal: null, timedOut: false, streams: null } });
    writeArtifact(join(output, 'recovery.json'), { kind: 'recovery', id: 'recovered-run-recovery',
      attempt: { id: 'recovered-run-recovery', parentId: 'recovered-run' },
      identities: emptyArtifactIdentities({ stackAdapter: { id: attempt.plan.stack } }),
      payload: { schemaVersion: 1, status: 'clean', runId: 'recovered-run',
        backend: attempt.plan.stack, reason: null,
        cleanup: { succeeded: true, retained: false },
        resources: { backendState: 'released', buildContainer: { id: 'container-id',
          name: 'container-name', running: false }, listenerProcesses: [],
        locks: [{ key: 'slot:test', released: true }] },
        instructions: ['No recovery action is required.'] } });
    const state = reconcileCampaign(example, root, {
      rescue: () => { throw new Error('private recovery must not run without private authority'); },
    });
    assert.equal(state.attempts[0]!.status, 'invalid');
    assert.equal(state.attempts[0]!.executions[0]!.outcome, 'scheduler_interrupted');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('campaign admission covers every stack once per distinct agent adapter and writes typed evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-admission-'));
  try {
    const plan = compileCampaignFile(example);
    const calls: PreflightRequest[] = [];
    const admission = runCampaignAdmission(plan, root, {
      env: {}, now: '2026-08-12T00:00:00.000Z', uuid: () => 'test',
      preflight: request => {
        calls.push(request);
        return { schemaVersion: 1, generatedAt: '2026-08-12T00:00:00.000Z',
          request: { backends: request.backends, track: request.track, levels: request.levelList,
            runIndex: request.runIndex, parallelism: request.parallelism,
            agentAdapter: request.agentAdapter,
            packs: request.packIds, checks: request.checkKeys, image: request.image,
            resultsDir: request.resultsDir, smoke: request.smoke },
          ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] };
      },
    });
    assert.equal(admission.payload.ok, true);
    assert.deepEqual(admission.payload.runtime, plan.definition.runtime);
    assert.equal(calls.length, 1);
    assert.deepEqual([...calls[0]!.backends].sort(), ['mongodb', 'postgres', 'spacetime']);
    assert.equal(new Set(calls[0]!.backends).size, 3);
    assert.equal(calls[0]!.smoke, true);
    assert.equal(readArtifact(admission.path,
      { expectedKind: 'campaign_admission' }).payload.campaignSha256, plan.contentSha256);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('campaign admission accepts a modular level selection without legacy pack filters', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-modular-campaign-admission-'));
  try {
    const plan = compileCampaignFile(productBrief);
    const calls: PreflightRequest[] = [];
    const admission = runCampaignAdmission(plan, root, {
      env: {}, now: '2026-08-12T00:00:00.000Z', uuid: () => 'modular',
      preflight: request => {
        calls.push(request);
        return { schemaVersion: 1, generatedAt: '2026-08-12T00:00:00.000Z',
          request: { backends: request.backends, track: request.track, levels: request.levelList,
            runIndex: request.runIndex, parallelism: request.parallelism,
            agentAdapter: request.agentAdapter,
            packs: request.packIds, checks: request.checkKeys, image: request.image,
            resultsDir: request.resultsDir, smoke: request.smoke },
          ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] };
      },
    });
    assert.equal(admission.payload.ok, true);
    assert.equal(calls.length, 1);
    assert.deepEqual(calls[0]!.packIds, []);
    assert.deepEqual(calls[0]!.checkKeys, []);
    const scope = calls[0]!.requestedScopes[0] as { levels: Array<{
      selection: { schemaVersion: number } }> };
    assert.equal(scope.levels[0]!.selection.schemaVersion, 3);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
