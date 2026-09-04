import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { campaignIdentity, compileCampaignFile, validateCampaignDefinition,
  validateCompiledCampaignPlan } from '../src/campaigns/campaign-compiler.js';

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

test('campaign identities change when a campaign choice changes', () => {
  const first = compile(manifest('campaign.example.json'));
  const changed = manifest('campaign.example.json');
  (changed.ordering as { seed: string }).seed = 'another-seed';
  const second = compile(changed);
  assert.notEqual(second.contentSha256, first.contentSha256);
});
