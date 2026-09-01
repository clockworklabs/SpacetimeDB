import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { campaignIdentity, compileCampaignFile, validateCampaignDefinition,
  validateCompiledCampaignPlan } from '../src/campaigns/campaign-compiler.js';
import type { CalibrationResolver, CompilerOptions, RecipeResolver }
  from '../src/campaigns/campaign-compiler.js';
import { runCampaignAdmission } from '../src/campaigns/campaign-admission.js';
import { attemptArgv } from '../src/campaigns/campaign-runner.js';
import { parseBenchArguments } from '../commands/bench-arguments.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.js';
import { resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { writeArtifact } from '../src/evidence/artifacts.js';
import { sha256 } from '../src/evidence/provenance.js';

const APPLIANCE_ROOT = resolve(STACK_BENCH_ROOT, 'appliance');

interface TestCondition {
  id: string;
  version: string;
  guidanceProfile: string;
  repairPolicy: string;
  specifications?: { levels: Array<{
    level: number; requested: string[]; expected: string[]; observed: string[];
  }> };
}

interface TestCampaignDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  track: string;
  mode: { id: string; version: string; strikes?: { default?: number; levels: Record<string, number> } };
  levels?: number[];
  selection: {
    packs?: string[];
    checks?: string[];
    levels?: Array<{ level: number; recipe: string; features?: string[]; checks?: string[] }>;
  };
  stacks: Array<{ id: string; adapterVersion: string; repetitions?: number }>;
  agents: Array<{ adapter: string; adapterVersion: string; model: string }>;
  conditions: TestCondition[];
  repetitions: number;
  ordering: { method: string; seed: string };
  parallelism?: number;
  budgets: { fixRounds: number; attemptTimeoutMinutes: number; maxCostUsdPerAttempt: number | null };
  attemptPolicy: { retries: number; retryOn: string[]; excludeFromAnalysis: string[] };
  runtime: { releaseManifestSha256: string | null; controllerImage: string | null;
    buildImage: string | null; platform: string };
  pricing: { currency: string; unit: string; capturedAt: string; source: string;
    models: Record<string, Record<string, number>> };
  analysis: { primaryMetric: string; secondaryMetrics: string[]; dispersion: string;
    invalidAttempts: string; missingData: string; comparisonUnit: string };
  featureCatalog?: unknown;
}

function definition(overrides: Partial<TestCampaignDefinition> = {}): TestCampaignDefinition {
  return {
    schemaVersion: 5,
    kind: 'campaign-manifest',
    id: 'ecommerce-l1-comparison',
    version: '1.0.0',
    state: 'draft',
    title: 'Ecommerce L1 comparison',
    track: 'ecommerce',
    mode: { id: 'sequential', version: '1.0.0' },
    levels: [1],
    selection: { packs: [], checks: [] },
    stacks: [
      { id: 'spacetime', adapterVersion: '1.1.0' },
      { id: 'postgres', adapterVersion: '1.4.0' },
      { id: 'mongodb', adapterVersion: '1.3.0' },
    ],
    agents: [{ adapter: 'deterministic', adapterVersion: '1.3.0', model: 'deterministic' }],
    conditions: [{ id: 'prescribed', version: '1.0.0',
      guidanceProfile: 'prescribed@1.2.0', repairPolicy: 'scored-only@1.0.0' }],
    repetitions: 3,
    ordering: { method: 'balanced-rotation', seed: 'published-seed-1' },
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: null },
    attemptPolicy: { retries: 1, retryOn: ['harness_failure', 'inconclusive'],
      excludeFromAnalysis: ['contaminated', 'harness_failure', 'inconclusive', 'ungraded'] },
    runtime: { releaseManifestSha256: null, controllerImage: null, buildImage: null,
      platform: 'linux/amd64' },
    pricing: { currency: 'USD', unit: 'USD-per-million-tokens',
      capturedAt: '2026-08-12T00:00:00.000Z',
      source: 'offline deterministic adapter', models: { deterministic: {
        input: 0, output: 0, cacheWrite5m: 0, cacheWrite1h: 0, cacheRead: 0,
      } } },
    analysis: { primaryMetric: 'firstBuildScoreRate',
      secondaryMetrics: ['finalScoreRate', 'totalCostUsd', 'totalDurationMs', 'correctionSuccessRate',
        'correctionCostUsd', 'correctionSpendUsd', 'invalidAttemptRate'],
      dispersion: 'median-iqr', invalidAttempts: 'report-separately', missingData: 'no-imputation',
      comparisonUnit: 'stack-agent-condition-recipe' },
    ...overrides,
  };
}

function compile(value: unknown, options: CompilerOptions = {}) {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-'));
  const path = join(root, 'campaign.json');
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
  try { return compileCampaignFile(path, options); }
  finally { rmSync(root, { recursive: true, force: true }); }
}

const QUALIFIED_BUILD_IMAGE = `sha256:${'d'.repeat(64)}`;

const resolvedQualification: CalibrationResolver = release => {
  return {
    id: `${release.id}-test-calibration`,
    version: release.version,
    state: 'qualified',
    contentSha256: sha256(canonicalDefinitionJson({
      kind: 'resolved-test-calibration',
      recipe: { id: release.id, version: release.version, sha256: release.contentSha256 },
    })),
    qualification: {
      buildImage: QUALIFIED_BUILD_IMAGE,
      stacks: ['mongodb', 'postgres', 'spacetime'].map(id => ({ id, status: 'qualified' })),
    },
    qualificationStaleness: [],
  };
};

