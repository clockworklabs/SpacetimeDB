import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT, compiledEntrypoint } from '../src/package-root.js';
import {
  buildRecipeRelease,
  bundleRecipeRelease,
  gradeRecipeRelease,
  requireRecipeRelease as resolveRecipeRelease,
  resolveGradeRecipeArtifactBinding,
  resolveGradeRecipeRelease,
  resolveRecipeRelease as resolveOptionalRecipeRelease,
  validateExactRecipeRequest,
} from '../src/composition/recipe-release.js';
import { compileTrackManifest } from '../src/composition/definition-compiler.js';
import { loadTrack, type Track } from '../src/composition/tracks.js';

const ECOMMERCE = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const L1_RECIPE = join(ECOMMERCE, 'composition', 'recipes', 'sequential-l1-2.5.0.json');

function copyTrack(): { temp: string; root: string; recipe: string } {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-release-'));
  const root = join(temp, 'ecommerce');
  cpSync(ECOMMERCE, root, { recursive: true });
  return { temp, root, recipe: join(root, 'composition', 'recipes', 'sequential-l1-2.5.0.json') };
}

function editJson(path: string, change: (value: unknown) => void): void {
  const value: unknown = JSON.parse(readFileSync(path, 'utf8'));
  change(value);
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

test('optional recipe resolution keeps missing composition explicit', () => {
  const box = copyTrack();
  try {
    rmSync(join(box.root, 'composition', 'promotions.json'));
    const track = copiedTrack(loadTrack('ecommerce'), box.root);
    assert.equal(resolveOptionalRecipeRelease(track, 1), null);
    assert.throws(() => resolveRecipeRelease(track, 1), /L1 has no recipe release/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

function promoteRecipe(root: string, alias: string, recipe: {
  path: string;
  id: string;
  version: string;
}): void {
  editJson(join(root, 'composition', 'promotions.json'), value => {
    const entries = array(object(value, 'promotion catalog').entries,
      'promotion catalog.entries');
    let target = entries.map(entry => object(entry, 'promotion entry')).find(entry => {
      const entryRecipe = object(entry.recipe, 'promotion entry.recipe');
      return entry.alias === alias && entryRecipe.id === recipe.id
        && entryRecipe.version === recipe.version;
    });
    for (const entryValue of entries) {
      const entry = object(entryValue, 'promotion entry');
      if (entry.alias === alias && entry.status === 'promoted') entry.status = 'retired';
    }
    if (!target) {
      target = { alias, status: 'promoted', recipe };
      entries.push(target);
    }
    target.status = 'promoted';
  });
}

function qualifyRecipe(root: string, name: string): void {
  const path = join(root, 'composition', 'recipes', name);
  const recipe = object(JSON.parse(readFileSync(path, 'utf8')), 'recipe');
  for (const selection of [object(recipe.fixture, 'recipe.fixture'),
    ...array(recipe.packs, 'recipe.packs').map(pack => object(pack, 'recipe pack'))]) {
    editJson(join(root, 'composition', 'recipes', string(selection.path, 'selection.path')),
      value => { object(value, 'selection').state = 'qualified'; });
  }
  editJson(path, value => { object(value, 'recipe').state = 'qualified'; });
}

function promoteCurrentDefaults(root: string): void {
  qualifyRecipe(root, 'sequential-l1-2.5.0.json');
  qualifyRecipe(root, 'sequential-l2-1.6.0.json');
  promoteRecipe(root, 'L1', {
    path: 'recipes/sequential-l1-2.5.0.json', id: 'ecommerce.sequential-l1', version: '2.5.0',
  });
  promoteRecipe(root, 'L2', {
    path: 'recipes/sequential-l2-1.6.0.json', id: 'ecommerce.sequential-l2', version: '1.6.0',
  });
}

function firstStepWithTestId(value: unknown): Record<string, unknown> {
  const scenario = object(value, 'scenario');
  for (const featureValue of array(scenario.features, 'scenario.features')) {
    const feature = object(featureValue, 'scenario feature');
    for (const criterionValue of array(feature.criteria, 'feature.criteria')) {
      const criterion = object(criterionValue, 'scenario criterion');
      const pending = [...array(criterion.steps, 'criterion.steps')];
      while (pending.length) {
        const step = pending.shift();
        const stepObject = object(step, 'scenario step');
        if (typeof stepObject.testid === 'string') return stepObject;
        if (stepObject.do === 'race') {
          for (const branch of array(stepObject.branches, 'race.branches')) {
            pending.push(...array(branch, 'race branch'));
          }
        }
      }
    }
  }
  throw new Error('fixture scenario has no criterion selector');
}

test('recipe releases are deterministic, compact, and bind L2 to the exact L1 release', () => {
  const l1a = buildRecipeRelease(L1_RECIPE, { trackRoot: ECOMMERCE });
  const l1b = buildRecipeRelease(L1_RECIPE, { trackRoot: ECOMMERCE });
  assert.deepEqual(l1a, l1b);
  assert.equal(l1a.checkCatalog.length, 48);
  assert.equal(l1a.scoring.points, 58);
  assert.equal(new Set(l1a.checkCatalog.map(check => check.stableKey)).size, 48);
  assert.match(l1a.task.composedSha256, /^[a-f0-9]{64}$/);
  assert(l1a.task.requirements.some(fragment =>
    fragment.id === 'ecommerce.spec.state-durability.accounts'));
  assert.match(l1a.contentSha256, /^[a-f0-9]{64}$/);
  assert.match(l1a.sourceManifestSha256, /^[a-f0-9]{64}$/);
  assert(l1a.sourceManifest.some(source => source.kinds.includes('fixture')));
  assert(!JSON.stringify(l1a).includes('stackbench-admin-2026'));

  const l2 = buildRecipeRelease(
    join(ECOMMERCE, 'composition', 'recipes', 'sequential-l2-1.6.0.json'),
    { trackRoot: ECOMMERCE },
  );
  assert.equal(l2.checkCatalog.length, 76);
  assert.equal(l2.scoring.points, 117);
  assert(l2.task.baseRecipe, 'L2 must bind its base recipe');
  assert.equal(l2.task.baseRecipe.contentSha256, l1a.contentSha256);
  assert(!JSON.stringify(l2).includes('stackbench-staff-2026'));
});

test('source formatting and pack declaration order do not change recipe content identity', () => {
  const box = copyTrack();
  try {
    const before = buildRecipeRelease(box.recipe, { trackRoot: box.root });
    editJson(box.recipe, recipe => {
      const value = object(recipe, 'recipe');
      value.packs = array(value.packs, 'recipe.packs').reverse();
    });
    const after = buildRecipeRelease(box.recipe, { trackRoot: box.root });
    assert.equal(after.meaningSha256, before.meaningSha256);
    assert.equal(after.executionSha256, before.executionSha256);
    assert.equal(after.contentSha256, before.contentSha256);
    assert.notEqual(after.sourceManifestSha256, before.sourceManifestSha256);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('requirement edits change meaning while selector and fixture edits only change execution', () => {
  const descriptionBox = copyTrack();
  try {
    const before = buildRecipeRelease(descriptionBox.recipe, { trackRoot: descriptionBox.root });
    const scenario = join(descriptionBox.root, 'scenarios', '01-account-create-2.4.0.json');
    editJson(scenario, value => {
      const criterion = scenarioCriterion(value, 0, 0);
      criterion.desc = `${string(criterion.desc, 'criterion.desc')} with the revised guarantee`;
    });
    const after = buildRecipeRelease(descriptionBox.recipe, { trackRoot: descriptionBox.root });
    assert.notEqual(after.meaningSha256, before.meaningSha256);
    assert.equal(after.executionSha256, before.executionSha256);
    assert.notEqual(after.contentSha256, before.contentSha256);
  } finally { rmSync(descriptionBox.temp, { recursive: true, force: true }); }

  const selectorBox = copyTrack();
  try {
    const before = buildRecipeRelease(selectorBox.recipe, { trackRoot: selectorBox.root });
    const scenario = join(selectorBox.root, 'scenarios', '01-account-create-2.4.0.json');
    editJson(scenario, value => {
      const step = firstStepWithTestId(value);
      step.testid = `${string(step.testid, 'step.testid')}-revised`;
    });
    const after = buildRecipeRelease(selectorBox.recipe, { trackRoot: selectorBox.root });
    assert.equal(after.meaningSha256, before.meaningSha256);
    assert.notEqual(after.executionSha256, before.executionSha256);
    assert.notEqual(after.contentSha256, before.contentSha256);
  } finally { rmSync(selectorBox.temp, { recursive: true, force: true }); }

  const fixtureBox = copyTrack();
  try {
    const before = buildRecipeRelease(fixtureBox.recipe, { trackRoot: fixtureBox.root });
    const fixture = join(fixtureBox.root, 'composition', 'fixtures', 'storefront-1.0.0.json');
    editJson(fixture, value => {
      const fixtureValue = object(value, 'fixture');
      const account = object(requiredAt(array(fixtureValue.accounts, 'fixture.accounts'), 0,
        'fixture account'), 'fixture account');
      account.password = `${string(account.password, 'account.password')}-revised`;
    });
    const after = buildRecipeRelease(fixtureBox.recipe, { trackRoot: fixtureBox.root });
    assert.equal(after.meaningSha256, before.meaningSha256);
    assert.notEqual(after.executionSha256, before.executionSha256);
    assert.notEqual(after.contentSha256, before.contentSha256);
  } finally { rmSync(fixtureBox.temp, { recursive: true, force: true }); }
});

test('promoted runner binding fails closed on drift and emits only the selected execution catalog', () => {
  const box = copyTrack();
  try {
    promoteCurrentDefaults(box.root);
    const track = copiedTrack(loadTrack('ecommerce'), box.root);
    const binding = resolveRecipeRelease(track, 2);
    assert.equal(binding.alias, 'L2');
    assert.equal(binding.status, 'promoted');
    const reportRelease = gradeRecipeRelease(binding, 'features-existing@L2');
    assert(reportRelease, 'the selected execution must produce a grade release');
    assert(reportRelease.checks.length > 0);
    assert(reportRelease.checks.every(check => check.executionId === 'features-existing@L2'));
    assert.equal(reportRelease.contentSha256, binding.release.contentSha256);
    const resolvedReport = resolveGradeRecipeRelease(track, 2,
      join(track.dir, 'scenarios', '02-features.json'));
    assert.deepEqual(resolvedReport, reportRelease);
    const artifactBinding = resolveGradeRecipeArtifactBinding(track, 2,
      join(track.dir, 'scenarios', '02-features.json'));
    assert(artifactBinding, 'the selected execution must produce an artifact binding');
    assert.deepEqual(artifactBinding.release, reportRelease);
    assert.equal(artifactBinding.sourceRelease.components.fixture.sha256,
      binding.release.components.fixture.sha256);
    assert(artifactBinding.sourceRelease.components.packs
      .every(pack => /^[a-f0-9]{64}$/.test(pack.sha256)));
    const bundled = bundleRecipeRelease(binding);
    assert(bundled, 'the promoted binding must produce a release bundle');
    assert.equal(bundled.selection.alias, 'L2');
    assert.equal(bundled.checkCatalog.length, 76);
    assert(!JSON.stringify(bundled).includes('stackbench-staff-2026'));
    assert.throws(() => resolveGradeRecipeRelease(track, 2,
      join(track.dir, 'scenarios', '03-features.json')), /does not select scenario/);

    editJson(join(box.root, 'composition', 'recipes', 'sequential-l2-1.6.0.json'),
      value => {
        const recipe = object(value, 'recipe');
        const execution = object(requiredAt(array(recipe.execution, 'recipe.execution'), 0,
          'recipe execution'), 'recipe execution');
        execution.source = 'scenarios/does-not-exist.json';
      });
    assert.throws(() => resolveRecipeRelease(track, 2), /does-not-exist|ENOENT/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('the current sequential release supports exact resolution and rejects bad identities', () => {
  const box = copyTrack();
  try {
    promoteCurrentDefaults(box.root);
    const track = copiedTrack(loadTrack('ecommerce'), box.root);
    const promoted = resolveRecipeRelease(track, 1);
    const exact = resolveRecipeRelease(track, 1, 'ecommerce.sequential-l1@2.5.0');
    assert.equal(promoted.release.id, 'ecommerce.sequential-l1');
    assert.equal(promoted.release.version, '2.5.0');
    assert.equal(promoted.status, 'promoted');
    assert.equal(exact.release.version, '2.5.0');
    assert.equal(exact.status, 'promoted');
    assert.equal(promoted.release.checkCatalog.length, 48);
    assert.equal(promoted.release.scoring.points, 58);
    assert.equal(resolveRecipeRelease(track, 1, {
      id: promoted.release.id,
      version: promoted.release.version,
      contentSha256: promoted.release.contentSha256,
    }).release.contentSha256, promoted.release.contentSha256);
    assert.throws(() => resolveRecipeRelease(track, 1, 'ecommerce.unknown@9.9.9'),
      /exactly one catalogued/);
    assert.throws(() => resolveRecipeRelease(track, 1, {
      id: promoted.release.id, version: promoted.release.version, contentSha256: '0'.repeat(64),
    }), /content changed/);
    assert.throws(() => resolveRecipeRelease(track, 1, {
      id: promoted.release.id, version: promoted.release.version, contentSha256: '',
    }), /must be a SHA-256 digest/);
    assert.throws(() => validateExactRecipeRequest({
      id: promoted.release.id, version: promoted.release.version, unexpected: true,
    }), /unknown field/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('the current L2 release binds sequential L1 exactly and retains its own checks', () => {
  const box = copyTrack();
  try {
    promoteCurrentDefaults(box.root);
    const track = copiedTrack(loadTrack('ecommerce'), box.root);
    const binding = resolveRecipeRelease(track, 2, 'ecommerce.sequential-l2@1.6.0');
    const l1 = buildRecipeRelease(
      join(box.root, 'composition', 'recipes', 'sequential-l1-2.5.0.json'),
      { trackRoot: box.root },
    );
    assert.equal(binding.status, 'promoted');
    const baseRecipe = binding.release.task.baseRecipe;
    assert(baseRecipe, 'the promoted L2 release must bind its base recipe');
    assert.equal(baseRecipe.contentSha256, l1.contentSha256);
    assert.equal(baseRecipe.meaningSha256, l1.meaningSha256);
    assert.equal(baseRecipe.executionSha256, l1.executionSha256);
    assert.deepEqual({ checks: binding.release.checkCatalog.length,
      points: binding.release.scoring.points }, { checks: 76, points: 117 });
    assert.deepEqual(binding.release.checkCatalog
      .filter(check => l1.checkCatalog.some(base => base.stableKey === check.stableKey))
      .map(check => check.stableKey).sort(), l1.checkCatalog.map(check => check.stableKey).sort());
    assert(binding.release.checkCatalog.some(check =>
      !l1.checkCatalog.some(base => base.stableKey === check.stableKey)));
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('an exact promoted release stays separate from public level defaults', () => {
  const box = copyTrack();
  try {
    promoteCurrentDefaults(box.root);
    const recipePath = join(box.root, 'composition', 'recipes', 'progression-depth3-2.0.1.json');
    const recipe = object(JSON.parse(readFileSync(recipePath, 'utf8')), 'progression recipe');
    for (const packValue of array(recipe.packs, 'progression recipe.packs')) {
      const pack = object(packValue, 'progression recipe pack');
      editJson(join(box.root, 'composition', 'recipes', string(pack.path, 'pack.path')),
        value => { object(value, 'pack').state = 'qualified'; });
    }
    editJson(recipePath, value => { object(value, 'recipe').state = 'qualified'; });
    editJson(join(box.root, 'composition', 'candidates.json'), value => {
      for (const entryValue of array(object(value, 'candidate catalog').entries,
        'candidate catalog.entries')) {
        const entry = object(entryValue, 'candidate entry');
        const entryRecipe = object(entry.recipe, 'candidate entry.recipe');
        if (entryRecipe.id === 'ecommerce.progression-depth3'
          && entryRecipe.version === '2.0.1') entry.status = 'promoted';
      }
    });
    const track = loadTrack('ecommerce');
    const copied = copiedTrack(track, box.root);

    assert.equal(resolveRecipeRelease(copied, 1).release.id, 'ecommerce.sequential-l1');
    assert.equal(resolveRecipeRelease(copied, 2).release.id, 'ecommerce.sequential-l2');
    for (const level of [1, 2, 3]) {
      const exact = resolveRecipeRelease(copied, level,
        'ecommerce.progression-depth3@2.0.1');
      assert.equal(exact.status, 'promoted');
      assert.equal(exact.catalog.id, 'ecommerce.recipe-candidates');
    }
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('a sequential candidate accepts the exact current prior level', () => {
  const box = copyTrack();
  try {
    const recipe = join(box.root, 'composition', 'recipes', 'l3-bootstrap-1.0.0.json');
    const source = object(JSON.parse(readFileSync(
      join(box.root, 'composition', 'recipes', 'sequential-l2-1.6.0.json'), 'utf8')),
    'L2 recipe');
    source.id = 'ecommerce.l3-bootstrap';
    source.version = '1.0.0';
    source.state = 'draft';
    source.title = 'L3 bootstrap fixture';
    source.sequence = { level: 3 };
    object(source.task, 'recipe.task').baseRecipe = { path: 'sequential-l2-1.6.0.json',
      id: 'ecommerce.sequential-l2', version: '1.6.0' };
    writeFileSync(recipe, `${JSON.stringify(source, null, 2)}\n`);
    writeFileSync(join(box.root, 'composition', 'candidates.json'), `${JSON.stringify({
      schemaVersion: 1,
      kind: 'promotion-catalog',
      id: 'ecommerce.candidates',
      version: '1.0.0',
      state: 'draft',
      title: 'Test candidates',
      entries: [{ alias: 'L3', status: 'candidate', recipe: {
        path: 'recipes/l3-bootstrap-1.0.0.json',
        id: string(source.id, 'recipe.id'), version: string(source.version, 'recipe.version'),
      } }],
    }, null, 2)}\n`);
    const track = loadTrack('ecommerce');
    const copied = copiedTrack(track, box.root);

    const candidate = resolveRecipeRelease(copied, 3, 'ecommerce.l3-bootstrap@1.0.0');
    assert.equal(candidate.status, 'candidate');
    assert(candidate.execution.every(execution => execution.ownership.kind === 'inherited'));
    const inheritedLevels = candidate.execution.flatMap(execution =>
      execution.ownership.kind === 'inherited' && typeof execution.ownership.fromLevel === 'number'
        ? [execution.ownership.fromLevel] : []);
    assert.deepEqual([...new Set(inheritedLevels)]
      .sort(), [1, 2]);

    editJson(recipe, value => {
      object(object(value, 'recipe').task, 'recipe.task').baseRecipe = {
        path: 'sequential-l1-2.5.0.json',
        id: 'ecommerce.sequential-l2', version: '1.6.0' };
    });
    assert.throws(() => resolveRecipeRelease(copied, 3,
      'ecommerce.l3-bootstrap@1.0.0'), /expected ecommerce\.sequential-l2@1\.6\.0/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('the grader rejects parent/child recipe drift before launching a browser', () => {
  const grader = compiledEntrypoint('grader', 'grade.js');
  const result = spawnSync(process.execPath, [grader,
    '--url', 'http://127.0.0.1:1',
    '--level', '1',
    '--track', 'ecommerce',
    '--spec', join(ECOMMERCE, 'scenarios', '01-account-create-2.4.0.json'),
    '--expected-recipe-sha256', '0'.repeat(64),
  ], { encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.match(`${result.stdout}${result.stderr}`, /recipe changed before grading/);
  assert.doesNotMatch(`${result.stdout}${result.stderr}`, /browser.*launch|ECONNREFUSED/i);
});

test('the grader rejects a stale selected check before launching a browser', () => {
  const grader = compiledEntrypoint('grader', 'grade.js');
  const result = spawnSync(process.execPath, [grader,
    '--url', 'http://127.0.0.1:1',
    '--level', '1',
    '--track', 'ecommerce',
    '--spec', join(ECOMMERCE, 'scenarios', '01-account-create-2.4.0.json'),
    '--selected-check', 'ecommerce.missing.check',
  ], { encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.match(`${result.stdout}${result.stderr}`, /no selected check/);
  assert.doesNotMatch(`${result.stdout}${result.stderr}`, /browser.*launch|ECONNREFUSED/i);
});

function object(value: unknown, at: string): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new Error(`${at} must be an object`);
  }
  return value;
}

function array(value: unknown, at: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`${at} must be an array`);
  return value;
}

function string(value: unknown, at: string): string {
  if (typeof value !== 'string') throw new Error(`${at} must be a string`);
  return value;
}

function requiredAt(values: unknown[], index: number, at: string): unknown {
  const value = values[index];
  if (value === undefined) throw new Error(`${at} is required`);
  return value;
}

function scenarioCriterion(value: unknown, featureIndex: number, criterionIndex: number):
  Record<string, unknown> {
  const scenario = object(value, 'scenario');
  const feature = object(requiredAt(array(scenario.features, 'scenario.features'), featureIndex,
    'scenario feature'), 'scenario feature');
  return object(requiredAt(array(feature.criteria, 'feature.criteria'), criterionIndex,
    'scenario criterion'), 'scenario criterion');
}

function copiedTrack(track: Track, root: string): Track {
  const manifest: unknown = JSON.parse(readFileSync(join(root, 'track.json'), 'utf8'));
  const compiled = compileTrackManifest(manifest, { source: join(root, 'track.json') });
  return { ...track, dir: root, suites: compiled.suites };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
