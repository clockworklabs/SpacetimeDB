import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { AGENT_ADAPTER_REGISTRY } from '../src/agents/agent-adapters.mjs';
import { compileCampaignFile, validateCompiledCampaignPlan }
  from '../src/campaigns/campaign-compiler.mjs';
import { attemptArgv, runCampaignAdmission } from '../src/campaigns/campaign-runner.mjs';
import { parseArgs } from '../commands/bench.mjs';
import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.mjs';
import { writeArtifact } from '../src/evidence/artifacts.mjs';
import { progressionEngine } from '../src/progression/progression-engine.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';
import { parseReferenceAgentArgs } from '../src/references/reference-agent.mjs';
import { loadReferenceRegistry, selectReferenceFixture, validateReferenceRegistry }
  from '../src/references/reference-fixtures.mjs';

const root = join(import.meta.dirname, '..');
const campaignPath = join(root, 'appliance', 'campaign.ecommerce-progression-reference.json');

function collectInputAttributes(value, attributes = new Set()) {
  if (Array.isArray(value)) value.forEach(item => collectInputAttributes(item, attributes));
  else if (value && typeof value === 'object') {
    if (typeof value.input?.attribute === 'string') attributes.add(value.input.attribute);
    Object.values(value).forEach(item => collectInputAttributes(item, attributes));
  }
  return attributes;
}

function sourceText(directory) {
  return readdirSync(directory, { recursive: true, withFileTypes: true })
    .filter(entry => entry.isFile() && /\.(?:js|jsx|ts|tsx|html)$/.test(entry.name))
    .map(entry => readFileSync(join(entry.parentPath, entry.name), 'utf8')).join('\n');
}

test('the ecommerce reference pilot resolves the exact L1-L5 progression inputs', () => {
  const plan = compileCampaignFile(campaignPath);
  assert.deepEqual(plan.definition.levels, [1, 2, 3, 4, 5]);
  assert.equal(plan.featureCatalog.identity.id, 'ecommerce.questlines');
  assert.equal(plan.featureCatalog.identity.version, '1.0.0');
  assert.equal(plan.dependencyPolicy.identity.id, 'dependency-gated');
  assert.deepEqual(plan.bindings.map(binding => `${binding.recipe.id}@${binding.recipe.version}`),
    Array(5).fill('ecommerce.progression-catalog@1.0.0'));
  assert.deepEqual(plan.attempts.map(attempt => attempt.stack).sort(),
    ['mongodb', 'postgres', 'spacetime']);
  assert(plan.attempts.every(attempt => attempt.agentAdapter === 'reference-fixture'));
  assert.equal(validateCompiledCampaignPlan(plan).contentSha256, plan.contentSha256);
});

