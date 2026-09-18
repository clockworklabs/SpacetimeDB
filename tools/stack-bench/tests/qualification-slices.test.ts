import assert from 'node:assert/strict';
import { join } from 'node:path';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { buildRecipeQualificationDocuments } from '../src/composition/recipe-release.js';
import { assertQualificationSliceCoverage, unchangedQualificationChecks,
  validateQualificationDocuments } from '../src/composition/qualification-slices.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { calibrationQualificationIdentity, calibrationQualificationRelease, compileCalibrationDefinition,
  validateQualificationSlice } from '../src/composition/calibration-compiler.js';
import type { CalibrationEvidence, CalibrationPlan } from '../src/composition/calibration-compiler.js';
import { qualificationScopeIdentity } from '../src/composition/qualification-scope.js';

const root = join(STACK_BENCH_ROOT, 'tracks/ecommerce');
const documents = validateQualificationDocuments(buildRecipeQualificationDocuments(
  join(root, 'composition/recipes/progression-catalog.json'), { trackRoot: root }));

test('saved definition proof binds the catalog and actual hash inputs', () => {
  assert.deepEqual(validateQualificationDocuments(documents), documents);
  for (const change of [
    (d: typeof documents) => { d.release.checkCatalog[0]!.points += 1; },
    (d: typeof documents) => { d.release.checkCatalog[0]!.executionId = 'wrong'; },
    (d: typeof documents) => { d.release.checkCatalog.push(d.release.checkCatalog[0]!); },
    (d: typeof documents) => { d.execution.runtime = {}; },
    (d: typeof documents) => { d.meaning.task = {}; },
  ]) {
    const changed = structuredClone(documents);
    change(changed);
    assert.throws(() => validateQualificationDocuments(changed), /qualification slice/);
  }
});

test('reuse invalidates the whole changed scenario and shared inputs', () => {
  const all = documents.release.checkCatalog.map(check => check.stableKey);
  assert.deepEqual([...unchangedQualificationChecks(documents, documents)], all);
  const changed = structuredClone(documents);
  const execution = changed.execution.execution as Array<Record<string, unknown>>;
  execution[0]!.checkGroups = [];
  const affected = documents.release.checkCatalog.filter(check => check.executionId === execution[0]!.id);
  const reused = unchangedQualificationChecks(documents, changed);
  assert.equal(reused.size, all.length - affected.length);
  assert(affected.every(check => !reused.has(check.stableKey)));
  for (const field of ['fixture', 'runtime', 'capabilities']) {
    const modified = structuredClone(documents);
    modified.execution[field] = {};
    assert.equal(unchangedQualificationChecks(documents, modified).size, 0, field);
  }
  const budget = structuredClone(documents);
  const packs = budget.execution.packs as Array<Record<string, unknown>>;
  packs[0]!.budget = {};
  const budgetReuse = unchangedQualificationChecks(documents, budget);
  assert.equal(budgetReuse.size, all.length - documents.release.checkCatalog
    .filter(check => check.packId === packs[0]!.id).length);
  const prompt = structuredClone(documents);
  prompt.meaning.task = {};
  assert.equal(unchangedQualificationChecks(documents, prompt).size, 0);
  const order = structuredClone(documents);
  (order.execution.execution as unknown[]).reverse();
  assert.equal(unchangedQualificationChecks(documents, order).size, 0);
});

test('slices require each check exactly once in every required evidence population', () => {
  const checks = documents.release.checkCatalog.slice(0, 2);
  const [a, b] = checks.map(check => check.stableKey);
  const first = { kind: 'reference', stack: 'postgres', repetition: 1, checks: [a!] };
  const second = { ...first, checks: [b!] };
  const required = ['reference:postgres:1'];
  assert.doesNotThrow(() => assertQualificationSliceCoverage([first, second], required, checks));
  assert.throws(() => assertQualificationSliceCoverage([first], required, checks), /missing coverage/);
  assert.throws(() => assertQualificationSliceCoverage([first, first, second], required, checks), /duplicate coverage/);
  assert.throws(() => assertQualificationSliceCoverage([{ ...first, checks: ['unknown'] }], required, checks), /unknown check/);
  assert.throws(() => assertQualificationSliceCoverage([first, second], [...required, 'mutation:postgres:1'], checks), /missing coverage/);
  assert.throws(() => assertQualificationSliceCoverage([{ ...first, repetition: 2 }], required, checks), /unexpected/);
});

