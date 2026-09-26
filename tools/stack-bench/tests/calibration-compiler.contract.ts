import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { calibrationQualificationIdentity, calibrationQualificationRelease, compileCalibrationDefinition,
  compileCalibrationFile, currentLevelPoints, hasExactSelectedPackRuntime,
  resolveCalibrationForRelease, validateQualificationEvidenceArtifact } from '../src/composition/calibration-compiler.js';
import { createArtifact } from '../src/evidence/artifacts.js';
import { buildRecipeRelease, executionPlanForRelease,
  requireRecipeRelease } from '../src/composition/recipe-release.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { loadTrack } from '../src/composition/tracks.js';
import { loadReferenceRegistry, validateReferenceRegistry } from '../src/references/reference-fixtures.js';

const TRACK = loadTrack('ecommerce');
const CALIBRATIONS = join(TRACK.dir, 'composition', 'calibrations');
const CALIBRATION = join(CALIBRATIONS, 'dependency-l3.json');
const BACKENDS = ['mongodb', 'postgres', 'spacetime'];
const PRECONDITION_CONTROLS = [
  ['ecommerce.spec.concurrency-safety.restock-race.202-control', 'precondition'],
  ['ecommerce.spec.external-data-sync.external-stock.901b', 'precondition'],
];
const EXPECTED_CALIBRATIONS: Record<string, { id: string; recipe: string; stacks: string[];
  controls: string[][] }> = {
  'dependency-l3.json': { id: 'ecommerce.dependency-l3-calibration', recipe: 'progression-catalog',
    stacks: ['convex', ...BACKENDS], controls: [] },
  'dependency-l6.json': { id: 'ecommerce.l6-return-refund-calibration', recipe: 'progression-catalog',
    stacks: BACKENDS, controls: [] },
  'sequential-l1.json': { id: 'ecommerce.sequential-l1-calibration', recipe: 'sequential-l1',
    stacks: BACKENDS, controls: PRECONDITION_CONTROLS },
  'sequential-l2.json': { id: 'ecommerce.sequential-l2-calibration', recipe: 'sequential-l2',
    stacks: BACKENDS, controls: PRECONDITION_CONTROLS },
};

function calibrationSource(): unknown {
  return JSON.parse(readFileSync(CALIBRATION, 'utf8'));
}

function current() {
  const binding = requireRecipeRelease(TRACK, 3, 'ecommerce.progression-catalog');
  const plan = compileCalibrationFile(CALIBRATION, {
    trackRoot: TRACK.dir,
    stackBenchRoot: STACK_BENCH_ROOT,
    release: binding.release,
  });
  return { ...calibrationQualificationRelease(plan, binding.release, binding.execution), plan };
}

test('qualification runtime must cover the exact selected pack set', () => {
  const release = { checkCatalog: [{ packId: 'accounts' }, { packId: 'accounts' },
    { packId: 'orders' }] };
  assert.equal(hasExactSelectedPackRuntime({ packs: [
    { id: 'accounts', exceeded: false }, { id: 'orders', exceeded: false },
  ] }, release), true);
  assert.equal(hasExactSelectedPackRuntime({ packs: [
    { id: 'accounts', exceeded: false }, { id: 'future', exceeded: false },
  ] }, release), false);
});