const resolvedQualificationOptions: CompilerOptions = { calibrationResolver: resolvedQualification };

// Frozen-state tests isolate compiler policy from the mutable public promotion catalog.
const promotedRecipe: RecipeResolver = (track, level) => {
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.sequential-l1@2.5.0');
  return binding ? {
    ...binding,
    alias: `L${level}`,
    status: 'promoted',
    release: {
      ...binding.release,
      state: 'qualified',
      components: {
        ...binding.release.components,
        fixture: { ...binding.release.components.fixture, state: 'qualified' },
      },
    },
  } : null;
};

const qualifiedCompilerOptions: CompilerOptions = {
  calibrationResolver: resolvedQualification,
  recipeResolver: promotedRecipe,
};

const modularFeatures = [
  'ecommerce.feature.accounts',
  'ecommerce.feature.cart-checkout',
  'ecommerce.feature.catalog',
  'ecommerce.feature.purchasing',
  'ecommerce.feature.reviews',
  'ecommerce.feature.warehouse-admin',
];

function modularDefinition({ requested = [], expected = [], observed = [] }:
  { requested?: string[]; expected?: string[]; observed?: string[] } = {}): TestCampaignDefinition {
  return definition({
    repetitions: 1,
    selection: { levels: [{ level: 1, recipe: 'ecommerce.sequential-l1@2.5.0',
      features: modularFeatures, checks: [] }] },
    conditions: [{ ...definition().conditions[0]!, specifications: { levels: [{ level: 1,
      requested, expected, observed }] } }],
  });
}

function multiLevelDefinition(levels: number[]): TestCampaignDefinition {
  return definition({
    levels,
    selection: { levels: levels.map(level => ({
      level,
      recipe: 'ecommerce.sequential-l1@2.5.0',
      features: modularFeatures,
      checks: [],
    })) },
    conditions: [{ ...definition().conditions[0]!, specifications: {
      levels: levels.map(level => ({ level, requested: [], expected: [], observed: [] })),
    } }],
  });
}

function dependencyDefinition() {
  const value = modularDefinition();
  value.mode = { id: 'dependency', version: '3.0.0',
    strikes: { default: 2, levels: {} } };
  delete value.levels;
  delete value.selection.levels![0]!.features;
  delete value.selection.levels![0]!.checks;
  delete value.conditions[0]!.specifications;
  value.featureCatalog = {
    schemaVersion: 1,
    kind: 'feature-catalog',
    id: 'ecommerce-dependency',
    version: '1.0.0',
    state: 'draft',
    title: 'Ecommerce dependency fixture',
    nodes: [{ id: 'accounts', title: 'Accounts', questline: 'identity', dependencies: [],
      featureRefs: ['ecommerce.feature.accounts@1.1.0'], promptModules: [],
      gradingChecks: [{ id: 'ecommerce.feature.accounts.accounts.1a', points: 1,
        role: 'feature' }] }],
    questlines: [{ id: 'identity', title: 'Identity', nodes: ['accounts'] }],
  };
  return value;
}

test('campaign compilation binds exact inputs and expands a balanced immutable attempt plan', () => {
  const plan = compile(definition(), resolvedQualificationOptions);
  assert.equal(plan.summary.attempts, 9);
  assert.equal(plan.bindings[0]!.recipe.id, 'ecommerce.sequential-l1');
  assert.match(plan.bindings[0]!.recipe.contentSha256, /^[a-f0-9]{64}$/);
  assert.equal(plan.bindings[0]!.calibration!.id, 'ecommerce.sequential-l1-test-calibration');
  assert.equal(plan.bindings[0]!.selection!.completeness, 'full');
  assert.deepEqual(campaignIdentity(plan, resolvedQualificationOptions), {
    id: plan.id, version: '1.0.0', sha256: plan.contentSha256, state: 'draft',
  });
  for (let repetition = 1; repetition <= 3; repetition += 1) {
    const attempts = plan.attempts.filter(attempt => attempt.repetition === repetition);
    assert.deepEqual([...attempts.map(attempt => attempt.order)].sort(), [1, 2, 3]);
    assert.deepEqual([...attempts.map(attempt => attempt.stack)].sort(), ['mongodb', 'postgres', 'spacetime']);
  }
  assert.equal(new Set(plan.attempts.filter(attempt => attempt.order === 1)
    .map(attempt => attempt.stack)).size, 3, 'each stack must lead one repetition');
  assert(plan.attempts.every(attempt => canonicalDefinitionJson(attempt.pricing)
    === canonicalDefinitionJson({ unit: plan.definition.pricing.unit,
      rates: plan.definition.pricing.models[attempt.model] })));
});

