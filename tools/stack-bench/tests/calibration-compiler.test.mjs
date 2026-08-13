import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { calibrationQualificationIdentity, compileCalibrationDefinition, compileCalibrationFile,
  resolveCalibrationForRelease, validateQualificationEvidenceArtifact } from '../calibration-compiler.mjs';
import { readArtifact } from '../artifacts.mjs';
import { checkCalibrations } from '../check-calibration.mjs';
import { resolveLegacyRecipeRelease } from '../recipe-release.mjs';
import { loadTrack } from '../tracks.mjs';

const ROOT = join(import.meta.dirname, '..');
const TRACK = loadTrack('ecommerce');
const CALIBRATION = join(TRACK.dir, 'composition', 'calibrations', 'l1-standard-1.0.0.json');

function current() {
  const binding = resolveLegacyRecipeRelease(TRACK, 1);
  return { binding, plan: compileCalibrationFile(CALIBRATION,
    { trackRoot: TRACK.dir, stackBenchRoot: ROOT, release: binding.release }) };
}

test('runtime calibration resolution is exact and returns null for an uncalibrated recipe', () => {
  const l1 = resolveLegacyRecipeRelease(TRACK, 1).release;
  const resolved = resolveCalibrationForRelease(l1, { trackRoot: TRACK.dir, stackBenchRoot: ROOT });
  assert.equal(resolved.id, 'ecommerce.l1-standard-calibration');
  assert.match(resolved.contentSha256, /^[a-f0-9]{64}$/);
  const l2 = resolveLegacyRecipeRelease(TRACK, 2).release;
  assert.equal(resolveCalibrationForRelease(l2, { trackRoot: TRACK.dir, stackBenchRoot: ROOT }), null);
});

function temporaryCalibration(change) {
  const directory = mkdtempSync(join(TRACK.dir, 'composition', 'calibrations', '.test-'));
  const path = join(directory, 'calibration.json');
  const value = JSON.parse(readFileSync(CALIBRATION, 'utf8'));
  // The source moved one directory deeper for this isolated copy.
  value.recipe.path = '../../recipes/l1-standard-1.0.0.json';
  change(value);
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
  return { directory, path };
}

function compileChanged(change) {
  const temporary = temporaryCalibration(change);
  try {
    const release = resolveLegacyRecipeRelease(TRACK, 1).release;
    return compileCalibrationFile(temporary.path,
      { trackRoot: TRACK.dir, stackBenchRoot: ROOT, release });
  } finally { rmSync(temporary.directory, { recursive: true, force: true }); }
}

test('the current L1 calibration deterministically binds recipe, fixture, references, mutations, and controls', () => {
  const first = current().plan;
  const second = current().plan;
  assert.deepEqual(first, second);
  assert.equal(first.state, 'draft');
  assert.equal(first.recipe.contentSha256, current().binding.release.contentSha256);
  assert.equal(first.fixture.sourceSha256, current().binding.release.components.fixture.sha256);
  assert.equal(first.references.entries.length, 3);
  assert.equal(first.mutations.length, 3);
  assert.equal(first.controls.length, 9);
  assert.equal(first.controls.filter(control => control.role === 'promotion-gate').length, 2);
  assert.equal(new Set(first.controls.map(control => control.stableKey)).size, 9);
  assert.match(first.contentSha256, /^[a-f0-9]{64}$/);
  assert.match(first.qualificationSha256, /^[a-f0-9]{64}$/);
  assert.equal(calibrationQualificationIdentity(first).sha256, first.qualificationSha256);
  assert.deepEqual(checkCalibrations({ trackName: 'ecommerce' }).map(result => result.id),
    ['ecommerce.l1-standard-calibration']);
});

test('qualification evidence is semantically bound and tampering fails closed', () => {
  const { binding, plan } = current();
  const context = {
    calibration: plan,
    qualificationIdentity: calibrationQualificationIdentity(plan),
    release: binding.release,
    references: plan.references.entries,
  };
  const referenceEntry = plan.qualification.evidence.find(entry =>
    entry.kind === 'reference' && entry.stack === 'mongodb' && entry.repetition === 1);
  const reference = readArtifact(join(ROOT, referenceEntry.path));
  assert.doesNotThrow(() => validateQualificationEvidenceArtifact(reference, referenceEntry, context));

  const wrongStack = structuredClone(reference);
  wrongStack.identities.stackAdapter.id = 'postgres';
  assert.throws(() => validateQualificationEvidenceArtifact(wrongStack, referenceEntry, context),
    /wrong stack adapter/);

  const hiddenFailure = structuredClone(reference);
  hiddenFailure.payload.runs[0].ok = false;
  assert.throws(() => validateQualificationEvidenceArtifact(hiddenFailure, referenceEntry, context),
    /failed or incomplete repetition/);

  const nullEntry = plan.qualification.evidence.find(entry => entry.kind === 'null');
  const nullArtifact = readArtifact(join(ROOT, nullEntry.path));
  const vacuous = structuredClone(nullArtifact);
  vacuous.payload.summary.vacuousPasses.criteria = 1;
  assert.throws(() => validateQualificationEvidenceArtifact(vacuous, nullEntry, context),
    /complete null policy/);
});