test('every current calibration binds stable authored identities', () => {
  const registry = loadReferenceRegistry();
  assert.deepEqual(validateReferenceRegistry(registry).issues, []);
  assert.deepEqual(readdirSync(CALIBRATIONS).filter(name => name.endsWith('.json')).sort(),
    Object.keys(EXPECTED_CALIBRATIONS).sort());
  for (const [name, expected] of Object.entries(EXPECTED_CALIBRATIONS)) {
    const path = join(CALIBRATIONS, name);
    const { selection } = JSON.parse(readFileSync(path, 'utf8')) as { selection: { alias: string } };
    const binding = requireRecipeRelease(TRACK, Number(selection.alias.slice(1)),
      `ecommerce.${expected.recipe}`);
    const plan = compileCalibrationFile(path, { trackRoot: TRACK.dir, stackBenchRoot: STACK_BENCH_ROOT,
      release: binding.release });
    const { release } = calibrationQualificationRelease(plan, binding.release, binding.execution);
    assert.equal(plan.id, expected.id);
    assert.deepEqual(plan.recipe, {
      path: `composition/recipes/${expected.recipe}.json`,
      id: release.id,
      meaningSha256: release.meaningSha256,
      executionSha256: release.executionSha256,
      contentSha256: release.contentSha256,
    }, name);
    assert.deepEqual(plan.qualification.stacks, expected.stacks, name);
    // Only the dependency L3 calibration has retained qualification evidence. Targeted
    // reruns add entries, but together they cover exactly each stack's reference and
    // mutation scopes plus the null control.
    assert.deepEqual([...new Set(plan.qualification.evidence.map(entry =>
      `${entry.kind}:${entry.stack ?? ''}:${entry.repetition}`))].sort(),
    name === 'dependency-l3.json' ? [...expected.stacks.flatMap(stack =>
      [`mutation:${stack}:1`, `reference:${stack}:1`]), 'null::1'].sort() : [], name);
    assert.deepEqual(plan.references.entries.map(reference => reference.backend).sort(), expected.stacks, name);
    assert.deepEqual(plan.mutations.map(mutation => mutation.backend).sort(), expected.stacks, name);
    assert.match(plan.contentSha256, /^[a-f0-9]{64}$/);
    assert.match(plan.qualificationSha256, /^[a-f0-9]{64}$/);
    assert.deepEqual(calibrationQualificationIdentity(plan), {
      id: plan.id,
      contentSha256: plan.qualificationSha256,
    });
    assert.deepEqual(plan.controls.map(control => [control.stableKey, control.role]), expected.controls, name);
    const scored = new Set(release.checkCatalog.filter(check => check.points > 0)
      .map(check => check.stableKey));
    assert(scored.size > 0, name);
    for (const reference of plan.references.entries) {
      assert.equal(reference.id, `ecommerce-reference-${reference.backend}`, name);
    }
    for (const mutation of plan.mutations) {
      const covered = new Set(mutation.targets.flatMap(target => target.stableKeys));
      assert.deepEqual([...scored].filter(key => !covered.has(key)), [], `${name} ${mutation.backend}`);
      const fixture = registry.fixtures.find(entry => entry.id === `ecommerce-reference-${mutation.backend}`);
      assert(fixture?.recipes, `${name} ${mutation.backend}`);
      assert(fixture.recipes.includes(release.id), `${name} ${mutation.backend}`);
      assert.deepEqual(fixture.mutationManifests, [mutation.path], `${name} ${mutation.backend}`);
    }
  }
});

test('covered depth metadata preserves the qualification identity', () => {
  const plan = compileCalibrationFile(join(TRACK.dir, 'composition/calibrations/dependency-l3.json'),
    { trackRoot: TRACK.dir, stackBenchRoot: STACK_BENCH_ROOT,
      release: requireRecipeRelease(TRACK, 3, 'ecommerce.progression-catalog').release });
  assert.deepEqual(plan.selection.coveredAliases, ['L1', 'L2', 'L3']);
  const exactDepthOnly = { ...plan, selection: { ...plan.selection } };
  delete exactDepthOnly.selection.coveredAliases;
  assert.deepEqual(calibrationQualificationIdentity(plan),
    calibrationQualificationIdentity(exactDepthOnly));
});

test('qualification accepts emitted artifact hashes and rejects mismatched identities', () => {
  const { release, plan, execution } = current();
  const reference = plan.references.entries[0]!;
  const calibration = { ...plan, qualification: { ...plan.qualification, runner: undefined } };
  const context = { calibration, qualificationIdentity: calibrationQualificationIdentity(plan),
    release, references: plan.references.entries, stackBenchRoot: STACK_BENCH_ROOT,
    execution, enforceQualificationScope: false };
  for (const kind of ['reference', 'mutation'] as const) {
    const repetitions = kind === 'reference' ? plan.qualification.referenceRepetitions
      : plan.qualification.mutationRepetitions;
    const artifact = createArtifact({ id: `qualification-${kind}`, kind: 'reference_qualification',
      identities: { recipe: { id: release.id, sha256: release.contentSha256 },
        calibration: { id: plan.id, sha256: plan.qualificationSha256 },
        fixture: { id: reference.id, sha256: reference.sourceSha256 },
        stackAdapter: { id: reference.backend, sha256: null } },
      payload: { fixture: reference.id, fixtureSha256: reference.sourceSha256,
        requiredRepetitions: repetitions, isolation: 'docker', mutationControl: kind === 'mutation',
        ok: true, stable: true, sameImage: true, sameHarness: true, harnessSha256: 'a'.repeat(64),
        qualifiedCheckKeys: release.checkCatalog.map(check => check.stableKey),
        runs: Array.from({ length: repetitions }, (_, index) => ({ repetition: index + 1,
          ok: true, processError: null, outcome: 'passed', failures: [],
          score: `${release.scoring.points}/${release.scoring.points}`, criteria: release.scoring.checks,
          zeroPointCriteria: release.checkCatalog.filter(check => check.points === 0).length,
          imageId: 'sha256:' + 'b'.repeat(64),
          harnessSha256Before: 'a'.repeat(64), harnessSha256After: 'a'.repeat(64),
          packRuntime: { packs: [...new Set(release.checkCatalog.map(check => check.packId))]
            .map(id => ({ id, exceeded: false })) },
          mutations: kind === 'mutation' ? { caught: 1, total: 1 } : null })) } });
    const entry = { kind, stack: reference.backend, repetition: 1, path: 'test.json', sha256: 'c'.repeat(64) };
    assert.doesNotThrow(() => validateQualificationEvidenceArtifact(artifact, entry, context));
    const timing = createArtifact({ ...artifact,
      payload: { ...artifact.payload, diagnostic: true, timingOnly: true } });
    assert.throws(() => validateQualificationEvidenceArtifact(timing, entry, context), /diagnostic evidence/);
    timing.payload.diagnostic = false;
    assert.throws(() => validateQualificationEvidenceArtifact(timing, entry, context), /diagnostic evidence/);
    for (const key of ['recipe', 'calibration', 'fixture'] as const) {
      for (const field of ['id', 'sha256'] as const) {
        const changed = structuredClone(artifact);
        changed.identities[key]![field] = field === 'id' ? 'wrong-id' : '0'.repeat(64);
        assert.throws(() => validateQualificationEvidenceArtifact(changed, entry, context), /identity|identities/);
      }
    }
  }
});

