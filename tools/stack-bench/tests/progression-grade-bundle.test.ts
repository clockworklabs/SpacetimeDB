import assert from 'node:assert/strict';
import test from 'node:test';

import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import type { CheckEvidence, CheckEvidenceStatus } from '../src/evidence/check-evidence.js';
import { createArtifact, emptyArtifactIdentities } from '../src/evidence/artifacts.js';
import { gradeBundleToProgressionResult }
  from '../src/progression/grade-bundle-result.js';

interface BundleFixture {
  observation: string;
  source: { sha256: string };
  selection: {
    sha256: string;
    checks: Array<{ stableKey: string; points: number }>;
    attemptedChecks: string[];
    reportedChecks: string[];
    notRun: Array<{ stableKey: string; reason: string }>;
  };
  outcome?: { kind: string; reason: string; phase?: string };
  totals: { score: number; max: number; regression: { score: number; max: number } | null };
  suites: { application: { features: Array<{
    id: number;
    setupEvidence: CheckEvidence;
    criteria: Array<{ stableKey: string; points: number; evidence: CheckEvidence }>;
  }> } };
}

const evidence = (status: CheckEvidenceStatus): CheckEvidence => createCheckEvidence({
  status,
  code: status === 'passed' ? 'completed' : 'test_result',
  phase: 'assertion',
  startedAtMs: 1,
  completedAtMs: 2,
});
const setupEvidence = () => createCheckEvidence({ status: 'passed', code: 'completed',
  phase: 'setup', startedAtMs: 1, completedAtMs: 2 });
const sourceSha256 = 'c'.repeat(64);
const recipeIdentity = { id: 'ecommerce-l1', sha256: 'd'.repeat(64) };
const progressionIdentity = { id: 'dependency-graph', contentSha256: '8'.repeat(64) };
const featureCatalogIdentity = { id: 'catalog', contentSha256: '7'.repeat(64) };
const owner = { schemaVersion: 1,
  campaign: { id: 'campaign', version: '1.0.0', sha256: 'e'.repeat(64) },
  attempt: { id: 'campaign-r1', track: 'ecommerce', stack: 'postgres',
    agentAdapter: 'claude-code', model: 'test-model', conditionSha256: 'f'.repeat(64) },
  workspace: { appDirectory: 'app' } };
const runArtifact = () => createArtifact({ kind: 'benchmark_run', id: 'run-1',
  attempt: { id: 'run-1', parentId: owner.attempt.id },
  identities: emptyArtifactIdentities({ agentAdapter: { id: owner.attempt.agentAdapter },
    stackAdapter: { id: owner.attempt.stack } }),
  payload: { backend: owner.attempt.stack, model: owner.attempt.model,
    condition: { contentSha256: owner.attempt.conditionSha256 },
    featureCatalog: featureCatalogIdentity, dependencyPolicy: progressionIdentity,
    progressionOwner: { schemaVersion: owner.schemaVersion,
      campaign: owner.campaign, attempt: owner.attempt } },
});

const action = () => ({
  type: 'repair',
  prompt: { nodeIds: ['accounts', 'catalog'] },
  grading: {
    nodeIds: ['accounts', 'catalog'],
    checks: [
      { id: 'check.accounts', nodeId: 'accounts', points: 1 },
      { id: 'check.catalog', nodeId: 'catalog', points: 2 },
    ],
  },
});

const bundle = (): BundleFixture => ({
  observation: 'scored',
  source: { sha256: sourceSha256 },
  selection: {
    sha256: 'a'.repeat(64),
    checks: [
      { stableKey: 'check.accounts', points: 1 },
      { stableKey: 'check.catalog', points: 2 },
    ],
    attemptedChecks: ['check.accounts', 'check.catalog'],
    reportedChecks: ['check.accounts', 'check.catalog'],
    notRun: [],
  },
  totals: { score: 1, max: 3, regression: null },
  suites: { application: { features: [{ id: 1, setupEvidence: setupEvidence(), criteria: [
    { stableKey: 'check.accounts', points: 1, evidence: evidence('passed') },
    { stableKey: 'check.catalog', points: 2, evidence: evidence('failed') },
  ] }] } },
});

