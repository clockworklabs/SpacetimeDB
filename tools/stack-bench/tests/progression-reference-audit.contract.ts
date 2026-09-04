import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { loadTrack } from '../src/composition/tracks.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import { emptyArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import { gradeBundleToProgressionResult } from '../src/progression/grade-bundle-result.js';
import { compileDependencyPolicyInput, compileFeatureCatalogInput,
  compileProgressionDefinitionFile, compileProgressionInput, dependencyRuntimeDefinition }
  from '../src/progression/progression-definition.js';
import { auditProgressionReferenceRun }
  from '../src/progression/progression-reference-audit.js';
import { resolveProgressionRecipeAction }
  from '../src/progression/progression-recipe-selection.js';
import { writeProgressionState } from '../src/progression/progression-state.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

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
  join(STACK_BENCH_ROOT, 'tracks', 'ecommerce', 'progression', 'ecommerce.json'),
  { trackRoot: track.dir },
));
const dependencyPolicy = compileDependencyPolicyInput(
  { selection: 'feature', budget: { perFeature: 2 } }, featureCatalog);
const progression = compileProgressionInput(dependencyRuntimeDefinition(
  featureCatalog, dependencyPolicy));
const recipeBindings = new Map([1, 2, 3, 4, 5, 6].map(level => [level,
  resolveRecipeRelease(track, level, 'ecommerce.progression-catalog')]));
const release = recipeBindings.get(6)!.release;

interface ReferenceRunOptions {
  attempts?: number;
  tamperRecordedResult?: boolean;
}

function writeReferenceRun(root: string,
  { attempts = Infinity, tamperRecordedResult = false }: ReferenceRunOptions = {}) {
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
      condition: { contentSha256: owner.attempt.conditionSha256 },
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
    const binding = recipeBindings.get(action.level);
    if (!binding) throw new Error(`missing recipe binding for L${action.level}`);
    const selected = resolveProgressionRecipeAction(binding, state);
    if (!('grader' in selected)) throw new Error('expected active progression recipe work');
    const checks = selected.grader.selection.scoredChecks;
    const keys = checks.map(check => check.stableKey);
    const max = checks.reduce((total, check) => total + check.points, 0);
    const sourceSha256 = String(sequence).padStart(64, '0');
    const bundle = writeArtifact(join(root, 'progression',
      `attempt-${String(sequence).padStart(3, '0')}`, 'bundle.json'), {
      kind: 'grade_bundle',
      id: `grade-${sequence}`,
      attempt: { id: `grade-${sequence}`, parentId: runArtifact.id },
      identities: emptyArtifactIdentities({
        recipe: { id: release.id, sha256: release.contentSha256 },
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
      recipeIdentity: { id: release.id, sha256: release.contentSha256 },
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

function audit(root: string, input: ReturnType<typeof writeReferenceRun>) {
  return auditProgressionReferenceRun({ outputDir: root, owner, ...input });
}

test('progression reference audit replays every action and separates catalog coverage', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-reference-audit-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const input = writeReferenceRun(root);
  const report = audit(root, input);

  assert.equal(report.ok, false);
  assert.deepEqual(report.actions.map(action => [action.level, action.checks]),
    [[1, 9], [2, 50], [3, 108], [4, 142], [5, 155], [6, 157]]);
  assert.deepEqual(report.graphOwned, {
    nodes: 43, checks: 157, points: 294,
    coveredNodes: 43, coveredChecks: 157,
    missingNodes: [], missingChecks: [], complete: true,
  });
  assert.equal(report.finalCatalogAudit.required, true);
  assert.equal(report.finalCatalogAudit.status, 'not-run');
  assert.equal(report.finalCatalogAudit.checks, 159);
  assert.equal(report.finalCatalogAudit.points, 294);
  assert.equal(report.finalCatalogAudit.zeroPointChecks, 2);
  assert.equal(report.finalCatalogAudit.checkKeys.length, 159);
  assert.equal(report.finalCatalogAudit.additionalChecks.length, 2);
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
  assert.equal(report.graphOwned.coveredChecks, 9);
  assert.equal(report.graphOwned.complete, false);
  assert.equal(report.finalCatalogAudit.checks, 159);
  assert.equal(report.finalCatalogAudit.status, 'not-run');
});