test('saved slices validate real artifacts and reject incomplete or mismatched evidence', () => {
  const path = join(root, 'composition/calibrations/dependency-l3.json');
  const entry: CalibrationEvidence = { kind: 'mutation', stack: 'postgres', repetition: 1,
    path: 'qualification-evidence/ecommerce-l3-e804c1302/postgres-targeted.json',
    sha256: 'efc4a7f4df5f664fca9457c5746508f53db02b52f51a9ad5281c337a44c29bed',
    slice: { checks: ['ecommerce.progression.review-access-specifications.review-eligibility-direct.618a'],
      snapshot: { path: 'qualification-evidence/ecommerce-l3-e804c1302/current-inputs.json',
        sha256: '85f21bfeb1160f3889cb10bd3c3819a12dba46428d82e055e25af268f82b3f45' } } };
  const saved = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, entry.slice!.snapshot.path), 'utf8'));
  const savedDocuments = validateQualificationDocuments(saved.documents);
  const plan: CalibrationPlan = saved.calibration;
  const artifact = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, entry.path), 'utf8'));
  const executable = qualificationScopeIdentity({ kind: 'mutation', release: savedDocuments.release,
    stack: 'postgres', reference: plan.references.entries.find(item => item.backend === 'postgres'),
    mutation: plan.mutations.find(item => item.backend === 'postgres'), stackBenchRoot: STACK_BENCH_ROOT });
  // Simulate an explicit review only inside this test. This does not qualify the current release.
  plan.qualificationReuse = { rationale: 'test-only executable equivalence', evidence: [], scopes: [{
    kind: 'mutation', stack: 'postgres',
    fromExecutableSha256: artifact.payload.qualificationScope.executableSha256,
    toExecutableSha256: executable.executableSha256,
  }] };
  const selected = calibrationQualificationRelease(plan, savedDocuments.release, []);
  const context = { calibration: plan, qualificationIdentity: calibrationQualificationIdentity(plan),
    ...selected, stackBenchRoot: STACK_BENCH_ROOT, references: plan.references.entries,
    qualificationDocuments: savedDocuments };
  assert.doesNotThrow(() => validateQualificationSlice(artifact, entry, context));
  for (const change of [
    (a: typeof artifact) => { a.payload.runs[0].mutations.total -= 1; },
    (a: typeof artifact) => { a.payload.runs[0].ok = false; },
    (a: typeof artifact) => { a.payload.runner.platform = 'wrong'; },
    (a: typeof artifact) => { a.payload.qualifiedCheckKeys = []; },
    (a: typeof artifact) => { a.identities.recipe.sha256 = 'f'.repeat(64); },
    (a: typeof artifact) => { a.identities.fixture.sha256 = 'f'.repeat(64); },
    (a: typeof artifact) => { a.payload.qualificationScope.sha256 = 'f'.repeat(64); },
  ]) {
    const bad = structuredClone(artifact);
    change(bad);
    assert.throws(() => validateQualificationSlice(bad, entry, context));
  }
  const staleSnapshot = structuredClone(entry);
  staleSnapshot.slice!.snapshot.sha256 = 'f'.repeat(64);
  assert.throws(() => validateQualificationSlice(artifact, staleSnapshot, context), /stale digest/);
  assert.throws(() => validateQualificationSlice(artifact, entry,
    { ...context, calibration: { ...plan, qualificationReuse: undefined } }), /executable changed/);
  const wrongEquivalence = structuredClone(plan);
  wrongEquivalence.qualificationReuse!.scopes[0]!.toExecutableSha256 = 'f'.repeat(64);
  assert.throws(() => validateQualificationSlice(artifact, entry,
    { ...context, calibration: wrongEquivalence }), /executable changed/);
  const changedReference = structuredClone(plan);
  changedReference.references.entries[0]!.sourceSha256 = 'f'.repeat(64);
  assert.throws(() => validateQualificationSlice(artifact, entry,
    { ...context, calibration: changedReference }), /source references differs/);

  const stale: CalibrationEvidence = { ...entry, kind: 'reference',
    path: 'qualification-evidence/ecommerce-l3-7cd96d01b/postgres-reference.json',
    sha256: 'baaba1487f36fa51907c8ff4fc866997fc2625097e7fe333b3d2250801ed9baf',
    slice: { checks: entry.slice!.checks,
      snapshot: { path: 'qualification-evidence/ecommerce-l3-e804c1302/previous-inputs.json',
        sha256: 'ef95150fbc546ecbdc6a6e3c7a2ee0b4945dce127b9ad43fac3fc0971f3f01a5' } } };
  const previousArtifact = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, stale.path), 'utf8'));
  assert.throws(() => validateQualificationSlice(previousArtifact, stale, context), /changed check/);

  const manifest = JSON.parse(readFileSync(path, 'utf8'));
  manifest.qualification.evidence = [structuredClone(entry)];
  manifest.qualification.evidence[0].slice.checks = [];
  assert.throws(() => compileCalibrationDefinition(manifest), /non-empty/);
  manifest.qualification.evidence[0].slice.checks = ['duplicate', 'duplicate'];
  assert.throws(() => compileCalibrationDefinition(manifest), /duplicates checks/);
  manifest.qualification.evidence[0].slice.checks = ['one'];
  manifest.qualification.evidence[0].slice.untrusted = true;
  assert.throws(() => compileCalibrationDefinition(manifest), /unknown field/);
});