const artifact = (payload = bundle(), id = 'grade-1') => createArtifact({
  kind: 'grade_bundle', id,
  attempt: { id, parentId: 'run-1' },
  identities: emptyArtifactIdentities({ recipe: recipeIdentity,
    stackAdapter: { id: owner.attempt.stack } }),
  payload,
});

const conversion = { owner, runArtifact: runArtifact(), featureCatalogIdentity,
  dependencyPolicyIdentity: progressionIdentity,
  sourceSha256, recipeIdentity,
  selectionSha256: 'a'.repeat(64) };

test('grade bundle conversion preserves exact node ownership and measured outcomes', () => {
  assert.deepEqual(gradeBundleToProgressionResult(artifact(), action(), conversion), {
    attemptId: 'grade-1',
    runId: 'run-1',
    sourceSha256,
    selectionSha256: 'a'.repeat(64),
    outcome: 'conclusive',
    nodes: [
      { id: 'accounts', checks: [{ id: 'check.accounts', outcome: 'pass' }] },
      { id: 'catalog', checks: [{ id: 'check.catalog', outcome: 'fail' }] },
    ],
  });
});

test('grade bundle conversion rejects incomplete, duplicate, or changed grading scope', () => {
  const missing = bundle();
  missing.selection.reportedChecks.pop();
  assert.throws(() => gradeBundleToProgressionResult(artifact(missing, 'missing'),
    action(), conversion), /complete progression evidence/);
  const duplicate = bundle();
  duplicate.suites.application.features[0]!.criteria.push(
    structuredClone(duplicate.suites.application.features[0]!.criteria[0]!));
  assert.throws(() => gradeBundleToProgressionResult(artifact(duplicate, 'duplicate'),
    action(), conversion), /repeats check/);
  const points = bundle();
  points.selection.checks[0]!.points = 2;
  assert.throws(() => gradeBundleToProgressionResult(artifact(points, 'points'),
    action(), conversion), /points.*do not match/);
  assert.throws(() => gradeBundleToProgressionResult(artifact(), action(), {
    ...conversion, selectionSha256: 'b'.repeat(64),
  }), /selection identity/);
  assert.throws(() => gradeBundleToProgressionResult(artifact(), action(), {
    ...conversion, selectionSha256: undefined,
  }), /selection identity is required/);
  const totals = bundle();
  totals.totals.score = 2;
  assert.throws(() => gradeBundleToProgressionResult(artifact(totals, 'totals'),
    action(), conversion), /totals do not match/);
});

test('grade bundle conversion binds the artifact owner, source, stack, recipe, and nodes', () => {
  const stale = artifact();
  stale.attempt.parentId = 'different-run';
  assert.throws(() => gradeBundleToProgressionResult(stale, action(), conversion),
    /does not belong/);
  assert.throws(() => gradeBundleToProgressionResult(artifact(), action(), {
    ...conversion, sourceSha256: '9'.repeat(64),
  }), /source identity/);
  const foreignRun = runArtifact();
  foreignRun.attempt.parentId = 'different-attempt';
  assert.throws(() => gradeBundleToProgressionResult(artifact(), action(), {
    ...conversion, runArtifact: foreignRun,
  }), /owned progression benchmark run/);
  const oldCampaign = runArtifact();
  oldCampaign.payload.progressionOwner.campaign.sha256 = '7'.repeat(64);
  assert.throws(() => gradeBundleToProgressionResult(artifact(), action(), {
    ...conversion, runArtifact: oldCampaign,
  }), /benchmark owner/);
  const badNodes = action();
  badNodes.grading.nodeIds.push('accounts');
  assert.throws(() => gradeBundleToProgressionResult(artifact(), badNodes, conversion),
    /node ownership/);
  const foreign = action();
  foreign.grading.checks[0]!.nodeId = 'foreign';
  assert.throws(() => gradeBundleToProgressionResult(artifact(), foreign, conversion),
    /node ownership/);
});

