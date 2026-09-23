import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { createNullQualification, nullControlSuites, parseNullControlArgs } from '../commands/null-control.js';
import { resolveCalibrationForRelease } from '../src/composition/calibration-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import type { RecipeCheck } from '../src/composition/recipe-release.js';
import { selectScenarioChecks } from '../src/composition/recipe-selection.js';
import { loadTrack } from '../src/composition/tracks.js';

const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));

interface RecipeSuite {
  id: string;
  spec: string;
  level: number;
  checks: RecipeCheck[];
}

function hasChecks(suite: unknown): suite is RecipeSuite {
  return suite !== null && typeof suite === 'object' && 'checks' in suite
    && Array.isArray(suite.checks);
}

test('null qualification can select one exact track and level', () => {
  const args = parseNullControlArgs(['node', 'null-control.js',
    '--track', 'ecommerce', '--level', '1']);
  assert.deepEqual(args.tracks, ['ecommerce']);
  assert.equal(args.level, 1);
  assert.throws(() => parseNullControlArgs(['node', 'null-control.js',
    '--track', 'ecommerce,chat', '--level', '1']), /exactly one/);
  assert.throws(() => parseNullControlArgs(['node', 'null-control.js',
    '--track', 'ecommerce', '--level', '0']), /positive integer/);
  assert.equal(parseNullControlArgs(['node', 'null-control.js', '--track', 'ecommerce',
    '--level', '1', '--recipe', 'ecommerce.sequential-l1']).recipe,
  'ecommerce.sequential-l1');
  assert.throws(() => parseNullControlArgs(['node', 'null-control.js', '--track', 'ecommerce',
    '--recipe', 'ecommerce.sequential-l1']), /requires --level/);
  const track = loadTrack('ecommerce');
  assert.deepEqual(nullControlSuites(track, 2).map(suite => suite.level), [2, 2, 2, 2, 2]);
  assert.throws(() => nullControlSuites(track, 4), /not declared/);
});

test('targeted null selection keeps only calibrated checks and their execution', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.sequential-l1');
  const calibration = resolveCalibrationForRelease(binding.release,
    { trackRoot: track.dir, stackBenchRoot: STACK_BENCH_ROOT, alias: 'L1' })!;
  const key = binding.release.checkCatalog[0]!.stableKey;
  const args = parseNullControlArgs(['node', 'null-control.js', '--track', 'ecommerce',
    '--level', '1', '--selected-check', key]);
  const selected = createNullQualification(binding, calibration, args.selectedChecks);
  assert.deepEqual(selected.binding.release.checkCatalog.map(check => check.stableKey), [key]);
  assert.equal(selected.binding.execution.length, 1);
  assert.throws(() => createNullQualification(binding, calibration, ['unknown']), /unknown checks/);
  assert.throws(() => parseNullControlArgs(['node', 'null-control.js', '--selected-check', key]), /requires --level/);
});

test('bound recipes can qualify levels outside legacy suites without widening legacy selection', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 6, 'ecommerce.progression-catalog');
  const calibration = resolveCalibrationForRelease(binding.release,
    { trackRoot: track.dir, stackBenchRoot: STACK_BENCH_ROOT, alias: 'L6' })!;
  const key = 'ecommerce.returns-pricing.cancellation-and-return.3d';
  const selected = createNullQualification(binding, calibration, [key]);
  const suites = nullControlSuites(track, 6, selected.binding);
  assert(suites.every(suite => suite.level === 6));
  assert.deepEqual(suites.filter(hasChecks).flatMap(suite => suite.checks.map(check => check.stableKey)), [key]);
  assert.throws(() => nullControlSuites(track, 6), /not declared/);
});

test('recipe-bound null qualification grades the exact modular execution and checks', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.sequential-l1');
  const suites = nullControlSuites(track, 1, binding);
  assert(suites.every(hasChecks));
  const recipeSuites = suites.filter(hasChecks);
  const checks = recipeSuites.flatMap(suite => suite.checks);

  assert.deepEqual(suites.map(suite => suite.id), binding.execution.map(execution => execution.id));
  assert.equal(checks.length, 47);
  assert.equal(checks.reduce((total, check) => total + check.points, 0), 57);
  assert.equal(checks.filter(check => check.points === 0).length, 2);
  assert.equal(new Set(checks.map(check => check.stableKey)).size, 47);

  const selectedByExecution = new Map(recipeSuites.map(suite => {
    const scenario = compileScenarioDefinition(readJson(suite.spec),
      { source: suite.spec, expectedLevel: 1 });
    const selected = selectScenarioChecks(scenario, { checks: binding.release.checkCatalog },
      suite.checks.map(check => check.stableKey));
    const selectedKeys = selected.features.flatMap(feature => feature.criteria
      .map(criterion => suite.checks.find(check => check.featureId === feature.id
        && check.criterionId === criterion.id)?.stableKey)).filter(Boolean).sort();
    assert.deepEqual(selectedKeys, suite.checks.map(check => check.stableKey).sort());
    return [suite.id, selectedKeys];
  }));
  for (const criterion of ['101a', '102a', '103a', '104a']) {
    const owners = [...selectedByExecution.entries()]
      .filter(([, keys]) => keys.some(key => key.endsWith(`.${criterion}`)));
    assert.equal(owners.length, 1);
    const owner = owners[0];
    assert(owner);
    assert.equal(owner[0].includes('invariants'), false);
  }
});

test('recipe-bound null qualification rejects an execution with no mapped checks', () => {
  const track = loadTrack('ecommerce');
  const binding = structuredClone(resolveRecipeRelease(track, 1, 'ecommerce.sequential-l1'));
  const firstExecution = binding.execution[0];
  assert(firstExecution);
  binding.execution.push({ id: 'empty-execution', source: firstExecution.source,
    ownership: { kind: 'current', level: 1 } });
  assert.throws(() => nullControlSuites(track, 1, binding), /maps no checks/);
});