test('sequential mode can run a prefix of the same ecommerce feature catalog', () => {
  const directory = mkdtempSync(join(tmpdir(), 'stack-bench-sequential-prefix-'));
  try {
    const manifest = JSON.parse(readFileSync(campaignPath, 'utf8'));
    manifest.id = 'ecommerce-sequential-prefix-proof';
    manifest.mode = { id: 'sequential', version: '1.0.0' };
    manifest.levels = [1, 2, 3];
    manifest.selection.levels = manifest.selection.levels.slice(0, 3);
    const path = join(directory, 'campaign.json');
    writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
    const plan = compileCampaignFile(path);
    assert.deepEqual(plan.definition.levels, [1, 2, 3]);
    assert(plan.attempts.every(attempt => attempt.dependencyPolicy === undefined));
    assert(plan.attempts.every(attempt =>
      attempt.featureCatalog.sha256 === plan.featureCatalog.identity.sha256));
    assert.deepEqual(plan.conditions[0].requested.levels.map(level => level.level), [1, 2, 3]);
    assert.deepEqual(plan.conditions[0].requested.levels
      .map(level => level.selection.scoredChecks.length), [11, 40, 61]);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test('dependency mode stops at the selected catalog prefix', () => {
  const directory = mkdtempSync(join(tmpdir(), 'stack-bench-dependency-prefix-'));
  try {
    const manifest = JSON.parse(readFileSync(campaignPath, 'utf8'));
    manifest.id = 'ecommerce-dependency-prefix-proof';
    manifest.levels = [1, 2, 3];
    manifest.selection.levels = manifest.selection.levels.slice(0, 3);
    const sourcePath = join(directory, 'campaign.json');
    writeFileSync(sourcePath, `${JSON.stringify(manifest, null, 2)}\n`);
    const plan = compileCampaignFile(sourcePath);
    assert.deepEqual(plan.dependencyPolicy.definition.levels, [1, 2, 3]);
    assert.deepEqual(Object.keys(plan.dependencyPolicy.definition.strikes.levels),
      ['1', '2', '3']);

    const planPath = join(directory, 'plan.json');
    writeArtifact(planPath, { kind: 'campaign_plan', id: `${plan.id}-plan`, payload: plan });
    const attempt = plan.attempts[0];
    const args = parseArgs(['node', ...attemptArgv(plan, attempt,
      join(directory, 'result'), 0, planPath)]);
    assert.deepEqual(args.levelList, [1, 2, 3]);
    assert.equal(Math.max(...args.progression.definition.nodes.map(node => node.level)), 3);

    let state = progressionEngine.initialize(args.progression.definition);
    const visited = [];
    while (state.phase === 'active') {
      const action = progressionEngine.nextAction(state);
      visited.push(action.level);
      const selection = progressionEngine.gradingSelection(state);
      state = progressionEngine.recordResult(state, {
        attemptId: `level-${action.level}`,
        outcome: 'conclusive',
        nodes: selection.nodeIds.map(id => ({ id,
          checks: selection.checks.filter(check => check.nodeId === id)
            .map(check => ({ id: check.id, outcome: 'pass' })) })),
      });
    }
    assert.deepEqual(visited, [1, 2, 3]);
    assert.equal(progressionEngine.nextAction(state).type, 'terminal');
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
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

test('every progression action input is exposed by every reference app', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 5, 'ecommerce.progression-catalog@1.0.0');
  const attributes = new Set();
  for (const execution of binding.plan.execution) {
    collectInputAttributes(JSON.parse(readFileSync(join(track.dir, execution.source), 'utf8')),
      attributes);
  }
  assert(attributes.size > 0);
  const missing = [];
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const text = sourceText(join(root, 'reference-apps', 'ecommerce', 'progression', backend));
    for (const attribute of attributes) {
      if (!text.includes(attribute)) missing.push(`${backend}:${attribute}`);
    }
  }
  assert.deepEqual(missing, []);
});

test('the model-free reference adapter can advance through progression levels', () => {
  const adapter = AGENT_ADAPTER_REGISTRY.get('reference-fixture');
  assert.deepEqual(adapter.modes, ['build', 'fix', 'upgrade']);
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

test('campaign admission sends the exact catalog, mode, and default build image to preflight', () => {
  const output = mkdtempSync(join(tmpdir(), 'stack-bench-progression-admission-'));
  try {
    const plan = compileCampaignFile(campaignPath);
    const calls = [];
    const admission = runCampaignAdmission(plan, output, {
      env: {}, now: '2026-08-25T00:00:00.000Z', uuid: () => 'test',
      preflight: request => {
        calls.push(request);
        return { schemaVersion: 1, generatedAt: '2026-08-25T00:00:00.000Z',
          request: { backends: request.backends, track: request.track,
            levels: request.levelList, runIndex: request.runIndex,
            parallelism: request.parallelism,
            agentAdapter: request.agentAdapter, packs: request.packIds,
            checks: request.checkKeys, image: request.image,
            resultsDir: request.resultsDir, smoke: request.smoke },
          ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] };
      },
    });
    assert.equal(admission.payload.ok, true);
    assert.equal(calls.length, 3);
    assert(calls.every(call => call.image === DEFAULT_BUILD_IMAGE));
    assert(calls.every(call => call.featureCatalog.identity.sha256
      === plan.featureCatalog.identity.sha256));
    assert(calls.every(call => call.mode.id === 'dependency'));
  } finally { rmSync(output, { recursive: true, force: true }); }
});
