import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { grantCampaignDependencyStrikes, prepareGrantWorkspace }
  from '../src/campaigns/campaign-progression-grant.js';
import { compileDependencyPolicyInput, compileFeatureCatalogInput }
  from '../src/progression/progression-definition.js';

interface TestContinuation {
  grantId: string;
  level: number;
  nodeIds: string[];
  strikes: number;
  stateSha256: string;
  resumeFrom: string;
  scheduledAt: string;
}

function campaignFixture({ marker = null, campaignStatus = 'completed' }:
  { marker?: TestContinuation | null; campaignStatus?: string } = {}) {
  const featureCatalog = compileFeatureCatalogInput({
    schemaVersion: 1,
    kind: 'feature-catalog',
    id: 'test.catalog',
    version: '1.0.0',
    state: 'draft',
    title: 'Test catalog',
    nodes: [{ id: 'accounts', title: 'Accounts', questline: 'identity',
      featureRefs: ['feature.accounts@1.0.0'], promptModules: [],
      gradingChecks: [{ id: 'accounts.create', points: 1, role: 'feature' }], dependencies: [] }],
    questlines: [{ id: 'identity', title: 'Identity', nodes: ['accounts'] }],
  });
  const dependencyPolicy = compileDependencyPolicyInput({ default: 1, levels: {} }, featureCatalog);
  const attemptPlan = {
    id: 'campaign-r1-c1-a1-postgres', mode: { id: 'dependency', version: '3.0.0' },
    stack: 'postgres', agentAdapter: 'claude-code', model: 'test-model',
    condition: { sha256: 'c'.repeat(64) },
  };
  const execution = {
    id: `${attemptPlan.id}-execution1`, status: 'completed',
    output: `attempts/${attemptPlan.id}/execution-1`,
    ...(marker === null ? {} : { continuation: marker }),
  };
  const plan = {
    id: 'campaign', version: '1.0.0', contentSha256: 'a'.repeat(64),
    definition: { mode: { id: 'dependency', version: '3.0.0' }, track: 'ecommerce' },
    featureCatalog, dependencyPolicy,
  };
  const state = { status: campaignStatus, attempts: [{ plan: attemptPlan,
    status: marker === null ? 'completed' : 'pending', executions: [execution] }] };
  return { plan, state, paths: { state: 'campaign-state.json' } };
}

const input = {
  attemptId: 'campaign-r1-c1-a1-postgres',
  grantId: 'operator-grant-1',
  level: 1,
  nodeIds: ['accounts'],
  strikes: 2,
};

