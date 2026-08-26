import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { campaignIdentity, compileCampaignFile, validateCampaignDefinition,
  validateCompiledCampaignPlan } from '../src/campaigns/campaign-compiler.mjs';
import { attemptArgv } from '../src/campaigns/campaign-runner.mjs';
import { parseArgs } from '../commands/bench.mjs';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.mjs';
import { writeArtifact } from '../src/evidence/artifacts.mjs';
import { sha256 } from '../src/evidence/provenance.mjs';

function definition(overrides = {}) {
  return {
    schemaVersion: 3,
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
      { id: 'spacetime', adapterVersion: '1.0.0' },
      { id: 'postgres', adapterVersion: '1.3.0' },
      { id: 'mongodb', adapterVersion: '1.2.0' },
    ],
    agents: [{ adapter: 'deterministic', adapterVersion: '1.2.0', model: 'deterministic' }],
    conditions: [{ id: 'prescribed', version: '1.0.0',
      guidanceProfile: 'prescribed@1.0.0', repairPolicy: 'scored-only@1.0.0' }],
    repetitions: 3,
    ordering: { method: 'balanced-rotation', seed: 'published-seed-1' },
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: null },
    attemptPolicy: { retries: 1, retryOn: ['harness_failure', 'inconclusive'],
      excludeFromAnalysis: ['contaminated', 'harness_failure', 'inconclusive', 'ungraded'] },
    runtime: { releaseManifestSha256: null, controllerImage: null, buildImage: null,
      platform: 'linux/amd64' },
    pricing: { currency: 'USD', capturedAt: '2026-08-12T00:00:00.000Z',
      source: 'offline deterministic adapter', models: { deterministic: {
        inputPerMillion: 0, outputPerMillion: 0, cacheWritePerMillion: 0, cacheReadPerMillion: 0,
      } } },
    analysis: { primaryMetric: 'firstBuildScoreRate',
      secondaryMetrics: ['finalScoreRate', 'totalCostUsd', 'totalDurationMs', 'correctionSuccessRate',
        'correctionCostUsd', 'correctionSpendUsd', 'invalidAttemptRate'],
      dispersion: 'median-iqr', invalidAttempts: 'report-separately', missingData: 'no-imputation',
      comparisonUnit: 'stack-agent-condition-recipe' },
    ...overrides,
  };
}

function compile(value, options = {}) {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-'));
  const path = join(root, 'campaign.json');
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
  try { return compileCampaignFile(path, options); }
  finally { rmSync(root, { recursive: true, force: true }); }
}

const QUALIFIED_BUILD_IMAGE = `sha256:${'d'.repeat(64)}`;

function resolvedQualification(release) {
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
}

const resolvedQualificationOptions = { calibrationResolver: resolvedQualification };

const modularFeatures = [
  'ecommerce.feature.accounts',
  'ecommerce.feature.cart-checkout',
  'ecommerce.feature.catalog',
  'ecommerce.feature.purchasing',
  'ecommerce.feature.reviews',
  'ecommerce.feature.warehouse-admin',
];

function modularDefinition({ requested = [], expected = [], observed = [] } = {}) {
  return definition({
    repetitions: 1,
    selection: { levels: [{ level: 1, recipe: 'ecommerce.l1-modular@2.5.0',
      features: modularFeatures, checks: [] }] },
    conditions: [{ ...definition().conditions[0], specifications: { levels: [{ level: 1,
      requested, expected, observed }] } }],
  });
}

function dependencyDefinition() {
  const value = modularDefinition();
  value.mode = { id: 'dependency', version: '1.0.0' };
  delete value.levels;
  delete value.selection.levels[0].features;
  delete value.selection.levels[0].checks;
  delete value.conditions[0].specifications;
  value.progression = {
    schemaVersion: 2,
    kind: 'progression-mode',
    id: 'ecommerce-dependency',
    version: '1.0.0',
    state: 'draft',
    title: 'Ecommerce dependency fixture',
    policy: 'dependency-gated',
    strikes: { default: 2, levels: {} },
    nodes: [{ id: 'accounts', title: 'Accounts', questline: 'identity', dependencies: [],
      featureRefs: ['ecommerce.feature.accounts@1.1.0'], promptModules: [],
      gradingChecks: [{ id: 'ecommerce.feature.accounts.accounts.1a', points: 1 }] }],
    questlines: [{ id: 'identity', title: 'Identity', nodes: ['accounts'] }],
  };
  return value;
}

