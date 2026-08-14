import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { campaignIdentity, compileCampaignFile, validateCampaignDefinition,
  validateCompiledCampaignPlan } from '../campaign-compiler.mjs';

function definition(overrides = {}) {
  return {
    schemaVersion: 1,
    kind: 'campaign-manifest',
    id: 'ecommerce-l1-comparison',
    version: '1.0.0',
    state: 'draft',
    title: 'Ecommerce L1 comparison',
    track: 'ecommerce',
    levels: [1],
    selection: { packs: [], checks: [] },
    stacks: [
      { id: 'spacetime', adapterVersion: '1.0.0' },
      { id: 'postgres', adapterVersion: '1.1.0' },
      { id: 'mongodb', adapterVersion: '1.1.0' },
    ],
    agents: [{ adapter: 'deterministic', adapterVersion: '1.0.0', model: 'deterministic',
      guidance: 'prescribed', skills: [] }],
    repetitions: 3,
    ordering: { method: 'balanced-rotation', seed: 'published-seed-1' },
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: null },
    attemptPolicy: { retries: 1, retryOn: ['harness_failure'],
      excludeFromAnalysis: ['contaminated', 'harness_failure', 'ungraded'] },
    runtime: { releaseManifestSha256: null, controllerImage: null, buildImage: null,
      platform: 'linux/amd64' },
    pricing: { currency: 'USD', capturedAt: '2026-08-12T00:00:00.000Z',
      source: 'offline deterministic adapter', models: { deterministic: {
        inputPerMillion: 0, outputPerMillion: 0, cacheWritePerMillion: 0, cacheReadPerMillion: 0,
      } } },
    analysis: { primaryMetric: 'firstBuildScoreRate',
      secondaryMetrics: ['finalScoreRate', 'totalCostUsd', 'totalDurationMs', 'invalidAttemptRate'],
      dispersion: 'median-iqr', invalidAttempts: 'report-separately', missingData: 'no-imputation',
      comparisonUnit: 'stack-agent-recipe' },
    ...overrides,
  };
}

function compile(value) {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-'));
  const path = join(root, 'campaign.json');
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
  try { return compileCampaignFile(path); }
  finally { rmSync(root, { recursive: true, force: true }); }
}

test('campaign compilation binds exact inputs and expands a balanced immutable attempt plan', () => {
  const plan = compile(definition());
  assert.equal(plan.summary.attempts, 9);
  assert.equal(plan.bindings[0].recipe.id, 'ecommerce.l1-standard');
  assert.match(plan.bindings[0].recipe.contentSha256, /^[a-f0-9]{64}$/);
  assert.equal(plan.bindings[0].calibration.id, 'ecommerce.l1-standard-calibration');
  assert.equal(plan.bindings[0].selection.completeness, 'full');
  assert.deepEqual(campaignIdentity(plan), { id: plan.id, version: '1.0.0',
    sha256: plan.contentSha256, state: 'draft' });
  for (let repetition = 1; repetition <= 3; repetition += 1) {
    const attempts = plan.attempts.filter(attempt => attempt.repetition === repetition);
    assert.deepEqual([...attempts.map(attempt => attempt.order)].sort(), [1, 2, 3]);
    assert.deepEqual([...attempts.map(attempt => attempt.stack)].sort(), ['mongodb', 'postgres', 'spacetime']);
  }
  assert.equal(new Set(plan.attempts.filter(attempt => attempt.order === 1)
    .map(attempt => attempt.stack)).size, 3, 'each stack must lead one repetition');
});

test('campaign identity ignores JSON formatting but changes with study semantics', () => {
  const first = compile(definition());
  const reordered = definition();
  reordered.stacks.reverse();
  reordered.selection = { checks: [], packs: [] };
  const same = compile(reordered);
  assert.equal(same.contentSha256, first.contentSha256);
  const changed = compile(definition({ repetitions: 4 }));
  assert.notEqual(changed.contentSha256, first.contentSha256);
  const multiAgent = definition({ agents: [definition().agents[0],
    { adapter: 'fault-injection', adapterVersion: '1.0.0', model: 'deterministic',
      guidance: 'prescribed', skills: [] }] });
  const multiAgentReordered = structuredClone(multiAgent);
  multiAgentReordered.agents.reverse();
  assert.equal(compile(multiAgent).contentSha256, compile(multiAgentReordered).contentSha256);
});