test('a grant workspace copies only resumable evidence without changing its source', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-grant-'));
  try {
    const execution = join(root, 'attempts', input.attemptId, 'execution-1');
    mkdirSync(join(execution, 'source'), { recursive: true });
    mkdirSync(join(execution, 'progression', 'attempt-001'), { recursive: true });
    mkdirSync(join(execution, 'level-l1-source'), { recursive: true });
    writeFileSync(join(execution, 'run.json'), '{"status":"complete"}\n');
    writeFileSync(join(execution, 'progression-state.json'), '{}\n');
    writeFileSync(join(execution, 'level-l1-checkpoint.json'), '{}\n');
    writeFileSync(join(execution, 'source', 'app.js'), 'export const value = 1;\n');
    writeFileSync(join(execution, 'level-l1-source', 'app.js'), 'export const value = 1;\n');
    writeFileSync(join(execution, 'progression', 'attempt-001', 'bundle.json'), '{}\n');
    writeFileSync(join(execution, 'process.stdout.log'), 'not resumable\n');
    const prepared = prepareGrantWorkspace(root, execution, input.attemptId, input.grantId, 1);
    assert.equal(prepared.relativePath,
      `continuations/${input.attemptId}/${input.grantId}`);
    writeFileSync(join(prepared.directory, 'source', 'app.js'), 'export const value = 2;\n');
    assert.equal(readFileSync(join(execution, 'source', 'app.js'), 'utf8'),
      'export const value = 1;\n');
    assert.equal(readFileSync(join(prepared.directory, 'level-l1-source', 'app.js'), 'utf8'),
      'export const value = 1;\n');
    assert.equal(existsSync(join(prepared.directory, 'process.stdout.log')), false);
    assert.equal(prepareGrantWorkspace(root, execution, input.attemptId, input.grantId, 1).created,
      false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('campaign strike grant derives exact state, owner, checkpoint, and continuation marker', () => {
  const campaign = campaignFixture();
  const calls: {
    inspect: number;
    released: boolean;
    written: { path: string; plan: unknown; state: unknown } | null;
  } = { inspect: 0, released: false, written: null };
  const result = grantCampaignDependencyStrikes('campaign-output', input, {
    inspect: () => { calls.inspect += 1; return structuredClone(campaign); },
    acquire: (_root, plan) => { assert.equal(plan.contentSha256, 'a'.repeat(64)); return 'lock'; },
    release: lock => { assert.equal(lock, 'lock'); calls.released = true; },
    prepareWorkspace: () => ({
      directory: 'campaign-output/continuations/campaign-r1-c1-a1-postgres/operator-grant-1',
      relativePath: 'continuations/campaign-r1-c1-a1-postgres/operator-grant-1',
      created: true,
    }),
    readState: (path, options) => {
      assert.match(path, /operator-grant-1[\\/]progression-state\.json$/);
      assert.equal(options.owner.attempt.id, input.attemptId);
      assert.equal(options.owner.attempt.conditionSha256, 'c'.repeat(64));
      assert.equal(options.requireCurrentEngine, true);
      return { stateSha256: 'b'.repeat(64), state: { phase: 'terminal', grants: [] } };
    },
    grantState: (_path, options) => {
      assert.deepEqual(options.grant, {
        grantId: input.grantId, level: 1, nodeIds: ['accounts'], strikes: 2,
      });
      assert.deepEqual(options.checkpoint, { artifact: 'level-l1-checkpoint.json' });
      assert.equal(options.expectedStateSha256, 'b'.repeat(64));
      return { stateSha256: 'd'.repeat(64) };
    },
    schedule: (state, attemptId, marker, options) => {
      assert.equal(state.status, 'completed');
      assert.equal(attemptId, input.attemptId);
      assert.deepEqual(marker, { grantId: input.grantId, level: 1,
        nodeIds: ['accounts'], strikes: 2, stateSha256: 'd'.repeat(64),
        resumeFrom: 'continuations/campaign-r1-c1-a1-postgres/operator-grant-1' });
      assert.equal(options.now, '2026-08-28T12:00:00.000Z');
      return { status: 'prepared' };
    },
    writeState: (path, plan, state) => { calls.written = { path, plan, state }; },
    now: '2026-08-28T12:00:00.000Z',
  });
  assert.equal(calls.inspect, 2);
  assert.equal(calls.released, true);
  assert.ok(calls.written);
  assert.equal(calls.written.path, 'campaign-state.json');
  assert.equal(result.stateSha256, 'd'.repeat(64));
  assert.equal(result.scheduled, true);
});

test('campaign strike grant is idempotent only for the exact recorded marker', () => {
  const marker = { grantId: input.grantId, level: 1, nodeIds: ['accounts'], strikes: 2,
    stateSha256: 'd'.repeat(64),
    resumeFrom: 'continuations/campaign-r1-c1-a1-postgres/operator-grant-1',
    scheduledAt: '2026-08-28T12:00:00.000Z' };
  const campaign = campaignFixture({ marker, campaignStatus: 'prepared' });
  let touched = false;
  const result = grantCampaignDependencyStrikes('campaign-output', input, {
    inspect: () => structuredClone(campaign),
    acquire: () => 'lock', release: () => {},
    readState: () => ({ stateSha256: marker.stateSha256,
      state: { grants: [{ grantId: input.grantId, level: 1,
        nodeIds: ['accounts'], strikes: 2 }] } }),
    grantState: () => { touched = true; return { stateSha256: 'e'.repeat(64) }; },
    schedule: () => { touched = true; }, writeState: () => { touched = true; },
  });
  assert.equal(result.stateSha256, marker.stateSha256);
  assert.equal(touched, false);
  assert.throws(() => grantCampaignDependencyStrikes('campaign-output', {
    ...input, strikes: 3,
  }, { inspect: () => structuredClone(campaign), acquire: () => 'lock', release: () => {} }),
  /different continuation/);
});
