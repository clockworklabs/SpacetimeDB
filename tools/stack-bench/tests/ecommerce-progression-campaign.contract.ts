import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { AGENT_ADAPTER_REGISTRY } from '../src/agents/agent-adapters.js';
import { compileCampaignFile, validateCompiledCampaignPlan }
  from '../src/campaigns/campaign-compiler.js';
import { runCampaignAdmission } from '../src/campaigns/campaign-admission.js';
import type { CampaignAdmissionPreflightRequest }
  from '../src/campaigns/campaign-admission.js';
import { attemptArgv } from '../src/campaigns/campaign-runner.js';
import { parseBenchArguments } from '../commands/bench-arguments.js';
import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.js';
import { writeArtifact } from '../src/evidence/artifacts.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';
import { parseReferenceAgentArgs } from '../src/references/reference-agent.js';
import { loadReferenceRegistry, selectReferenceFixture, validateReferenceRegistry }
  from '../src/references/reference-fixtures.js';

const root = STACK_BENCH_ROOT;
const campaignPath = join(root, 'appliance', 'campaign.ecommerce-progression-reference.json');

type UnknownRecord = Record<string, unknown>;
const record = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const first = <Value>(values: readonly Value[]): Value => {
  const value = values[0];
  assert(value);
  return value;
};

function collectInputAttributes(value: unknown, attributes = new Set<string>()): Set<string> {
  if (Array.isArray(value)) value.forEach(item => collectInputAttributes(item, attributes));
  else if (record(value)) {
    if (record(value.input) && typeof value.input.attribute === 'string') {
      attributes.add(value.input.attribute);
    }
    Object.values(value).forEach(item => collectInputAttributes(item, attributes));
  }
  return attributes;
}

function sourceText(directory: string): string {
  return readdirSync(directory, { recursive: true, withFileTypes: true })
    .filter(entry => entry.isFile() && /\.(?:js|jsx|ts|tsx|html)$/.test(entry.name))
    .map(entry => readFileSync(join(entry.parentPath, entry.name), 'utf8')).join('\n');
}

test('the ecommerce reference pilot resolves the exact L1-L6 progression inputs', () => {
  const plan = compileCampaignFile(campaignPath);
  assert(plan.featureCatalog);
  assert(plan.dependencyPolicy);
  assert.deepEqual(plan.definition.levels, [1, 2, 3, 4, 5, 6]);
  assert.equal(plan.featureCatalog.identity.id, 'ecommerce.questlines');
  assert.equal(plan.featureCatalog.identity.version, '2.0.2');
  assert.equal(plan.dependencyPolicy.identity.id, 'dependency-graph');
  assert.deepEqual(plan.bindings.map(binding => `${binding.recipe.id}@${binding.recipe.version}`),
    Array(6).fill('ecommerce.progression-catalog@2.0.2'));
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
    const featureCatalog = plan.featureCatalog;
    assert(featureCatalog);
    const condition = first(plan.conditions);
    assert.deepEqual(plan.definition.levels, [1, 2, 3]);
    assert(plan.attempts.every(attempt => attempt.dependencyPolicy === undefined));
    assert(plan.attempts.every(attempt => {
      assert(attempt.featureCatalog);
      return attempt.featureCatalog.sha256 === featureCatalog.identity.sha256;
    }));
    assert.deepEqual(condition.requested.levels.map(level => level.level), [1, 2, 3]);
    assert.deepEqual(condition.requested.levels.map(level => {
      assert(level.selection.scoredChecks);
      return level.selection.scoredChecks.length;
    }), [9, 41, 58]);
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
    assert(plan.dependencyPolicy);
    assert.deepEqual(plan.dependencyPolicy.definition.levels, [1, 2, 3]);
    assert.deepEqual(Object.keys(plan.dependencyPolicy.definition.strikes.levels),
      ['1', '2', '3']);

    const planPath = join(directory, 'plan.json');
    writeArtifact(planPath, { kind: 'campaign_plan', id: `${plan.id}-plan`, payload: plan });
    const attempt = first(plan.attempts);
    const args = parseBenchArguments(['node', ...attemptArgv(plan, attempt,
      join(directory, 'result'), 0, planPath)]);
    assert(args.progression);
    assert.deepEqual(args.levelList, [1, 2, 3]);
    assert.equal(Math.max(...args.progression.definition.nodes.map(node => node.level)), 3);

    let state = progressionEngine.initialize(args.progression.definition);
    const visited: number[] = [];
    while (state.phase === 'active') {
      const action = progressionEngine.nextAction(state);
      if (typeof action.level !== 'number') throw new Error('active progression has no level');
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

test('every campaign level resolves one current reference for each stack', () => {
  const registry = loadReferenceRegistry();
  assert.deepEqual(validateReferenceRegistry(registry), { ok: true, issues: [] });
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    for (let level = 1; level <= 5; level += 1) {
      const fixture = selectReferenceFixture(registry, {
        backend,
        track: 'ecommerce',
        level,
        recipe: 'ecommerce.progression-catalog@2.0.2',
      });
      assert.equal(fixture.status, 'candidate');
      assert.equal(fixture.targetPath, `reference-apps/ecommerce/${backend}`);
    }
  }
});

test('every progression action input is exposed by every reference app', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 5, 'ecommerce.progression-catalog@2.0.2');
  const attributes = new Set<string>();
  for (const execution of binding.plan.execution) {
    collectInputAttributes(JSON.parse(readFileSync(join(track.dir, execution.source), 'utf8')),
      attributes);
  }
  assert(attributes.size > 0);
  const missing: string[] = [];
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const text = sourceText(join(root, 'reference-apps', 'ecommerce', backend));
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
    'node', 'reference-agent.js',
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
    const featureCatalog = plan.featureCatalog;
    assert(featureCatalog);
    const calls: CampaignAdmissionPreflightRequest[] = [];
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
    assert(calls.every(call => {
      assert(call.featureCatalog);
      return call.featureCatalog.identity.sha256 === featureCatalog.identity.sha256;
    }));
    assert(calls.every(call => call.mode.id === 'dependency'));
  } finally { rmSync(output, { recursive: true, force: true }); }
});