test('campaign compilation binds exact inputs and expands a balanced immutable attempt plan', () => {
  const plan = compile(definition(), resolvedQualificationOptions);
  assert.equal(plan.summary.attempts, 9);
  assert.equal(plan.bindings[0].recipe.id, 'ecommerce.l1-modular');
  assert.match(plan.bindings[0].recipe.contentSha256, /^[a-f0-9]{64}$/);
  assert.equal(plan.bindings[0].calibration.id, 'ecommerce.l1-modular-test-calibration');
  assert.equal(plan.bindings[0].selection.completeness, 'full');
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
});

test('dependency campaigns derive levels and freeze the progression identity in every attempt', () => {
  const plan = compile(dependencyDefinition());
  assert.deepEqual(plan.definition.levels, [1]);
  assert.equal(plan.definition.progression, undefined);
  assert.equal(plan.progression.identity.id, 'ecommerce-dependency');
  assert.equal(plan.progression.identity.version, '1.0.0');
  assert.match(plan.progression.identity.sha256, /^[a-f0-9]{64}$/);
  assert(plan.attempts.every(attempt =>
    canonicalDefinitionJson(attempt.progression)
      === canonicalDefinitionJson(plan.progression.identity)));
  assert(plan.attempts.every(attempt =>
    canonicalDefinitionJson(attempt.featureCatalog)
      === canonicalDefinitionJson(plan.progression.identity)));
  assert.deepEqual(validateCompiledCampaignPlan(plan), plan);
  assert.throws(() => validateCampaignDefinition({ ...dependencyDefinition(), levels: [1, 2] }),
    /levels.*progression/);
});

test('sequential campaigns can use the same feature catalog without dependency gating', () => {
  const definition = dependencyDefinition();
  definition.mode = { id: 'sequential', version: '1.0.0' };
  const plan = compile(definition);
  assert.equal(plan.attempts[0].progression, undefined);
  assert.deepEqual(plan.attempts[0].featureCatalog, plan.progression.identity);
  assert.deepEqual(validateCompiledCampaignPlan(plan), plan);
});