test('dependency campaigns bind separate catalog and policy identities in every attempt', () => {
  const plan = compile(dependencyDefinition());
  assert(plan.featureCatalog && plan.dependencyPolicy);
  assert.deepEqual(plan.definition.levels, [1]);
  assert.equal(plan.definition.featureCatalog, undefined);
  assert.equal(plan.featureCatalog.identity.id, 'ecommerce-dependency');
  assert.equal(plan.featureCatalog.identity.version, '1.0.0');
  assert.equal(plan.dependencyPolicy.definition.repairSelection, 'feature');
  assert.equal(plan.dependencyPolicy.definition.version, '3.0.0');
  assert.match(plan.featureCatalog.identity.sha256, /^[a-f0-9]{64}$/);
  assert(plan.attempts.every(attempt =>
    canonicalDefinitionJson(attempt.dependencyPolicy)
      === canonicalDefinitionJson(plan.dependencyPolicy!.identity)));
  assert(plan.attempts.every(attempt =>
    canonicalDefinitionJson(attempt.featureCatalog)
      === canonicalDefinitionJson(plan.featureCatalog!.identity)));
  assert.deepEqual(validateCompiledCampaignPlan(plan), plan);
  assert.throws(() => validateCampaignDefinition({ ...dependencyDefinition(), levels: [1, 2] }),
    /levels.*feature catalog/);
});

test('dependency catalog references use the shared semantic-version parser', () => {
  const value = dependencyDefinition();
  value.levels = [1];
  value.featureCatalog = 'ecommerce.questlines@1.0.0-rc.1+build.7';
  assert.equal(validateCampaignDefinition(value).featureCatalog, value.featureCatalog);
  value.featureCatalog = 'ecommerce:questlines@1.0.0';
  assert.throws(() => validateCampaignDefinition(value), /exact id@version reference/);
});

test('dependency campaign plans bind only the selected feature catalog levels', () => {
  const value = JSON.parse(readFileSync(join(APPLIANCE_ROOT,
    'campaign.ecommerce-progression-reference.json'), 'utf8'));
  value.levels = [1, 2, 3];
  value.selection.levels = value.selection.levels.filter((entry: { level: number }) => entry.level <= 3);
  const plan = compile(value);
  assert(plan.featureCatalog && plan.dependencyPolicy);

  assert.deepEqual([...new Set(plan.featureCatalog.definition.nodes.map(node => node.level))],
    [1, 2, 3]);
  assert(plan.featureCatalog.definition.nodes.every(node => node.level <= 3));
  assert(plan.featureCatalog.definition.questlines.every(questline => questline.nodes.every(nodeId =>
    plan.featureCatalog!.definition.nodes.some(node => node.id === nodeId))));
  assert.equal(plan.featureCatalog.definition.state, 'draft');
  assert.deepEqual(plan.dependencyPolicy.definition.levels, [1, 2, 3]);
  assert.equal(plan.featureCatalog.identity.sha256,
    sha256(canonicalDefinitionJson(plan.featureCatalog.definition)));
  assert(plan.attempts.every(attempt => canonicalDefinitionJson(attempt.featureCatalog)
    === canonicalDefinitionJson(plan.featureCatalog!.identity)));
  assert.deepEqual(validateCompiledCampaignPlan(plan), plan);
});

test('campaign graph version must match its recipe calibration', () => {
  const value = JSON.parse(readFileSync(join(APPLIANCE_ROOT,
    'campaign.ecommerce-progression-reference.json'), 'utf8'));
  const calibrationResolver: CalibrationResolver = (release, options) => {
    const calibration = resolvedQualification(release, options);
    assert(calibration);
    calibration.qualification.featureCatalog = {
      id: 'ecommerce.questlines',
      version: '1.1.0',
      sha256: 'a'.repeat(64),
    };
    return calibration;
  };
  assert.throws(() => compile(value, { calibrationResolver }),
    /L1 calibration qualifies ecommerce\.questlines@1\.1\.0, not ecommerce\.questlines@2\.0\.1/);
});

test('sequential campaigns can use the same feature catalog without dependency gating', () => {
  const dependencyPlan = compile(dependencyDefinition());
  assert(dependencyPlan.featureCatalog);
  const definition = dependencyDefinition();
  definition.mode = { id: 'sequential', version: '1.0.0' };
  const plan = compile(definition);
  assert(plan.featureCatalog);
  assert.equal(plan.attempts[0]!.dependencyPolicy, undefined);
  assert.deepEqual(plan.attempts[0]!.featureCatalog, plan.featureCatalog.identity);
  assert.equal(plan.featureCatalog.identity.sha256,
    dependencyPlan.featureCatalog.identity.sha256);
  assert.deepEqual(validateCompiledCampaignPlan(plan), plan);
});

test('strike changes affect dependency policy but not the shared feature catalog', () => {
  const first = compile(dependencyDefinition());
  const changed = dependencyDefinition();
  changed.mode.strikes!.default = 5;
  const second = compile(changed);
  assert(first.featureCatalog && first.dependencyPolicy);
  assert(second.featureCatalog && second.dependencyPolicy);
  assert.equal(first.featureCatalog.identity.sha256, second.featureCatalog.identity.sha256);
  assert.notEqual(first.dependencyPolicy.identity.sha256,
    second.dependencyPolicy.identity.sha256);
  const changedIdentity = structuredClone(first);
  changedIdentity.dependencyPolicy!.identity.sha256 = 'a'.repeat(64);
  assert.throws(() => validateCompiledCampaignPlan(changedIdentity),
    /dependency policy identity/);
  assert.throws(() => validateCompiledCampaignPlan({ ...first, dependencyPolicy: null }),
    /dependency policy does not match its mode/);
});

