import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { loadTrack } from '../src/composition/tracks.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { createCheckEvidence } from '../src/evidence/check-evidence.mjs';
import { emptyArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.mjs';
import { progressionEngine } from '../dist/src/progression/progression-engine.js';
import { gradeBundleToProgressionResult } from '../src/progression/grade-bundle-result.mjs';
import { compileDependencyPolicyInput, compileFeatureCatalogInput,
  compileProgressionDefinitionFile, compileProgressionInput, dependencyRuntimeDefinition }
  from '../dist/src/progression/progression-definition.js';
import { auditProgressionReferenceRun }
  from '../src/progression/progression-reference-audit.mjs';
import { resolveProgressionRecipeAction }
  from '../dist/src/progression/progression-recipe-selection.js';
import { writeProgressionState } from '../dist/src/progression/progression-state.js';
import { STACK_BENCH_ROOT } from '../src/project-paths.mjs';

const owner = {
  schemaVersion: 1,
  campaign: { id: 'progression-reference', version: '1.0.0', sha256: 'e'.repeat(64) },
  attempt: { id: 'progression-reference-mongodb-r1', track: 'ecommerce', stack: 'mongodb',
    agentAdapter: 'reference-fixture', model: 'reference-fixture',
    conditionSha256: 'f'.repeat(64) },
  workspace: { appDirectory: 'source' },
};

const passedEvidence = () => createCheckEvidence({ status: 'passed', code: 'completed',
  phase: 'assertion', startedAtMs: 1, completedAtMs: 2 });
const setupEvidence = () => createCheckEvidence({ status: 'passed', code: 'completed',
  phase: 'setup', startedAtMs: 1, completedAtMs: 2 });

const track = loadTrack('ecommerce');
const featureCatalog = compileFeatureCatalogInput(compileProgressionDefinitionFile(
  join(STACK_BENCH_ROOT, 'tracks', 'ecommerce', 'progression', 'ecommerce-1.0.0.json'),
  { trackRoot: track.dir },
));
const dependencyPolicy = compileDependencyPolicyInput({ default: 3, levels: {} }, featureCatalog);
const progression = compileProgressionInput(dependencyRuntimeDefinition(
  featureCatalog, dependencyPolicy));
const recipeBindings = new Map([1, 2, 3, 4, 5].map(level => [level,
  resolveRecipeRelease(track, level, 'ecommerce.progression-catalog@1.0.0')]));
const release = recipeBindings.get(5).release;

function writeReferenceRun(root, { attempts = Infinity, tamperRecordedResult = false } = {}) {
  const runArtifact = writeArtifact(join(root, 'run.json'), {
    kind: 'benchmark_run',
    id: 'reference-run',
    attempt: { id: 'reference-run', parentId: owner.attempt.id },
    identities: emptyArtifactIdentities({
      agentAdapter: { id: owner.attempt.agentAdapter },
      stackAdapter: { id: owner.attempt.stack },
    }),
    payload: {
      track: owner.attempt.track,
      backend: owner.attempt.stack,
      model: owner.attempt.model,
      condition: { sha256: owner.attempt.conditionSha256 },
      featureCatalog: featureCatalog.identity,
      dependencyPolicy: dependencyPolicy.identity,
      progressionOwner: { schemaVersion: owner.schemaVersion,
        campaign: owner.campaign, attempt: owner.attempt },
    },
  });
  let state = progressionEngine.initialize(progression.definition);
  let sequence = 0;
  while (sequence < attempts) {
    const action = progressionEngine.nextAction(state);
    if (action.type === 'terminal') break;
    sequence += 1;
    const selected = resolveProgressionRecipeAction(recipeBindings.get(action.level), state);
    const checks = selected.grader.selection.scoredChecks;
    const keys = checks.map(check => check.stableKey);
    const max = checks.reduce((total, check) => total + check.points, 0);
    const sourceSha256 = String(sequence).repeat(64);
    const bundle = writeArtifact(join(root, 'progression',
      `attempt-${String(sequence).padStart(3, '0')}`, 'bundle.json'), {
      kind: 'grade_bundle',
      id: `grade-${sequence}`,
      attempt: { id: `grade-${sequence}`, parentId: runArtifact.id },
      identities: emptyArtifactIdentities({
        recipe: { id: release.id, version: release.version, sha256: release.contentSha256 },
        stackAdapter: { id: owner.attempt.stack },
      }),
      payload: {
        observation: 'scored',
        source: { sha256: sourceSha256 },
        selection: { sha256: selected.grader.selectionSha256, checks,
          attemptedChecks: keys, reportedChecks: keys, notRun: [] },
        totals: { score: max, max, regression: null },
        suites: { application: { features: [{ id: sequence, setupEvidence: setupEvidence(),
          criteria: checks.map(check => ({
            stableKey: check.stableKey, points: check.points, evidence: passedEvidence(),
          })) }] } },
      },
    });
    let result = gradeBundleToProgressionResult(bundle, action, {
      owner,
      runArtifact,
      featureCatalogIdentity: featureCatalog.identity,
      dependencyPolicyIdentity: dependencyPolicy.identity,
      selectionSha256: selected.grader.selectionSha256,
      sourceSha256,
      recipeIdentity: { id: release.id, version: release.version,
        sha256: release.contentSha256 },
      sequence,
    });
    if (tamperRecordedResult && sequence === 1) {
      result = { ...result, selectionSha256: '9'.repeat(64) };
    }
    state = progressionEngine.recordResult(state, result);
  }
  writeProgressionState(join(root, 'progression-state.json'), {
    progression,
    featureCatalogIdentity: featureCatalog.identity,
    dependencyPolicyIdentity: dependencyPolicy.identity,
    owner,
    state,
    id: 'progression-state',
  });
  return { progression, featureCatalogIdentity: featureCatalog.identity,
    dependencyPolicyIdentity: dependencyPolicy.identity, recipeBindings, release };
}

function audit(root, input) {
  return auditProgressionReferenceRun({ outputDir: root, owner, ...input });
}

test('progression reference audit replays every action and separates catalog coverage', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-reference-audit-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const input = writeReferenceRun(root);
  const report = audit(root, input);

  assert.equal(report.ok, true);
  assert.deepEqual(report.actions.map(action => [action.level, action.checks]),
    [[1, 11], [2, 51], [3, 112], [4, 135], [5, 146]]);
  assert.deepEqual(report.graphOwned, {
    nodes: 39, checks: 146, points: 281,
    coveredNodes: 39, coveredChecks: 146,
    missingNodes: [], missingChecks: [], complete: true,
  });
  assert.equal(report.finalCatalogAudit.required, true);
  assert.equal(report.finalCatalogAudit.status, 'not-run');
  assert.equal(report.finalCatalogAudit.checks, 151);
  assert.equal(report.finalCatalogAudit.points, 286);
  assert.equal(report.finalCatalogAudit.zeroPointChecks, 2);
  assert.equal(report.finalCatalogAudit.checkKeys.length, 151);
  assert.equal(report.finalCatalogAudit.additionalChecks.length, 5);
});

