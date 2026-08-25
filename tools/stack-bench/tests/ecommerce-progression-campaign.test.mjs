import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { AGENT_ADAPTER_REGISTRY } from '../src/agents/agent-adapters.mjs';
import { compileCampaignFile, validateCompiledCampaignPlan }
  from '../src/campaigns/campaign-compiler.mjs';
import { parseReferenceAgentArgs } from '../src/references/reference-agent.mjs';
import { loadReferenceRegistry, selectReferenceFixture, validateReferenceRegistry }
  from '../src/references/reference-fixtures.mjs';

const root = join(import.meta.dirname, '..');
const campaignPath = join(root, 'appliance', 'campaign.ecommerce-progression-reference.json');

test('the ecommerce reference pilot resolves the exact L1-L5 progression inputs', () => {
  const plan = compileCampaignFile(campaignPath);
  assert.deepEqual(plan.definition.levels, [1, 2, 3, 4, 5]);
  assert.equal(plan.progression.identity.id, 'ecommerce.questlines');
  assert.equal(plan.progression.identity.version, '1.0.0');
  assert.deepEqual(plan.bindings.map(binding => `${binding.recipe.id}@${binding.recipe.version}`),
    Array(5).fill('ecommerce.progression-catalog@1.0.0'));
  assert.deepEqual(plan.attempts.map(attempt => attempt.stack).sort(),
    ['mongodb', 'postgres', 'spacetime']);
  assert(plan.attempts.every(attempt => attempt.agentAdapter === 'reference-fixture'));
  assert.equal(validateCompiledCampaignPlan(plan).contentSha256, plan.contentSha256);
});

test('every campaign level resolves one candidate reference for each stack', () => {
  const registry = loadReferenceRegistry();
  assert.deepEqual(validateReferenceRegistry(registry), { ok: true, issues: [] });
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    for (let level = 1; level <= 5; level += 1) {
      const fixture = selectReferenceFixture(registry, {
        backend,
        track: 'ecommerce',
        level,
        recipe: 'ecommerce.progression-catalog@1.0.0',
      });
      assert.equal(fixture.status, 'candidate');
      assert.equal(fixture.targetPath, `reference-apps/ecommerce/progression/${backend}`);
    }
  }
});

test('the model-free reference adapter can advance through progression levels', () => {
  const adapter = AGENT_ADAPTER_REGISTRY.get('reference-fixture');
  assert.deepEqual(adapter.modes, ['build', 'upgrade']);
  const parsed = parseReferenceAgentArgs([
    'node', 'reference-agent.mjs',
    '--backend', 'mongodb',
    '--app', 'app',
    '--track', 'ecommerce',
    '--level', '2',
    '--run-index', '0',
    '--mode', 'upgrade',
  ]);
  assert.equal(parsed.mode, 'upgrade');
  assert.equal(parsed.level, 2);
});