test('dependency bench input is bound to one fully validated campaign attempt', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dependency-plan-'));
  try {
    const plan = compile(dependencyDefinition());
    assert(plan.featureCatalog && plan.dependencyPolicy);
    const planPath = join(root, 'plan.json');
    writeArtifact(planPath, { kind: 'campaign_plan', id: `${plan.id}-plan`, payload: plan });
    const attempt = plan.attempts[0];
    assert(attempt);
    const argv = attemptArgv(plan, attempt, join(root, 'result'), 0, planPath);
    const args = parseBenchArguments(['node', ...argv]);
    assert.deepEqual(args.pricing, attempt.pricing);
    assert.deepEqual(args.runMode, attempt.mode);
    assert.deepEqual(args.experimentIdentity, campaignIdentity(plan));
    assert.deepEqual(args.featureCatalog, plan.featureCatalog);
    assert.deepEqual(args.dependencyPolicy, plan.dependencyPolicy);
    assert(args.progression);
    assert.equal(args.progression.definition.policy, 'dependency-gated');
    assert.deepEqual(args.progressionOwner, { schemaVersion: 1,
      campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
      attempt: { id: attempt.id, track: plan.definition.track, stack: attempt.stack,
        agentAdapter: attempt.agentAdapter,
        model: attempt.model, conditionSha256: attempt.condition.sha256 } });
    assert.throws(() => parseBenchArguments(['node', ...argv, '--backend', 'postgres']),
      /campaign attempts cannot override --backend/);

    const now = new Date().toISOString();
    const admission = runCampaignAdmission(plan, root, {
      now, uuid: () => 'bench-input', env: {},
      preflight: request => ({
        schemaVersion: 1,
        generatedAt: now,
        request: { backends: request.backends, track: request.track, levels: request.levelList,
          runIndex: request.runIndex, parallelism: request.parallelism,
          agentAdapter: request.agentAdapter,
          packs: request.packIds, checks: request.checkKeys, image: request.image,
          resultsDir: request.resultsDir, smoke: request.smoke },
        ok: true,
        summary: { passed: 1, failed: 0, warnings: 0 },
        checks: [{ id: 'smoke.container', status: 'pass', summary: 'passed' }],
      }),
    });
    const admittedArgv = attemptArgv(plan, attempt, join(root, 'admitted-result'), 0,
      planPath, null, admission.id);
    const admittedArgs = parseBenchArguments(['node', ...admittedArgv]);
    assert(admittedArgs.campaignAdmission);
    assert.equal(admittedArgs.campaignAdmission.id, admission.id);
    assert.equal(admittedArgs.campaignAdmission.reusable, true);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('sequential bench input uses the catalog for selection without live gating', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-sequential-catalog-'));
  try {
    const definition = dependencyDefinition();
    definition.mode = { id: 'sequential', version: '1.0.0' };
    const plan = compile(definition);
    assert(plan.featureCatalog);
    const planPath = join(root, 'plan.json');
    writeArtifact(planPath, { kind: 'campaign_plan', id: `${plan.id}-plan`, payload: plan });
    const argv = attemptArgv(plan, plan.attempts[0]!, join(root, 'result'), 0, planPath);
    assert.equal(argv.includes('--levels'), false);
    const args = parseBenchArguments(['node', ...argv]);
    assert.deepEqual(args.runMode, plan.attempts[0]!.mode);
    assert.deepEqual(args.experimentIdentity, campaignIdentity(plan));
    assert.equal(args.progression, undefined);
    assert.deepEqual(args.featureCatalog, plan.featureCatalog);
    assert.deepEqual(args.levelList, [1]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('sequential campaign input retains its campaign and mode without a feature catalog', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-sequential-plan-'));
  try {
    const plan = compileCampaignFile(join(APPLIANCE_ROOT,
      'campaign.example.json'));
    const planPath = join(root, 'plan.json');
    writeArtifact(planPath, { kind: 'campaign_plan', id: `${plan.id}-plan`, payload: plan });
    const attempt = plan.attempts[0];
    assert(attempt);
    const argv = attemptArgv(plan, attempt, join(root, 'result'), 0, planPath);
    assert.equal(argv.includes('--feature-catalog-sha256'), false);
    const args = parseBenchArguments(['node', ...argv]);
    assert.deepEqual(args.runMode, attempt.mode);
    assert.deepEqual(args.experimentIdentity, campaignIdentity(plan));
    assert.equal(args.featureCatalog, undefined);
    assert.equal(args.progression, undefined);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('dependency campaigns derive product scope and reject duplicate author scope', () => {
  const value = dependencyDefinition();
  const plan = compile(value);
  assert.deepEqual(plan.conditions[0].requested.levels[0].selection.requested.features,
    ['ecommerce.feature.accounts']);
  assert.deepEqual(plan.conditions[0].requested.levels[0].selection.requested.checks,
    ['ecommerce.feature.accounts.accounts.1a']);
  value.selection.levels![0]!.features = ['ecommerce.feature.accounts'];
  assert.throws(() => compile(value), /features: is unknown/);
  const specifications = dependencyDefinition();
  specifications.conditions[0]!.specifications = { levels: [{ level: 1,
    requested: [], expected: [], observed: [] }] };
  assert.throws(() => compile(specifications), /progression graph owns specification scope/);
  const legacy = dependencyDefinition();
  legacy.selection = { packs: ['accounts'], checks: [] };
  assert.throws(() => compile(legacy), /require recipe bindings by level/);
});

test('campaign identity ignores JSON formatting but changes with study semantics', () => {
  const first = compile(definition());
  const reordered = definition();
  reordered.stacks.reverse();
  reordered.selection = { checks: [], packs: [] };
  const same = compile(reordered);
  assert.equal(same.contentSha256, first.contentSha256);
  const explicitDefaults = definition({ parallelism: 1,
    stacks: definition().stacks.map(stack => ({ ...stack, repetitions: 3 })) });
  assert.equal(compile(explicitDefaults).contentSha256, first.contentSha256);
  const changed = compile(definition({ repetitions: 4 }));
  assert.notEqual(changed.contentSha256, first.contentSha256);
  const partial = compile(definition({ selection: { packs: [],
    checks: ['ecommerce.feature.accounts.accounts.1a'] } }));
  assert.notEqual(partial.conditions[0].sha256, first.conditions[0].sha256);
  assert.equal(partial.conditions[0].requested.levels[0].task.sha256,
    partial.bindings[0]!.task!.sha256);
  assert.deepEqual(partial.conditions[0].requested.levels[0].selection.taskPacks,
    partial.bindings[0]!.selection!.taskPacks);
  const identityOnly = compile(definition({ selection: {
    packs: ['ecommerce.feature.accounts'], checks: [],
  } }));
  assert.deepEqual(identityOnly.bindings[0]!.selection!.taskPacks,
    ['ecommerce.feature.accounts']);
  assert.notEqual(identityOnly.bindings[0]!.task!.sha256, first.bindings[0]!.task!.sha256);
  assert.notEqual(identityOnly.conditions[0].sha256, first.conditions[0].sha256);
  const multiAgent = definition({ agents: [definition().agents[0]!,
    { adapter: 'fault-injection', adapterVersion: '1.2.0', model: 'deterministic' }] });
  const multiAgentReordered = structuredClone(multiAgent);
  multiAgentReordered.agents.reverse();
  assert.equal(compile(multiAgent).contentSha256, compile(multiAgentReordered).contentSha256);
});

test('balanced rotation covers every stack-agent condition and rotates the global lead', () => {
  const agents = [
    { adapter: 'deterministic', adapterVersion: '1.3.0', model: 'deterministic' },
    { adapter: 'fault-injection', adapterVersion: '1.2.0', model: 'deterministic' },
  ];
  const plan = compile(definition({ agents, repetitions: 6 }));
  for (let repetition = 1; repetition <= 6; repetition += 1) {
    const attempts = plan.attempts.filter(attempt => attempt.repetition === repetition);
    assert.equal(attempts.length, 6);
    assert.equal(new Set(attempts.map(attempt => `${attempt.agentAdapter}:${attempt.stack}`)).size, 6);
  }
  assert.equal(new Set(plan.attempts.filter(attempt => attempt.order === 1)
    .map(attempt => `${attempt.agentAdapter}:${attempt.stack}`)).size, 6);
});

test('guidance conditions are an independent campaign axis with stack-specific API material', () => {
  const conditions = [...definition().conditions, { id: 'neutral', version: '1.0.0',
    guidanceProfile: 'neutral@1.7.0', repairPolicy: 'scored-only@1.0.0' }];
  const plan = compile(definition({ conditions, repetitions: 1 }));
  assert.equal(plan.summary.attempts, 6);
  assert.equal(new Set(plan.attempts.map(attempt =>
    `${attempt.stack}:${attempt.condition.id}`)).size, 6);
  const neutralSpacetime = plan.attempts.filter(attempt =>
    attempt.stack === 'spacetime' && attempt.condition.id === 'neutral');
  assert(neutralSpacetime.every(attempt => attempt.skills.length === 1
    && attempt.skills[0] === 'spacetimedb-typescript-core'));
  assert(plan.attempts.filter(attempt => attempt.stack !== 'spacetime')
    .every(attempt => attempt.skills.length === 0));
});

test('modular campaigns bind requested, expected, and observed specifications independently', () => {
  const requestedSpecifications = ['ecommerce.spec.access-control@1.2.0'];
  const expected = compile(modularDefinition({ requested: requestedSpecifications,
    expected: ['ecommerce.spec.state-durability@1.1.0'] }));
  const observed = compile(modularDefinition({ requested: requestedSpecifications,
    observed: ['ecommerce.spec.state-durability@1.1.0'] }));
  const selected = expected.conditions[0].requested.levels[0];
  assert.equal(expected.bindings[0]!.recipe.id, 'ecommerce.sequential-l1');
  assert.equal(expected.bindings[0]!.selection, null,
    'condition-specific specification choices must not be flattened into a shared binding');
  assert.equal(selected.selection.schemaVersion, 3);
  assert.deepEqual(selected.selection.features, modularFeatures);
  assert.deepEqual(selected.selection.specifications, { requested: requestedSpecifications,
    expected: ['ecommerce.spec.state-durability@1.1.0'], observed: [] });
  assert(selected.selection.scoredChecks!.length > 0);
  assert.equal(selected.selection.observedChecks!.length, 0);
  assert.equal(observed.conditions[0].requested.levels[0].selection.observedChecks!.length, 4);
  assert.equal(expected.conditions[0].requested.levels[0].task.sha256,
    observed.conditions[0].requested.levels[0].task.sha256,
    'changing an undisclosed treatment must not change the task shown to the agent');
  assert(expected.conditions[0].requested.levels[0].selection.scoredPoints
    > observed.conditions[0].requested.levels[0].selection.scoredPoints,
  'expected specifications must contribute to the score');
  assert.notEqual(expected.conditions[0].requested.levels[0].selection.sha256,
    observed.conditions[0].requested.levels[0].selection.sha256);
  assert.notEqual(expected.conditions[0].sha256, observed.conditions[0].sha256);
});

test('campaigns reject unavailable specification versions and pack-selection mixing', () => {
  assert.throws(() => compile(modularDefinition({
    expected: ['ecommerce.spec.external-data-sync@1.0.0'],
  })), /has no expected specification/);
  assert.throws(() => compile(definition({ conditions: [{ ...definition().conditions[0]!,
    specifications: { levels: [{ level: 1, requested: [], expected: [], observed: [] }] },
  }] })), /pack selection cannot declare modular specifications/);
});

test('a one-repetition pilot is allowed and reports its exact sample size', () => {
  const plan = compile(definition({ repetitions: 1 }));
  assert.equal(plan.summary.repetitions, 1);
  assert.equal(plan.summary.attempts, 3);
});

test('campaigns freeze independent stack counts and bounded parallel capacity', () => {
  const stacks = definition().stacks.map(stack => ({ ...stack,
    repetitions: stack.id === 'postgres' ? 4 : stack.id === 'mongodb' ? 2 : 3 }));
  const plan = compile(definition({ stacks, repetitions: 1, parallelism: 6 }));
  assert.deepEqual(plan.summary.repetitionsByStack,
    { mongodb: 2, postgres: 4, spacetime: 3 });
  assert.equal(plan.summary.parallelism, 6);
  assert.equal(plan.summary.attempts, 9);
  assert.deepEqual(Object.fromEntries(['mongodb', 'postgres', 'spacetime'].map(stack =>
    [stack, plan.attempts.filter(attempt => attempt.stack === stack).length])),
  { mongodb: 2, postgres: 4, spacetime: 3 });
  assert.notEqual(plan.contentSha256, compile(definition({ stacks, repetitions: 1,
    parallelism: 5 })).contentSha256);
  assert.throws(() => validateCampaignDefinition(definition({ parallelism: 22 })), /parallelism/);
});

test('campaign validation rejects ambiguity, silent fallback, and incomplete analysis policy', () => {
  assert.throws(() => validateCampaignDefinition({ ...definition(), surprise: true }), /surprise.*unknown/);
  assert.throws(() => validateCampaignDefinition({ ...definition(), pricing: {
    ...definition().pricing, unit: 'USD-per-token',
  } }), /pricing\.unit/);
  assert.throws(() => validateCampaignDefinition({ ...definition(), pricing: {
    ...definition().pricing, models: { deterministic: {
      inputPerMillion: 0, output: 0, cacheWrite5m: 0, cacheWrite1h: 0, cacheRead: 0,
    } },
  } }), /inputPerMillion.*unknown/);
  assert.throws(() => validateCampaignDefinition({ ...definition(), mode: undefined }), /mode must be an object/);
  assert.throws(() => validateCampaignDefinition({ ...definition(), mode: {
    id: 'unknown', version: '1.0.0',
  } }), /unknown unknown@1\.0\.0/);
  assert.throws(() => validateCampaignDefinition({ ...definition(), mode: {
    id: 'dependency', version: '3.0.0', strikes: { default: 3, levels: {} },
  } }), /featureCatalog.*required/);
  assert.throws(() => validateCampaignDefinition({ ...definition(), mode: {
    id: 'sequential', version: '1.0.0', graph: 'not-allowed',
  } }), /graph is unknown for sequential mode/);
  assert.throws(() => validateCampaignDefinition(definition({ levels: [1, 3] })), /ascending and contiguous/);
  assert.throws(() => validateCampaignDefinition(definition({ stacks: [
    { id: 'postgres', adapterVersion: '1.4.0' }, { id: 'postgres', adapterVersion: '1.4.0' },
  ] })), /duplicates|name each stack once/);
  assert.throws(() => validateCampaignDefinition(definition({ attemptPolicy: {
    retries: 1, retryOn: [], excludeFromAnalysis: [],
  } })), /retryOn/);
  const providerRetry = validateCampaignDefinition(definition({ attemptPolicy: {
    retries: 1, retryOn: ['provider_failure'], excludeFromAnalysis: ['provider_failure'],
  } }));
  assert.deepEqual(providerRetry.attemptPolicy.retryOn, ['provider_failure']);
  assert.throws(() => compile(definition({ selection: { packs: [], checks: ['missing.check'] } })),
    /recipe has no check/);
  assert.throws(() => compile(definition({ conditions: [{
    id: 'bad', version: '1.0.0', guidanceProfile: 'missing@1.0.0',
    repairPolicy: 'scored-only@1.0.0',
  }] })), /missing@1.0.0|catalog/);
  assert.throws(() => validateCampaignDefinition(definition({ runtime: {
    releaseManifestSha256: null, controllerImage: 'stack-bench:latest', buildImage: null,
    platform: 'linux/amd64',
  } })), /exact image digest/);
});

test('frozen campaigns require exact runtime images', () => {
  assert.throws(() => compile(definition({ state: 'frozen' })), /maxCostUsdPerAttempt.*required/);
  const runtime = { releaseManifestSha256: 'a'.repeat(64),
    controllerImage: `registry.example/stack-bench-controller@sha256:${'b'.repeat(64)}`,
    buildImage: `registry.example/stack-bench-build@sha256:${'c'.repeat(64)}`,
    platform: 'linux/amd64' };
  const claudeAgent = [{ adapter: 'claude-code', adapterVersion: '1.17.2',
    model: 'claude-sonnet-5' }];
  const claudePricing = { currency: 'USD', unit: 'USD-per-million-tokens',
    capturedAt: '2026-08-12T00:00:00.000Z',
    source: 'test snapshot', models: { 'claude-sonnet-5': {
      input: 1, output: 1, cacheWrite5m: 1, cacheWrite1h: 1, cacheRead: 1,
    } } };
  const frozen = validateCampaignDefinition(definition({ state: 'frozen', levels: [1], runtime,
    agents: claudeAgent, pricing: claudePricing,
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 25 } }));
  assert.equal(frozen.state, 'frozen');
  const internal = validateCampaignDefinition(definition({ state: 'frozen', levels: [1], runtime: {
    ...runtime, releaseManifestSha256: null,
  }, agents: claudeAgent, pricing: claudePricing,
  budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 25 } }));
  assert.equal(internal.runtime.releaseManifestSha256, null);
});

test('frozen campaigns accept a resolved qualification result for every selected level', () => {
  const qualifiedBuildImages = new Set(compile(multiLevelDefinition([1, 2]),
    qualifiedCompilerOptions).bindings
    .map(binding => binding.calibration!.buildImage));
  assert.equal(qualifiedBuildImages.size, 1);
  const [qualifiedBuildImage] = qualifiedBuildImages;
  const runtime = { releaseManifestSha256: 'a'.repeat(64),
    controllerImage: `registry.example/stack-bench-controller@sha256:${'b'.repeat(64)}`,
    buildImage: `registry.example/stack-bench-build@${qualifiedBuildImage}`,
    platform: 'linux/amd64' };
  const claudeAgent = [{ adapter: 'claude-code', adapterVersion: '1.17.2',
    model: 'claude-sonnet-5' }];
  const claudePricing = { currency: 'USD', unit: 'USD-per-million-tokens',
    capturedAt: '2026-08-12T00:00:00.000Z',
    source: 'test snapshot', models: { 'claude-sonnet-5': {
      input: 1, output: 1, cacheWrite5m: 1, cacheWrite1h: 1, cacheRead: 1,
    } } };
  const l1l2 = multiLevelDefinition([1, 2]);
  Object.assign(l1l2, { state: 'frozen', runtime,
    agents: claudeAgent, pricing: claudePricing,
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 25 } });
  const compiled = compile(l1l2, qualifiedCompilerOptions);
  assert.equal(compiled.state, 'frozen');
  assert.deepEqual(compiled.bindings.map(binding => binding.level), [1, 2]);
});

test('frozen campaigns record a build image that has not been qualified', () => {
  const runtime = { releaseManifestSha256: 'a'.repeat(64),
    controllerImage: `registry.example/stack-bench-controller@sha256:${'b'.repeat(64)}`,
    buildImage: `registry.example/stack-bench-build@sha256:${'c'.repeat(64)}`,
    platform: 'linux/amd64' };
  const frozen = definition({ state: 'frozen', levels: [1], runtime,
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 25 } });
  const compiled = compile(frozen, qualifiedCompilerOptions);
  assert.equal(compiled.bindings[0]!.qualification.status, 'pending');
  assert(compiled.bindings[0]!.qualification.reasons
    .includes('build image does not match qualification evidence'));
});

test('frozen dependency campaigns record incomplete calibration coverage', () => {
  const value = dependencyDefinition();
  value.state = 'frozen';
  value.budgets.maxCostUsdPerAttempt = 25;
  value.runtime = {
    releaseManifestSha256: 'a'.repeat(64),
    controllerImage: `registry.example/stack-bench-controller@sha256:${'b'.repeat(64)}`,
    buildImage: `registry.example/stack-bench-build@${QUALIFIED_BUILD_IMAGE}`,
    platform: 'linux/amd64',
  };
  const calibrationResolver: CalibrationResolver = (release, options) => {
    const calibration = resolvedQualification(release, options);
    assert(calibration);
    calibration.qualification.checks = [];
    return calibration;
  };
  const incomplete = compile(value, { calibrationResolver,
    recipeResolver: promotedRecipe });
  assert.equal(incomplete.bindings[0]!.qualification.status, 'pending');
  assert(incomplete.bindings[0]!.qualification.reasons
    .includes('calibration does not cover 1 selected checks'));

  const coveredResolver: CalibrationResolver = (release, options) => {
    const calibration = resolvedQualification(release, options);
    assert(calibration);
    calibration.qualification.checks = ['ecommerce.feature.accounts.accounts.1a'];
    return calibration;
  };
  const covered = compile(value, { calibrationResolver: coveredResolver,
    recipeResolver: promotedRecipe });
  assert.equal(covered.state, 'frozen');
  assert.equal(covered.bindings[0]!.qualification.status, 'qualified');
});

test('frozen manifest validation does not hard-code an agent provider', () => {
  const qualifiedBuildImage = compile(definition(), qualifiedCompilerOptions)
    .bindings[0]!.calibration!.buildImage;
  const runtime = { releaseManifestSha256: 'a'.repeat(64),
    controllerImage: `registry.example/stack-bench-controller@sha256:${'b'.repeat(64)}`,
    buildImage: `registry.example/stack-bench-build@${qualifiedBuildImage}`,
    platform: 'linux/amd64' };
  const validated = validateCampaignDefinition(definition({ state: 'frozen', levels: [1], runtime,
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 25 } }));
  assert.equal(validated.agents[0]!.adapter, 'deterministic');
  assert.equal(compile(validated, qualifiedCompilerOptions).agents[0]!.adapter, 'deterministic');
});

test('the packaged model-free campaign example compiles without starting work', () => {
  const plan = compileCampaignFile(join(APPLIANCE_ROOT, 'campaign.example.json'));
  assert.equal(plan.state, 'draft');
  assert.deepEqual(plan.summary, { agents: 1, attempts: 9, conditions: 1,
    parallelism: 1, repetitions: 3,
    repetitionsByStack: { mongodb: 3, postgres: 3, spacetime: 3 }, stacks: 3 });
});

test('the packaged modular reference gate scores quality specifications without prompting them', () => {
  const plan = compileCampaignFile(join(APPLIANCE_ROOT,
    'campaign.product-brief-reference.json'));
  assert.equal(plan.state, 'draft');
  assert.deepEqual(plan.summary, { agents: 1, attempts: 6, conditions: 1,
    parallelism: 1, repetitions: 2,
    repetitionsByStack: { mongodb: 2, postgres: 2, spacetime: 2 }, stacks: 3 });
  assert.equal(plan.agents[0]!.adapter, 'reference-fixture');
  const expected = plan.conditions.find(condition => condition.id === 'product-brief-quality');
  assert(expected);
  assert.deepEqual(expected.requested.levels[0].selection.specifications,
    { requested: [], expected: [
      'ecommerce.spec.access-control@1.2.0',
      'ecommerce.spec.concurrency-safety@1.3.0',
      'ecommerce.spec.external-data-sync@1.1.0',
      'ecommerce.spec.live-state@1.2.0',
      'ecommerce.spec.state-durability@1.1.0',
      'ecommerce.spec.transactional-integrity@1.3.0',
    ], observed: [] });
  assert.equal(expected.requested.levels[0].selection.observedChecks!.length, 0);
  assert.equal(expected.requested.levels[0].selection.scoredPoints, 58);
  assert(expected.requested.levels[0].selection.scoredChecks!
    .some(check => check.treatment === 'expected'));
});

test('compiled campaign validation rejects a rewritten identity, schedule, or summary', () => {
  const plan = compile(definition());
  assert.deepEqual(validateCompiledCampaignPlan(plan), plan);
  assert.throws(() => validateCompiledCampaignPlan({ ...plan, contentSha256: 'a'.repeat(64) }),
    /content identity/);
  const schedule = structuredClone(plan);
  schedule.attempts[0].stack = schedule.attempts[0].stack === 'postgres' ? 'mongodb' : 'postgres';
  assert.throws(() => validateCompiledCampaignPlan(schedule), /attempt schedule/);
  const resolved = structuredClone(plan);
  resolved.bindings[0]!.promotion.status = plan.bindings[0]!.promotion.status === 'candidate'
    ? 'promoted' : 'candidate';
  assert.throws(() => validateCompiledCampaignPlan(resolved), /bindings.*current resolved inputs/);
  assert.throws(() => validateCompiledCampaignPlan({ ...plan,
    summary: { ...plan.summary, attempts: 99 } }), /summary/);
});

test('frozen plans remain inspectable after an engine upgrade but cannot execute there', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-history-'));
  try {
    const path = join(root, 'campaign.json');
    writeFileSync(path, `${JSON.stringify(definition(), null, 2)}\n`);
    const frozen = structuredClone(compileCampaignFile(path));
    frozen.identities.engine.sha256 = 'f'.repeat(64);
    frozen.contentSha256 = sha256(canonicalDefinitionJson({
      campaignSchemaVersion: frozen.campaignSchemaVersion,
      definition: frozen.definition,
      engine: frozen.identities.engine,
      bindings: frozen.bindings,
      stacks: frozen.stacks,
      agents: frozen.agents,
      conditions: frozen.conditions,
    }));
    assert.throws(() => validateCompiledCampaignPlan(frozen), /engine identity/);
    assert.equal(validateCompiledCampaignPlan(frozen,
      { requireCurrentInputs: false }).contentSha256, frozen.contentSha256);
    const tampered = structuredClone(frozen);
    tampered.attempts[0].model = 'rewritten-model';
    assert.throws(() => validateCompiledCampaignPlan(tampered,
      { requireCurrentInputs: false }), /attempt schedule/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
