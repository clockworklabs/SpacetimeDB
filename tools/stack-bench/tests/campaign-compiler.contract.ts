import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { campaignIdentity, compileCampaignFile, validateCampaignDefinition,
  validateCompiledCampaignPlan } from '../src/campaigns/campaign-compiler.js';
import { attemptArgv } from '../src/campaigns/campaign-runner.js';
import { writeArtifact } from '../src/evidence/artifacts.js';
import { parseBenchArguments } from '../commands/bench-arguments.js';

const APPLIANCE = resolve(STACK_BENCH_ROOT, 'appliance');

function manifest(name: string): Record<string, unknown> {
  return JSON.parse(readFileSync(join(APPLIANCE, name), 'utf8')) as Record<string, unknown>;
}

function compile(value: unknown) {
  const directory = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-'));
  const path = join(directory, 'campaign.json');
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
  try { return compileCampaignFile(path); }
  finally { rmSync(directory, { recursive: true, force: true }); }
}

test('a campaign preserves its own version and state while binding authored content by hash', () => {
  const plan = compile(manifest('campaign.example.json'));
  assert.equal(plan.definition.version, '2.0.0');
  assert.equal(plan.definition.state, 'draft');
  assert.equal(plan.bindings[0]?.recipe.id, 'ecommerce.sequential-l1');
  assert.match(plan.bindings[0]?.recipe.contentSha256 ?? '', /^[a-f0-9]{64}$/);
  assert.deepEqual(campaignIdentity(plan), {
    id: plan.id,
    version: plan.version,
    sha256: plan.contentSha256,
    state: plan.state,
  });
  assert.deepEqual(validateCompiledCampaignPlan(plan), plan);
});

test('dependency campaigns bind a graph and feature catalog by stable ID and content hash', () => {
  const plan = compile(manifest('campaign.ecommerce-progression-reference.json'));
  assert(plan.featureCatalog && plan.dependencyPolicy);
  assert.match(plan.featureCatalog.identity.contentSha256, /^[a-f0-9]{64}$/);
  assert.match(plan.dependencyPolicy.identity.contentSha256, /^[a-f0-9]{64}$/);
  assert.equal(plan.featureCatalog.identity.id, 'ecommerce.questlines');
  assert(plan.attempts.every(attempt => attempt.featureCatalog?.contentSha256
    === plan.featureCatalog?.identity.contentSha256));
  assert(plan.attempts.every(attempt => attempt.dependencyPolicy?.contentSha256
    === plan.dependencyPolicy?.identity.contentSha256));
});

test('campaign definitions reject versioned authored references', () => {
  const value = manifest('campaign.example.json');
  const selection = value.selection as { levels: Array<{ recipe: string }> };
  selection.levels[0]!.recipe = 'ecommerce.sequential-l1@2.5.0';
  assert.throws(() => validateCampaignDefinition(value), /recipe.*invalid/);
});

test('prior interface retention defaults on in new plans and freezes the JSON opt-out', () => {
  const value = manifest('campaign.ecommerce-progression-reference.json');
  assert.equal(validateCampaignDefinition(value).mode.retainPriorContracts, undefined,
    'stored definitions without the option must not be reinterpreted');
  const enabled = compile(value);
  assert.equal(enabled.definition.mode.retainPriorContracts, true);
  assert(enabled.attempts.every(attempt => attempt.mode.retainPriorContracts === true));
  const disabled = compile({ ...value,
    mode: { ...(value.mode as object), retainPriorContracts: false } });
  assert.equal(disabled.definition.mode.retainPriorContracts, false);
  assert.notEqual(enabled.contentSha256, disabled.contentSha256);
  assert.deepEqual(enabled.bindings, disabled.bindings, 'the authored grading definition is unchanged');
  assert.deepEqual(validateCompiledCampaignPlan(enabled), enabled);
  assert.deepEqual(validateCompiledCampaignPlan(disabled), disabled);
  const rewritten = structuredClone(enabled);
  rewritten.definition.mode.retainPriorContracts = false;
  assert.throws(() => validateCompiledCampaignPlan(rewritten), /identity|match/);
  const directory = mkdtempSync(join(tmpdir(), 'stack-bench-retained-contracts-'));
  try {
    const path = join(directory, 'plan.json');
    for (const plan of [enabled, disabled]) {
      const attempt = plan.attempts[0]!;
      writeArtifact(path, { kind: 'campaign_plan', id: plan.id, payload: plan });
      const argv = attemptArgv(plan, attempt, join(directory, 'out'), 0, path);
      const parsed = parseBenchArguments(['node', ...argv]);
      assert.equal(parsed.retainPriorContracts, plan === enabled);
    }
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('campaign runtime accepts immutable local IDs and registry digests, but rejects tags', () => {
  const value = manifest('campaign.paid-l1.json');
  value.state = 'frozen';
  const runtime = value.runtime as Record<string, unknown>;
  const id = `sha256:${'a'.repeat(64)}`;
  runtime.controllerImage = id;
  runtime.buildImage = id;
  for (const field of ['controllerImage', 'buildImage']) {
    for (const image of [id, `stack-bench@${id}`]) {
      runtime[field] = image;
      assert.equal(validateCampaignDefinition(value).runtime[field as 'controllerImage' | 'buildImage'], image);
    }
    for (const image of ['stack-bench:local', 'stack-bench:latest', 'sha256:abc']) {
      runtime[field] = image;
      assert.throws(() => validateCampaignDefinition(value), /exact image digest reference/);
    }
    runtime[field] = id;
  }
});

test('campaign identities change when a campaign choice changes', () => {
  const first = compile(manifest('campaign.example.json'));
  const changed = manifest('campaign.example.json');
  (changed.ordering as { seed: string }).seed = 'another-seed';
  const second = compile(changed);
  assert.notEqual(second.contentSha256, first.contentSha256);
});


test('cost/completion thresholds are declared numeric campaign policy, not inferred from results', () => {
  const value = manifest('campaign.example.json');
  const analysis = value.analysis as Record<string, unknown>;
  analysis.spendThresholdsUsd = [0, 2, 5];
  analysis.completionTargets = [0.5, 1];
  assert.deepEqual(validateCampaignDefinition(value).analysis.spendThresholdsUsd, [0, 2, 5]);
  analysis.completionTargets = [1.1];
  assert.throws(() => validateCampaignDefinition(value), /completionTargets/);
  analysis.completionTargets = [1];
  analysis.spendThresholdsUsd = [-1];
  assert.throws(() => validateCampaignDefinition(value), /spendThresholdsUsd/);
});