test('balanced rotation covers every stack-agent condition and rotates the global lead', () => {
  const agents = [
    { adapter: 'deterministic', adapterVersion: '1.0.0', model: 'deterministic',
      guidance: 'prescribed', skills: [] },
    { adapter: 'fault-injection', adapterVersion: '1.0.0', model: 'deterministic',
      guidance: 'prescribed', skills: [] },
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

test('a one-repetition pilot is allowed and reports its exact sample size', () => {
  const plan = compile(definition({ repetitions: 1 }));
  assert.equal(plan.summary.repetitions, 1);
  assert.equal(plan.summary.attempts, 3);
});

test('campaign validation rejects ambiguity, silent fallback, and incomplete analysis policy', () => {
  assert.throws(() => validateCampaignDefinition({ ...definition(), surprise: true }), /surprise.*unknown/);
  assert.throws(() => validateCampaignDefinition(definition({ levels: [1, 3] })), /ascending and contiguous/);
  assert.throws(() => validateCampaignDefinition(definition({ stacks: [
    { id: 'postgres', adapterVersion: '1.1.0' }, { id: 'postgres', adapterVersion: '1.1.0' },
  ] })), /duplicates|name each stack once/);
  assert.throws(() => validateCampaignDefinition(definition({ attemptPolicy: {
    retries: 1, retryOn: [], excludeFromAnalysis: [],
  } })), /retryOn/);
  assert.throws(() => compile(definition({ selection: { packs: [], checks: ['missing.check'] } })),
    /recipe has no check/);
  assert.throws(() => compile(definition({ agents: [{ adapter: 'deterministic',
    adapterVersion: '1.0.0', model: 'deterministic', guidance: 'minimal', skills: [] }] })),
  /minimal guidance unsupported by spacetime/);
  assert.throws(() => validateCampaignDefinition(definition({ runtime: {
    releaseManifestSha256: null, controllerImage: 'stack-bench:latest', buildImage: null,
    platform: 'linux/amd64',
  } })), /exact image digest/);
});

test('frozen campaigns require exact runtime images and accept qualified levels', () => {
  assert.throws(() => compile(definition({ state: 'frozen' })), /maxCostUsdPerAttempt.*required/);
  const runtime = { releaseManifestSha256: 'a'.repeat(64),
    controllerImage: `registry.example/stack-bench-controller@sha256:${'b'.repeat(64)}`,
    buildImage: `registry.example/stack-bench-build@sha256:${'c'.repeat(64)}`,
    platform: 'linux/amd64' };
  const claudeAgent = [{ adapter: 'claude-code', adapterVersion: '1.4.0',
    model: 'claude-sonnet-5', guidance: 'prescribed', skills: [] }];
  const claudePricing = { currency: 'USD', capturedAt: '2026-08-12T00:00:00.000Z',
    source: 'test snapshot', models: { 'claude-sonnet-5': {
      inputPerMillion: 1, outputPerMillion: 1, cacheWritePerMillion: 1, cacheReadPerMillion: 1,
    } } };
  const frozen = compile(definition({ state: 'frozen', levels: [1], runtime, agents: claudeAgent,
    pricing: claudePricing,
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 25 } }));
  assert.equal(frozen.state, 'frozen');
  const internal = compile(definition({ state: 'frozen', levels: [1], runtime: {
    ...runtime, releaseManifestSha256: null,
  }, agents: claudeAgent, pricing: claudePricing,
  budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 25 } }));
  assert.equal(internal.definition.runtime.releaseManifestSha256, null);
  const l1l2 = compile(definition({ state: 'frozen', levels: [1, 2], runtime,
    agents: claudeAgent, pricing: claudePricing,
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 25 } }));
  assert.deepEqual(l1l2.bindings.map(binding => binding.level), [1, 2]);
  assert(l1l2.bindings.every(binding => binding.promotion.status === 'promoted'));
});

test('frozen manifest validation does not hard-code an agent provider', () => {
  const runtime = { releaseManifestSha256: 'a'.repeat(64),
    controllerImage: `registry.example/stack-bench-controller@sha256:${'b'.repeat(64)}`,
    buildImage: `registry.example/stack-bench-build@sha256:${'c'.repeat(64)}`,
    platform: 'linux/amd64' };
  const validated = validateCampaignDefinition(definition({ state: 'frozen', levels: [1], runtime,
    budgets: { fixRounds: 3, attemptTimeoutMinutes: 240, maxCostUsdPerAttempt: 25 } }));
  assert.equal(validated.agents[0].adapter, 'deterministic');
  assert.equal(compile(validated).state, 'frozen');
});

test('the packaged model-free campaign example compiles without starting work', () => {
  const plan = compileCampaignFile(join(import.meta.dirname, '..', 'appliance', 'campaign.example.json'));
  assert.equal(plan.state, 'draft');
  assert.deepEqual(plan.summary, { agents: 1, attempts: 9, repetitions: 3, stacks: 3 });
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