test('typed grader failures do not consume a progression repair as a zero score', () => {
  const failed = bundle();
  failed.outcome = { kind: 'harness_failure', reason: 'browser worker stopped' };
  assert.deepEqual(gradeBundleToProgressionResult(artifact(failed, 'harness'),
    action(), conversion), { attemptId: 'harness', runId: 'run-1', sourceSha256,
    selectionSha256: 'a'.repeat(64),
    outcome: 'inconclusive', category: 'harness_failure',
    reason: 'browser worker stopped' });
});

test('one unmeasured check makes the grading attempt inconclusive', () => {
  const mixed = bundle();
  mixed.outcome = { kind: 'app_failure', phase: 'grading', reason: 'catalog failed' };
  mixed.suites.application.features[0]!.criteria[0]!.evidence = evidence('inconclusive');
  mixed.suites.application.features[0]!.criteria[1]!.evidence = evidence('failed');
  mixed.totals.score = 0;
  assert.deepEqual(gradeBundleToProgressionResult(artifact(mixed, 'mixed'),
    action(), conversion), {
    attemptId: 'mixed', runId: 'run-1', sourceSha256,
    selectionSha256: 'a'.repeat(64), outcome: 'inconclusive',
    category: 'inconclusive_evidence',
    reason: '1 selected check did not produce measured evidence',
  });
});

test('completed progression evidence outranks a stale application failure', () => {
  const completed = bundle();
  completed.outcome = { kind: 'app_failure', phase: 'grading', reason: 'stale summary' };
  const result = gradeBundleToProgressionResult(artifact(completed, 'completed'),
    action(), conversion);
  assert.equal(result.outcome, 'conclusive');
  if (result.outcome !== 'conclusive') throw new Error('expected a conclusive result');
  assert.equal(result.applicationFailure, undefined);
  assert.deepEqual(result.nodes, [
    { id: 'accounts', checks: [{ id: 'check.accounts', outcome: 'pass' }] },
    { id: 'catalog', checks: [{ id: 'check.catalog', outcome: 'fail' }] },
  ]);
});

test('a typed application abort charges current work but not earlier regression guards', () => {
  const failed = bundle();
  failed.outcome = { kind: 'app_failure', phase: 'application-start',
    reason: 'the generated application did not start' };
  failed.selection.attemptedChecks = [];
  failed.selection.reportedChecks = [];
  failed.selection.notRun = failed.selection.checks.map(check => ({
    stableKey: check.stableKey, reason: failed.outcome!.reason,
  }));
  failed.totals = { score: 0, max: 3, regression: null };
  const selected = action();
  selected.prompt.nodeIds = ['catalog'];
  const result = gradeBundleToProgressionResult(artifact(failed, 'app-failure'),
    selected, conversion);
  assert.equal(result.outcome, 'conclusive');
  if (result.outcome !== 'conclusive') throw new Error('expected a conclusive result');
  assert.deepEqual(result.applicationFailure, {
    phase: 'application-start', reason: 'the generated application did not start',
  });
  assert.deepEqual(result.nodes, [
    { id: 'accounts', checks: [{ id: 'check.accounts', outcome: 'not-run' }] },
    { id: 'catalog', checks: [{ id: 'check.catalog', outcome: 'fail' }] },
  ]);
  const incomplete = structuredClone(failed);
  incomplete.selection.notRun.pop();
  assert.throws(() => gradeBundleToProgressionResult(artifact(incomplete, 'bad-abort'),
    action(), conversion), /application abort is incomplete/);
});