test('null qualification accepts blocked setup but rejects unmeasured or contradictory results', () => {
  const { release, plan } = current();
  const scored = release.checkCatalog.filter(check => check.points > 0);
  const zero = release.checkCatalog.length - scored.length;
  const totals = { criteria: scored.length, points: release.scoring.points };
  const artifact = createArtifact({ id: 'null-qualification', kind: 'null_control',
    identities: { recipe: { id: release.id, sha256: release.contentSha256 },
      calibration: { id: plan.id, sha256: plan.qualificationSha256 } },
    payload: { ok: true, tracks: [release.track], summary: { ...totals, expectedFailures: totals,
      vacuousPasses: { criteria: 0, points: 0 }, oracleGaps: { criteria: 0, points: 0 },
      unscored: { criteria: zero, passed: 0, failed: zero, inconclusive: 0 } },
      criteria: scored.map(check => ({ scenario: check.source, feature: check.featureId,
        criterion: check.criterionId, track: release.track, level: 3, points: check.points,
        status: 'expected_fail', evidenceStatus: 'blocked', failureStage: 'setup' })) } });
  const entry = { kind: 'null' as const, repetition: 1, path: 'null.json', sha256: 'a'.repeat(64) };
  const context = { calibration: { ...plan, qualification: { ...plan.qualification, runner: undefined } },
    qualificationIdentity: calibrationQualificationIdentity(plan), release,
    references: plan.references.entries, execution: [], stackBenchRoot: STACK_BENCH_ROOT,
    enforceQualificationScope: false };
  assert.doesNotThrow(() => validateQualificationEvidenceArtifact(artifact, entry, context));
  for (const patch of [{ evidenceStatus: 'inconclusive' }, { evidenceStatus: 'harness_failure' },
    { evidenceStatus: 'passed' }, { failureStage: 'assertion' }, { failureStage: null }]) {
    const changed = structuredClone(artifact);
    Object.assign(changed.payload.criteria[0]!, patch);
    assert.throws(() => validateQualificationEvidenceArtifact(changed, entry, context), /invalid null result/);
  }
});

test('calibration identity changes when selected checks change', () => {
  const value = compileCalibrationDefinition(calibrationSource());
  const identityInput = { ...value, mutations: value.mutations.map(mutation => ({
    ...mutation, executionSha256: 'a'.repeat(64),
  })) };
  const first = calibrationQualificationIdentity(identityInput);
  identityInput.qualification.checks = [...(identityInput.qualification.checks ?? []).slice(1)];
  assert.notDeepEqual(calibrationQualificationIdentity(identityInput), first);
});

test('calibration rejects unknown fields', () => {
  const value = calibrationSource() as Record<string, unknown>;
  value.version = '1.0.0';
  assert.throws(() => compileCalibrationDefinition(value), /unknown field/);
});

test('current level points come from the selected recipe', () => {
  const release = buildRecipeRelease(join(TRACK.dir, 'composition', 'recipes', 'sequential-l2.json'));
  const base = buildRecipeRelease(join(TRACK.dir, 'composition', 'recipes', 'sequential-l1.json'));
  assert.equal(currentLevelPoints(release, executionPlanForRelease(
    join(TRACK.dir, 'composition', 'recipes', 'sequential-l2.json'),
    { trackRoot: TRACK.dir, level: 2 },
  )), release.scoring.points - base.scoring.points);
  const currentRelease = requireRecipeRelease(TRACK, 3, 'ecommerce.progression-catalog').release;
  assert.equal(resolveCalibrationForRelease(currentRelease, {
    trackRoot: TRACK.dir,
    stackBenchRoot: STACK_BENCH_ROOT,
    alias: 'L3',
  })?.recipe.id, currentRelease.id);
});
