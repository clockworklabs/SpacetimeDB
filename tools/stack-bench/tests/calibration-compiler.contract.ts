import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { calibrationQualificationIdentity, compileCalibrationDefinition,
  compileCalibrationFile, currentLevelPoints, hasExactSelectedPackRuntime,
  resolveCalibrationForRelease } from '../src/composition/calibration-compiler.js';
import { buildRecipeRelease, executionPlanForRelease,
  requireRecipeRelease } from '../src/composition/recipe-release.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { loadTrack } from '../src/composition/tracks.js';

const TRACK = loadTrack('ecommerce');
const CALIBRATION = join(TRACK.dir, 'composition', 'calibrations', 'sequential-l1.json');

function calibrationSource(): unknown {
  return JSON.parse(readFileSync(CALIBRATION, 'utf8'));
}

function current() {
  const release = requireRecipeRelease(TRACK, 1).release;
  return { release, plan: compileCalibrationFile(CALIBRATION, {
    trackRoot: TRACK.dir,
    stackBenchRoot: STACK_BENCH_ROOT,
    release,
  }) };
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

test('current calibration binds stable authored identities', () => {
  const { release, plan } = current();
  assert.equal(plan.id, 'ecommerce.sequential-l1-calibration');
  assert.deepEqual(plan.recipe, {
    path: 'composition/recipes/sequential-l1.json',
    id: release.id,
    meaningSha256: release.meaningSha256,
    executionSha256: release.executionSha256,
    contentSha256: release.contentSha256,
  });
  assert.deepEqual(plan.qualification.stacks, ['mongodb', 'postgres', 'spacetime']);
  assert.equal(plan.qualification.evidence.length, 0);
  assert.equal(plan.references.entries.length, 3);
  assert.equal(plan.mutations.length, 3);
  assert.match(plan.contentSha256, /^[a-f0-9]{64}$/);
  assert.match(plan.qualificationSha256, /^[a-f0-9]{64}$/);
  assert.deepEqual(calibrationQualificationIdentity(plan), {
    id: plan.id,
    contentSha256: plan.qualificationSha256,
  });
  const scored = new Set(release.checkCatalog.filter(check => check.points > 0)
    .map(check => check.stableKey));
  for (const mutation of plan.mutations) {
    const covered = new Set(mutation.targets.flatMap(target => target.stableKeys));
    assert.deepEqual([...scored].filter(key => !covered.has(key)), [], mutation.backend);
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

test('calibration rejects retired authored lifecycle fields', () => {
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
  assert.equal(resolveCalibrationForRelease(release, {
    trackRoot: TRACK.dir,
    stackBenchRoot: STACK_BENCH_ROOT,
    alias: 'L2',
  })?.recipe.id, release.id);
});