test('progression reference audit rejects a changed saved grade bundle', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-reference-tamper-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const input = writeReferenceRun(root);
  const path = join(root, 'progression', 'attempt-002', 'bundle.json');
  const bundle = JSON.parse(readFileSync(path, 'utf8'));
  bundle.payload.selection.sha256 = '8'.repeat(64);
  writeFileSync(path, `${JSON.stringify(bundle, null, 2)}\n`);

  assert.throws(() => audit(root, input), /selection identity does not match/);
});

test('progression reference audit rejects a recorded result that differs from evidence', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-reference-result-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const input = writeReferenceRun(root, { tamperRecordedResult: true });

  assert.throws(() => audit(root, input), /recorded result differs/);
});

test('progression reference audit reports incomplete graph coverage without hiding catalog scope', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-reference-partial-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const input = writeReferenceRun(root, { attempts: 1 });
  const report = audit(root, input);

  assert.equal(report.ok, false);
  assert.equal(report.actions.length, 1);
  assert.equal(report.graphOwned.coveredNodes, 4);
  assert.equal(report.graphOwned.coveredChecks, 11);
  assert.equal(report.graphOwned.complete, false);
  assert.equal(report.finalCatalogAudit.checks, 151);
  assert.equal(report.finalCatalogAudit.status, 'not-run');
});