test('qualification identity excludes governance transitions but binds executable controls', () => {
  const plan = current().plan;
  const governance = structuredClone(plan);
  governance.state = 'qualified';
  governance.promotion.status = 'promoted';
  governance.promotion.catalogSha256 = '0'.repeat(64);
  governance.qualification.evidence = [{ kind: 'reference', stack: 'mongodb', repetition: 1,
    path: 'results/evidence.json', sha256: '1'.repeat(64) }];
  governance.qualification.stacks.forEach(stack => { stack.status = 'qualified'; });
  governance.references.registrySha256 = '2'.repeat(64);
  governance.references.entries.forEach(entry => { entry.status = 'active'; });
  governance.mutations.forEach(entry => { entry.status = 'active'; entry.sha256 = '3'.repeat(64); });
  assert.equal(calibrationQualificationIdentity(governance).sha256, plan.qualificationSha256);

  const changed = structuredClone(plan);
  changed.nullControl.repetitions += 1;
  assert.notEqual(calibrationQualificationIdentity(changed).sha256, plan.qualificationSha256);
});

test('set-like calibration source ordering does not change its identity', () => {
  const before = current().plan;
  const after = compileChanged(value => {
    value.references.entries.reverse();
    value.mutations.reverse();
    value.controls.reverse();
    value.qualification.stacks.reverse();
  });
  assert.equal(after.contentSha256, before.contentSha256);
});

test('stale recipe, fixture, reference, mutation, and promotion hashes fail compilation', () => {
  for (const change of [
    value => { value.recipe.contentSha256 = '0'.repeat(64); },
    value => { value.fixture.sourceSha256 = '0'.repeat(64); },
    value => { value.references.registrySha256 = '0'.repeat(64); },
    value => { value.mutations[0].sha256 = '0'.repeat(64); },
    value => { value.promotion.catalogSha256 = '0'.repeat(64); },
  ]) assert.throws(() => compileChanged(change), /does not match|stale/);
});

test('every zero-point check requires one typed policy with valid mutation targets', () => {
  assert.throws(() => compileChanged(value => { value.controls.pop(); }), /missing zero-point checks/);
  assert.throws(() => compileChanged(value => {
    value.controls[0].mutationTargets = ['postgres:not-a-mutant'];
  }), /unknown mutation/);
  assert.throws(() => compileChanged(value => {
    value.controls[0].promotionPolicy = 'record-only-until-calibrated';
  }), /must be must-pass-reference-and-kill-declared-mutant/);
  assert.throws(() => compileChanged(value => {
    value.controls[0].stableKey = value.controls[1].stableKey;
  }), /duplicates|not an unassigned/);
});

test('draft component evidence cannot be mislabeled as a qualified calibration or promoted alias', () => {
  assert.throws(() => compileChanged(value => { value.state = 'qualified'; }),
    /cannot qualify a draft or retired recipe/);
  assert.throws(() => compileChanged(value => { value.promotion.status = 'promoted'; }),
    /does not resolve|requires a qualified calibration/);
});

test('equivalence decisions require two distinct executions and hash-bound evidence', () => {
  const definition = JSON.parse(readFileSync(CALIBRATION, 'utf8'));
  definition.equivalenceDecisions = [{
    fromExecutionSha256: definition.recipe.executionSha256,
    toExecutionSha256: definition.recipe.executionSha256,
    rationale: 'No change',
    evidence: [{ path: 'evidence.json', sha256: '0'.repeat(64) }],
  }];
  assert.throws(() => compileCalibrationDefinition(definition), /must compare different execution hashes/);
  assert.throws(() => compileChanged(value => {
    value.qualification.evidence = [];
    value.equivalenceDecisions = [{
      fromExecutionSha256: '1'.repeat(64),
      toExecutionSha256: '2'.repeat(64),
      rationale: 'Calibrated execution-only correction',
      evidence: [{ path: 'missing-evidence.json', sha256: '3'.repeat(64) }],
    }];
  }), /does not exist/);
});