test('dependency bench input is bound to one fully validated campaign attempt', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dependency-plan-'));
  try {
    const plan = compile(dependencyDefinition());
    const planPath = join(root, 'plan.json');
    writeArtifact(planPath, { kind: 'campaign_plan', id: `${plan.id}-plan`, payload: plan });
    const attempt = plan.attempts[0];
    const argv = attemptArgv(plan, attempt, join(root, 'result'), 0, planPath);
    const args = parseArgs(['node', ...argv]);
    assert.deepEqual(args.runMode, attempt.mode);
    assert.deepEqual(args.experimentIdentity, campaignIdentity(plan));
    assert.deepEqual(args.progression, plan.progression);
    assert.deepEqual(args.progressionOwner, { schemaVersion: 1,
      campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
      attempt: { id: attempt.id, track: plan.definition.track, stack: attempt.stack,
        agentAdapter: attempt.agentAdapter,
        model: attempt.model, conditionSha256: attempt.condition.sha256 } });
    assert.throws(() => parseArgs(['node', ...argv, '--skip-probe']),
      /cannot override --skip-probe/);
    const changedParent = [...argv];
    changedParent[changedParent.indexOf('--parent-attempt-id') + 1] = 'different-attempt';
    assert.throws(() => parseArgs(['node', ...changedParent]),
      /does not match the requested campaign attempt/);
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
    const planPath = join(root, 'plan.json');
    writeArtifact(planPath, { kind: 'campaign_plan', id: `${plan.id}-plan`, payload: plan });
    const argv = attemptArgv(plan, plan.attempts[0], join(root, 'result'), 0, planPath);
    assert(argv.includes('--levels'));
    const args = parseArgs(['node', ...argv]);
    assert.deepEqual(args.runMode, plan.attempts[0].mode);
    assert.deepEqual(args.experimentIdentity, campaignIdentity(plan));
    assert.equal(args.progression, undefined);
    assert.deepEqual(args.featureCatalog, plan.progression);
    assert.deepEqual(args.levelList, [1]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('sequential campaign input retains its campaign and mode without a feature catalog', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-sequential-plan-'));
  try {
    const plan = compileCampaignFile(join(import.meta.dirname, '..', 'appliance',
      'campaign.example.json'));
    const planPath = join(root, 'plan.json');
    writeArtifact(planPath, { kind: 'campaign_plan', id: `${plan.id}-plan`, payload: plan });
    const attempt = plan.attempts[0];
    const argv = attemptArgv(plan, attempt, join(root, 'result'), 0, planPath);
    assert.equal(argv.includes('--feature-catalog-sha256'), false);
    const args = parseArgs(['node', ...argv]);
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
  value.selection.levels[0].features = ['ecommerce.feature.accounts'];
  assert.throws(() => compile(value), /features: is unknown/);
  const specifications = dependencyDefinition();
  specifications.conditions[0].specifications = { levels: [{ level: 1,
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
    partial.bindings[0].task.sha256);
  assert.deepEqual(partial.conditions[0].requested.levels[0].selection.taskPacks,
    partial.bindings[0].selection.taskPacks);
  const identityOnly = compile(definition({ selection: {
    packs: ['ecommerce.feature.accounts'], checks: [],
  } }));
  assert.deepEqual(identityOnly.bindings[0].selection.taskPacks,
    ['ecommerce.feature.accounts']);
  assert.notEqual(identityOnly.bindings[0].task.sha256, first.bindings[0].task.sha256);
  assert.notEqual(identityOnly.conditions[0].sha256, first.conditions[0].sha256);
  const multiAgent = definition({ agents: [definition().agents[0],
    { adapter: 'fault-injection', adapterVersion: '1.0.0', model: 'deterministic' }] });
  const multiAgentReordered = structuredClone(multiAgent);
  multiAgentReordered.agents.reverse();
  assert.equal(compile(multiAgent).contentSha256, compile(multiAgentReordered).contentSha256);
});

test('balanced rotation covers every stack-agent condition and rotates the global lead', () => {
  const agents = [
    { adapter: 'deterministic', adapterVersion: '1.2.0', model: 'deterministic' },
    { adapter: 'fault-injection', adapterVersion: '1.0.0', model: 'deterministic' },
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
    guidanceProfile: 'neutral@1.0.0', repairPolicy: 'scored-only@1.0.0' }];
  const plan = compile(definition({ conditions, repetitions: 1 }));
  assert.equal(plan.summary.attempts, 6);
  assert.equal(new Set(plan.attempts.map(attempt =>
    `${attempt.stack}:${attempt.condition.id}`)).size, 6);
  const spacetime = plan.attempts.filter(attempt => attempt.stack === 'spacetime');
  assert(spacetime.every(attempt => attempt.skills.length === 2));
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
  assert.equal(expected.bindings[0].recipe.id, 'ecommerce.l1-modular');
  assert.equal(expected.bindings[0].promotion.status, 'promoted');
  assert.equal(expected.bindings[0].selection, null,
    'condition-specific specification choices must not be flattened into a shared binding');
  assert.equal(selected.selection.schemaVersion, 3);
  assert.deepEqual(selected.selection.features, modularFeatures);
  assert.deepEqual(selected.selection.specifications, { requested: requestedSpecifications,
    expected: ['ecommerce.spec.state-durability@1.1.0'], observed: [] });
  assert(selected.selection.scoredChecks.length > 0);
  assert.equal(selected.selection.observedChecks.length, 0);
  assert.equal(observed.conditions[0].requested.levels[0].selection.observedChecks.length, 4);
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

test('campaigns reject unavailable specification versions and legacy mixing', () => {
  assert.throws(() => compile(modularDefinition({
    expected: ['ecommerce.spec.external-data-sync@1.0.0'],
  })), /has no expected specification/);
  assert.throws(() => compile(definition({ conditions: [{ ...definition().conditions[0],
    specifications: { levels: [{ level: 1, requested: [], expected: [], observed: [] }] },
  }] })), /legacy selection cannot declare modular specifications/);
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
  assert.throws(() => validateCampaignDefinition({ ...definition(), mode: undefined }), /mode must be an object/);
  assert.throws(() => validateCampaignDefinition({ ...definition(), mode: {
    id: 'unknown', version: '1.0.0',
  } }), /unknown unknown@1\.0\.0/);
  assert.throws(() => validateCampaignDefinition({ ...definition(), mode: {
    id: 'dependency', version: '1.0.0',
  } }), /progression.*required/);
  assert.throws(() => validateCampaignDefinition({ ...definition(), mode: {
    id: 'sequential', version: '1.0.0', graph: 'not-allowed',
  } }), /graph is unknown for sequential mode/);
  assert.throws(() => validateCampaignDefinition(definition({ levels: [1, 3] })), /ascending and contiguous/);
  assert.throws(() => validateCampaignDefinition(definition({ stacks: [
    { id: 'postgres', adapterVersion: '1.3.0' }, { id: 'postgres', adapterVersion: '1.3.0' },
  ] })), /duplicates|name each stack once/);
  assert.throws(() => validateCampaignDefinition(definition({ attemptPolicy: {
    retries: 1, retryOn: [], excludeFromAnalysis: [],
  } })), /retryOn/);
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
  const claudeAgent = [{ adapter: 'claude-code', adapterVersion: '1.15.0',
    model: 'claude-sonnet-5' }];
  const claudePricing = { currency: 'USD', capturedAt: '2026-08-12T00:00:00.000Z',
    source: 'test snapshot', models: { 'claude-sonnet-5': {
      inputPerMillion: 1, outputPerMillion: 1, cacheWritePerMillion: 1, cacheReadPerMillion: 1,
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
  const qualifiedBuildImages = new Set(compile(definition({ levels: [1, 2] }),
    resolvedQualificationOptions).bindings
    .map(binding => binding.calibration.buildImage));
  assert.equal(qualifiedBuildImages.size, 1);
  const [qualifiedBuildImage] = qualifiedBuildImages;
  const runtime = { releaseManifestSha256: 'a'.repeat(64),
    controllerImage: `registry.example/stack-bench-controller@sha256:${'b'.repeat(64)}`,
    buildImage: `registry.example/stack-bench-build@${qualifiedBuildImage}`,
    platform: 'linux/amd64' };
  const claudeAgent = [{ adapter: 'claude-code', adapterVersion: '1.15.0',
    model: 'claude-sonnet-5' }];
  const claudePricing = { currency: 'USD', capturedAt: '2026-08-12T00:00:00.000Z',
    source: 'test snapshot', models: { 'claude-sonnet-5': {
      inputPerMillion: 1, outputPerMillion: 1, cacheWritePerMillion: 1, cacheReadPerMillion: 1,
    } } };
  const l1l2 = definition({ state: 'frozen', levels: [1, 2], runtime,
    agents: claudeAgent, pricing: claudePricing,
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 25 } });
  const compiled = compile(l1l2, resolvedQualificationOptions);
  assert.equal(compiled.state, 'frozen');
  assert.deepEqual(compiled.bindings.map(binding => binding.level), [1, 2]);
});

test('frozen campaigns require the build image used for qualification', () => {
  const runtime = { releaseManifestSha256: 'a'.repeat(64),
    controllerImage: `registry.example/stack-bench-controller@sha256:${'b'.repeat(64)}`,
    buildImage: `registry.example/stack-bench-build@sha256:${'c'.repeat(64)}`,
    platform: 'linux/amd64' };
  const frozen = definition({ state: 'frozen', levels: [1], runtime,
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 25 } });
  assert.throws(() => compile(frozen, resolvedQualificationOptions),
    /buildImage does not match the qualified build image/);
});

test('frozen manifest validation does not hard-code an agent provider', () => {
  const qualifiedBuildImage = compile(definition(), resolvedQualificationOptions)
    .bindings[0].calibration.buildImage;
  const runtime = { releaseManifestSha256: 'a'.repeat(64),
    controllerImage: `registry.example/stack-bench-controller@sha256:${'b'.repeat(64)}`,
    buildImage: `registry.example/stack-bench-build@${qualifiedBuildImage}`,
    platform: 'linux/amd64' };
  const validated = validateCampaignDefinition(definition({ state: 'frozen', levels: [1], runtime,
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 25 } }));
  assert.equal(validated.agents[0].adapter, 'deterministic');
  assert.equal(compile(validated, resolvedQualificationOptions).agents[0].adapter, 'deterministic');
});

test('the packaged model-free campaign example compiles without starting work', () => {
  const plan = compileCampaignFile(join(import.meta.dirname, '..', 'appliance', 'campaign.example.json'));
  assert.equal(plan.state, 'draft');
  assert.deepEqual(plan.summary, { agents: 1, attempts: 9, conditions: 1,
    parallelism: 1, repetitions: 3,
    repetitionsByStack: { mongodb: 3, postgres: 3, spacetime: 3 }, stacks: 3 });
});

test('the packaged modular reference gate scores quality specifications without prompting them', () => {
  const plan = compileCampaignFile(join(import.meta.dirname, '..', 'appliance',
    'campaign.product-brief-reference.json'));
  assert.equal(plan.state, 'draft');
  assert.deepEqual(plan.summary, { agents: 1, attempts: 6, conditions: 1,
    parallelism: 1, repetitions: 2,
    repetitionsByStack: { mongodb: 2, postgres: 2, spacetime: 2 }, stacks: 3 });
  assert.equal(plan.agents[0].adapter, 'reference-fixture');
  const expected = plan.conditions.find(condition => condition.id === 'product-brief-quality');
  assert.deepEqual(expected.requested.levels[0].selection.specifications,
    { requested: [], expected: [
      'ecommerce.spec.access-control@1.2.0',
      'ecommerce.spec.concurrency-safety@1.3.0',
      'ecommerce.spec.external-data-sync@1.1.0',
      'ecommerce.spec.live-state@1.2.0',
      'ecommerce.spec.state-durability@1.1.0',
      'ecommerce.spec.transactional-integrity@1.3.0',
    ], observed: [] });
  assert.equal(expected.requested.levels[0].selection.observedChecks.length, 0);
  assert.equal(expected.requested.levels[0].selection.scoredPoints, 58);
  assert(expected.requested.levels[0].selection.scoredChecks
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
  resolved.bindings[0].promotion.status = 'candidate';
  assert.throws(() => validateCompiledCampaignPlan(resolved), /bindings.*current resolved inputs/);
  assert.throws(() => validateCompiledCampaignPlan({ ...plan,
    summary: { ...plan.summary, attempts: 99 } }), /summary/);
});

test('stored plans remain readable across engine upgrades but cannot execute there', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-history-'));
  try {
    const path = join(root, 'campaign.json');
    writeFileSync(path, `${JSON.stringify(definition(), null, 2)}\n`);
    const historical = structuredClone(compileCampaignFile(path));
    historical.identities.engine.sha256 = 'f'.repeat(64);
    historical.contentSha256 = sha256(canonicalDefinitionJson({
      campaignSchemaVersion: historical.campaignSchemaVersion,
      definition: historical.definition,
      engine: historical.identities.engine,
      bindings: historical.bindings,
      stacks: historical.stacks,
      agents: historical.agents,
      conditions: historical.conditions,
    }));
    assert.throws(() => validateCompiledCampaignPlan(historical), /engine identity/);
    assert.equal(validateCompiledCampaignPlan(historical,
      { requireCurrentInputs: false }).contentSha256, historical.contentSha256);
    const tampered = structuredClone(historical);
    tampered.attempts[0].model = 'rewritten-model';
    assert.throws(() => validateCompiledCampaignPlan(tampered,
      { requireCurrentInputs: false }), /attempt schedule/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
