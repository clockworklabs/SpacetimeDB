import assert from 'node:assert/strict';
import { join } from 'node:path';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { buildRecipeQualificationDocuments } from '../src/composition/recipe-release.js';
import { assertQualificationSliceCoverage, unchangedQualificationChecks,
  validateQualificationDocuments } from '../src/composition/qualification-slices.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { calibrationQualificationIdentity, calibrationQualificationRelease, compileCalibrationDefinition,
  compileCalibrationFile, validateQualificationSlice } from '../src/composition/calibration-compiler.js';
import { executionPlanForRelease } from '../src/composition/recipe-release.js';

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

test('registered slices validate real artifacts and reject incomplete or mismatched evidence', () => {
  const path = join(root, 'composition/calibrations/dependency-l3.json');
  const plan = compileCalibrationFile(path, { trackRoot: root, stackBenchRoot: STACK_BENCH_ROOT,
    release: documents.release });
  assert.deepEqual(plan.qualificationStaleness, []);
  const selected = calibrationQualificationRelease(plan, documents.release,
    executionPlanForRelease(join(root, plan.recipe.path), { trackRoot: root, level: 3 }));
  const context = { calibration: plan, qualificationIdentity: calibrationQualificationIdentity(plan),
    ...selected, stackBenchRoot: STACK_BENCH_ROOT, references: plan.references.entries };
  const entry = plan.qualification.evidence.find(item => item.kind === 'mutation'
    && item.stack === 'postgres' && item.slice?.checks.length === 1)!;
  assert(entry);
  const artifact = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, entry.path), 'utf8'));
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

  const previous = plan.qualification.evidence.find(item => item.kind === 'reference'
    && item.stack === 'postgres' && item.slice!.checks.length > 1)!;
  const stale = structuredClone(previous);
  stale.slice!.checks = entry.slice!.checks;
  const previousArtifact = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, previous.path), 'utf8'));
  assert.throws(() => validateQualificationSlice(previousArtifact, stale, context), /changed check/);

  const manifest = JSON.parse(readFileSync(path, 'utf8'));
  manifest.qualification.evidence[0].slice.checks = [];
  assert.throws(() => compileCalibrationDefinition(manifest), /non-empty/);
  manifest.qualification.evidence[0].slice.checks = ['duplicate', 'duplicate'];
  assert.throws(() => compileCalibrationDefinition(manifest), /duplicates checks/);
  manifest.qualification.evidence[0].slice.checks = ['one'];
  manifest.qualification.evidence[0].slice.untrusted = true;
  assert.throws(() => compileCalibrationDefinition(manifest), /unknown field/);
});
