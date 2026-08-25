import assert from 'node:assert/strict';
import test from 'node:test';

import { createCheckEvidence } from '../src/evidence/check-evidence.mjs';
import { createArtifact, emptyArtifactIdentities } from '../src/evidence/artifacts.mjs';
import { gradeBundleToProgressionResult }
  from '../src/progression/grade-bundle-result.mjs';

const evidence = status => createCheckEvidence({
  status,
  code: status === 'passed' ? 'completed' : 'test_result',
  phase: 'assertion',
  startedAtMs: 1,
  completedAtMs: 2,
});
const setupEvidence = () => createCheckEvidence({ status: 'passed', code: 'completed',
  phase: 'setup', startedAtMs: 1, completedAtMs: 2 });
const sourceSha256 = 'c'.repeat(64);
const recipeIdentity = { id: 'ecommerce-l1', version: '1.0.0', sha256: 'd'.repeat(64) };
const progressionIdentity = { id: 'dependency', version: '1.0.0',
  policy: 'dependency-gated', sha256: '8'.repeat(64) };
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
    condition: { sha256: owner.attempt.conditionSha256 }, progression: progressionIdentity,
    progressionOwner: { schemaVersion: owner.schemaVersion,
      campaign: owner.campaign, attempt: owner.attempt } },
});

const action = () => ({
  type: 'repair',
  grading: {
    nodeIds: ['accounts', 'catalog'],
    checks: [
      { id: 'check.accounts', nodeId: 'accounts', points: 1 },
      { id: 'check.catalog', nodeId: 'catalog', points: 2 },
    ],
  },
});

const bundle = () => ({
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

const conversion = { owner, runArtifact: runArtifact(), progressionIdentity,
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
  duplicate.suites.application.features[0].criteria.push(
    structuredClone(duplicate.suites.application.features[0].criteria[0]));
  assert.throws(() => gradeBundleToProgressionResult(artifact(duplicate, 'duplicate'),
    action(), conversion), /repeats check/);
  const points = bundle();
  points.selection.checks[0].points = 2;
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
  foreign.grading.checks[0].nodeId = 'foreign';
  assert.throws(() => gradeBundleToProgressionResult(artifact(), foreign, conversion),
    /node ownership/);
});

test('typed grader failures do not consume a progression strike as a zero score', () => {
  const failed = bundle();
  failed.outcome = { kind: 'harness_failure', reason: 'browser worker stopped' };
  assert.deepEqual(gradeBundleToProgressionResult(artifact(failed, 'harness'),
    action(), conversion), { attemptId: 'harness', runId: 'run-1', sourceSha256,
    selectionSha256: 'a'.repeat(64),
    outcome: 'inconclusive', category: 'harness_failure',
    reason: 'browser worker stopped' });
});

test('a typed application abort fails the selected checks instead of becoming inconclusive', () => {
  const failed = bundle();
  failed.outcome = { kind: 'app_failure', phase: 'application-start',
    reason: 'the generated application did not start' };
  failed.selection.attemptedChecks = [];
  failed.selection.reportedChecks = [];
  failed.selection.notRun = failed.selection.checks.map(check => ({
    stableKey: check.stableKey, reason: failed.outcome.reason,
  }));
  failed.totals = { score: 0, max: 3, regression: null };
  const result = gradeBundleToProgressionResult(artifact(failed, 'app-failure'),
    action(), conversion);
  assert(result.nodes.every(node => node.checks.every(check => check.outcome === 'fail')));
  const incomplete = structuredClone(failed);
  incomplete.selection.notRun.pop();
  assert.throws(() => gradeBundleToProgressionResult(artifact(incomplete, 'bad-abort'),
    action(), conversion), /application abort is incomplete/);
});
