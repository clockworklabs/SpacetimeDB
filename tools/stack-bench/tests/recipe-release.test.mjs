import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import {
  buildRecipeRelease,
  bundleRecipeRelease,
  gradeRecipeRelease,
  resolveGradeRecipeArtifactBinding,
  resolveGradeRecipeRelease,
  resolveRecipeRelease,
} from '../recipe-release.mjs';
import { loadTrack } from '../tracks.mjs';

const ECOMMERCE = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const L1_RECIPE = join(ECOMMERCE, 'composition', 'recipes', 'l1-standard-1.1.0.json');
const RETIRED_L1_RECIPE = join(ECOMMERCE, 'composition', 'recipes', 'l1-standard-1.0.0.json');

function copyTrack() {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-release-'));
  const root = join(temp, 'ecommerce');
  cpSync(ECOMMERCE, root, { recursive: true });
  return { temp, root, recipe: join(root, 'composition', 'recipes', 'l1-standard-1.1.0.json') };
}

function editJson(path, change) {
  const value = JSON.parse(readFileSync(path, 'utf8'));
  change(value);
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function firstStepWithTestId(value) {
  for (const feature of value.features) {
    for (const criterion of feature.criteria) {
      const pending = [...criterion.steps];
      while (pending.length) {
        const step = pending.shift();
        if (step.testid) return step;
        if (step.do === 'race') pending.push(...step.branches.flat());
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
  assert.equal(l1a.scoring.points, 51);
  assert.equal(new Set(l1a.checkCatalog.map(check => check.stableKey)).size, 48);
  assert.match(l1a.task.composedSha256, /^[a-f0-9]{64}$/);
  assert(l1a.task.requirements.some(fragment => fragment.id === 'ecommerce.l1.session-durability'));
  assert.match(l1a.contentSha256, /^[a-f0-9]{64}$/);
  assert.match(l1a.sourceManifestSha256, /^[a-f0-9]{64}$/);
  assert(l1a.sourceManifest.some(source => source.kinds.includes('fixture')));
  assert(!JSON.stringify(l1a).includes('stackbench-admin-2026'));

  const l2 = buildRecipeRelease(
    join(ECOMMERCE, 'composition', 'recipes', 'l2-standard-1.2.0.json'),
    { trackRoot: ECOMMERCE },
  );
  assert.equal(l2.checkCatalog.length, 53);
  assert.equal(l2.scoring.points, 75);
  assert.equal(l2.task.baseRecipe.contentSha256, l1a.contentSha256);
  assert(!JSON.stringify(l2).includes('stackbench-staff-2026'));
});

test('source formatting and pack declaration order do not change recipe content identity', () => {
  const box = copyTrack();
  try {
    const before = buildRecipeRelease(box.recipe, { trackRoot: box.root });
    editJson(box.recipe, recipe => recipe.packs.reverse());
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
    const scenario = join(descriptionBox.root, 'scenarios', '01-features.json');
    editJson(scenario, value => { value.features[0].criteria[0].desc += ' with the revised guarantee'; });
    const after = buildRecipeRelease(descriptionBox.recipe, { trackRoot: descriptionBox.root });
    assert.notEqual(after.meaningSha256, before.meaningSha256);
    assert.equal(after.executionSha256, before.executionSha256);
    assert.notEqual(after.contentSha256, before.contentSha256);
  } finally { rmSync(descriptionBox.temp, { recursive: true, force: true }); }

  const selectorBox = copyTrack();
  try {
    const before = buildRecipeRelease(selectorBox.recipe, { trackRoot: selectorBox.root });
    const scenario = join(selectorBox.root, 'scenarios', '01-features.json');
    editJson(scenario, value => { firstStepWithTestId(value).testid += '-revised'; });
    const after = buildRecipeRelease(selectorBox.recipe, { trackRoot: selectorBox.root });
    assert.equal(after.meaningSha256, before.meaningSha256);
    assert.notEqual(after.executionSha256, before.executionSha256);
    assert.notEqual(after.contentSha256, before.contentSha256);
  } finally { rmSync(selectorBox.temp, { recursive: true, force: true }); }

  const fixtureBox = copyTrack();
  try {
    const before = buildRecipeRelease(fixtureBox.recipe, { trackRoot: fixtureBox.root });
    const fixture = join(fixtureBox.root, 'composition', 'fixtures', 'storefront-1.0.0.json');
    editJson(fixture, value => { value.accounts[0].password += '-revised'; });
    const after = buildRecipeRelease(fixtureBox.recipe, { trackRoot: fixtureBox.root });
    assert.equal(after.meaningSha256, before.meaningSha256);
    assert.notEqual(after.executionSha256, before.executionSha256);
    assert.notEqual(after.contentSha256, before.contentSha256);
  } finally { rmSync(fixtureBox.temp, { recursive: true, force: true }); }
});

test('legacy runner binding fails closed on drift and emits only the suite check catalog', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 2);
  assert.equal(binding.alias, 'L2');
  assert.equal(binding.status, 'promoted');
  const reportRelease = gradeRecipeRelease(binding, 'features');
  assert(reportRelease.checks.length > 0);
  assert(reportRelease.checks.every(check => check.executionId === 'features'));
  assert.equal(reportRelease.contentSha256, binding.release.contentSha256);
  const resolvedReport = resolveGradeRecipeRelease(track, 2,
    join(track.dir, 'scenarios', '02-features.json'));
  assert.deepEqual(resolvedReport, reportRelease);
  const artifactBinding = resolveGradeRecipeArtifactBinding(track, 2,
    join(track.dir, 'scenarios', '02-features.json'));
  assert.deepEqual(artifactBinding.release, reportRelease);
  assert.equal(artifactBinding.sourceRelease.components.fixture.sha256,
    binding.release.components.fixture.sha256);
  assert(artifactBinding.sourceRelease.components.packs.every(pack => /^[a-f0-9]{64}$/.test(pack.sha256)));
  const bundled = bundleRecipeRelease(binding);
  assert.equal(bundled.selection.alias, 'L2');
  assert.equal(bundled.checkCatalog.length, 53);
  assert(!JSON.stringify(bundled).includes('stackbench-staff-2026'));
  assert.throws(() => resolveGradeRecipeRelease(track, 2,
    join(track.dir, 'scenarios', '03-features.json')), /does not select scenario/);

  const box = copyTrack();
  try {
    editJson(box.recipe, value => { value.execution.reverse(); });
    const copiedTrack = { ...track, dir: box.root,
      suites: JSON.parse(readFileSync(join(box.root, 'track.json'), 'utf8')).suites };
    assert.throws(() => resolveRecipeRelease(copiedTrack, 1), /does not exactly match/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('the framework-neutral release is the promoted default and the retired release cannot launch', () => {
  const track = loadTrack('ecommerce');
  const promoted = resolveRecipeRelease(track, 1);
  const exact = resolveRecipeRelease(track, 1, 'ecommerce.l1-standard@1.1.0');
  const retired = buildRecipeRelease(RETIRED_L1_RECIPE, { trackRoot: ECOMMERCE });
  assert.equal(promoted.release.version, '1.1.0');
  assert.equal(promoted.status, 'promoted');
  assert.equal(exact.release.version, '1.1.0');
  assert.equal(exact.status, 'promoted');
  assert.equal(retired.executionSha256, promoted.release.executionSha256);
  assert.notEqual(retired.meaningSha256, promoted.release.meaningSha256);
  const stableChecks = release => release.checkCatalog.map(({ packVersion: _packVersion, ...check }) => check);
  assert.deepEqual(stableChecks(retired), stableChecks(promoted.release));
  assert.equal(resolveRecipeRelease(track, 1, {
    id: promoted.release.id,
    version: promoted.release.version,
    contentSha256: promoted.release.contentSha256,
  }).release.contentSha256, promoted.release.contentSha256);
  assert.throws(() => resolveRecipeRelease(track, 1, 'ecommerce.l1-standard@1.0.0'),
    /exactly one catalogued/);
  assert.throws(() => resolveRecipeRelease(track, 1, 'ecommerce.l1-standard@9.9.9'),
    /exactly one catalogued/);
  assert.throws(() => resolveRecipeRelease(track, 1, {
    id: promoted.release.id, version: promoted.release.version, contentSha256: '0'.repeat(64),
  }), /content changed/);
  assert.throws(() => resolveRecipeRelease(track, 1, {
    id: promoted.release.id, version: promoted.release.version, contentSha256: '',
  }), /must be a SHA-256 digest/);
  assert.throws(() => resolveRecipeRelease(track, 1, {
    id: promoted.release.id, version: promoted.release.version, unexpected: true,
  }), /unknown field/);
});

test('the catalogued L2 candidate binds modular L1 exactly and keeps all L2-only checks', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 2, 'ecommerce.l2-standard@1.3.0');
  const l1 = buildRecipeRelease(
    join(ECOMMERCE, 'composition', 'recipes', 'l1-modular-2.2.0.json'),
    { trackRoot: ECOMMERCE },
  );
  const previous = resolveRecipeRelease(track, 2).release;
  const l2Packs = new Set([
    'ecommerce.operations-access',
    'ecommerce.inventory-operations',
    'ecommerce.returns-pricing',
  ]);
  assert.equal(binding.status, 'candidate');
  assert.equal(binding.release.task.baseRecipe.contentSha256, l1.contentSha256);
  assert.equal(binding.release.task.baseRecipe.meaningSha256, l1.meaningSha256);
  assert.equal(binding.release.task.baseRecipe.executionSha256, l1.executionSha256);
  assert.deepEqual({ checks: binding.release.checkCatalog.length, points: binding.release.scoring.points },
    { checks: 76, points: 111 });
  assert.deepEqual(binding.release.checkCatalog
    .filter(check => l1.checkCatalog.some(base => base.stableKey === check.stableKey))
    .map(check => check.stableKey).sort(),
  l1.checkCatalog.map(check => check.stableKey).sort());
  assert.deepEqual(binding.release.checkCatalog.filter(check => l2Packs.has(check.packId))
    .map(check => check.stableKey).sort(),
  previous.checkCatalog.filter(check => l2Packs.has(check.packId))
    .map(check => check.stableKey).sort());
});

test('cumulative candidate resolution rejects dropped L2 coverage', () => {
  const box = copyTrack();
  try {
    const candidate = join(box.root, 'composition', 'recipes', 'l2-standard-1.3.0.json');
    editJson(candidate, value => {
      value.packs = value.packs.filter(pack => pack.id !== 'ecommerce.returns-pricing');
    });
    const track = loadTrack('ecommerce');
    const copiedTrack = { ...track, dir: box.root,
      suites: JSON.parse(readFileSync(join(box.root, 'track.json'), 'utf8')).suites };
    assert.throws(() => resolveRecipeRelease(copiedTrack, 2, 'ecommerce.l2-standard@1.3.0'),
      /changes the cumulative L2 check set/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('cumulative continuity does not trust mutable scenario level labels', () => {
  const box = copyTrack();
  try {
    for (const name of ['02-features.json', '02-invariants.json', '02-server-actions-1.0.0.json']) {
      editJson(join(box.root, 'scenarios', name), value => { value.level = 1; });
    }
    const candidate = join(box.root, 'composition', 'recipes', 'l2-standard-1.3.0.json');
    editJson(candidate, value => {
      value.packs = value.packs.filter(pack => ![
        'ecommerce.operations-access',
        'ecommerce.inventory-operations',
        'ecommerce.returns-pricing',
      ].includes(pack.id));
      value.execution = value.execution.filter(execution => execution.id.endsWith('@L1'));
    });
    const track = loadTrack('ecommerce');
    const copiedTrack = { ...track, dir: box.root,
      suites: JSON.parse(readFileSync(join(box.root, 'track.json'), 'utf8')).suites };
    assert.throws(() => resolveRecipeRelease(copiedTrack, 2, 'ecommerce.l2-standard@1.3.0'),
      /changes the cumulative L2 check set/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('promoting a cumulative recipe cannot drop checks retained from earlier releases', () => {
  const box = copyTrack();
  try {
    const packs = join(box.root, 'composition', 'packs');
    for (const name of readdirSync(packs).filter(name => name.endsWith('.json'))) {
      editJson(join(packs, name), value => { value.state = 'qualified'; });
    }
    editJson(join(box.root, 'composition', 'recipes', 'l1-modular-2.2.0.json'),
      value => { value.state = 'qualified'; });
    const candidate = join(box.root, 'composition', 'recipes', 'l2-standard-1.3.0.json');
    editJson(candidate, value => {
      value.state = 'qualified';
      value.packs = value.packs.filter(pack => pack.id !== 'ecommerce.returns-pricing');
    });
    editJson(join(box.root, 'composition', 'promotions.json'), value => {
      value.entries.find(entry => entry.alias === 'L2' && entry.status === 'promoted').status = 'retired';
      value.entries.push({ alias: 'L2', status: 'promoted', recipe: {
        path: 'recipes/l2-standard-1.3.0.json', id: 'ecommerce.l2-standard', version: '1.3.0',
      } });
    });
    const track = loadTrack('ecommerce');
    const copiedTrack = { ...track, dir: box.root,
      suites: JSON.parse(readFileSync(join(box.root, 'track.json'), 'utf8')).suites };
    assert.throws(() => resolveRecipeRelease(copiedTrack, 2),
      /changes the cumulative L2 check set/);
  } finally { rmSync(box.temp, { recursive: true, force: true }); }
});

test('the grader rejects parent/child recipe drift before launching a browser', () => {
  const grader = join(import.meta.dirname, '..', 'grader', 'grade.mjs');
  const result = spawnSync(process.execPath, [grader,
    '--url', 'http://127.0.0.1:1',
    '--level', '1',
    '--track', 'ecommerce',
    '--spec', join(ECOMMERCE, 'scenarios', '01-features.json'),
    '--expected-recipe-sha256', '0'.repeat(64),
  ], { encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.match(`${result.stdout}${result.stderr}`, /recipe changed before grading/);
  assert.doesNotMatch(`${result.stdout}${result.stderr}`, /browser.*launch|ECONNREFUSED/i);
});

test('the grader rejects a stale selected check before launching a browser', () => {
  const grader = join(import.meta.dirname, '..', 'grader', 'grade.mjs');
  const result = spawnSync(process.execPath, [grader,
    '--url', 'http://127.0.0.1:1',
    '--level', '1',
    '--track', 'ecommerce',
    '--spec', join(ECOMMERCE, 'scenarios', '01-features.json'),
    '--selected-check', 'ecommerce.missing.check',
  ], { encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.match(`${result.stdout}${result.stderr}`, /no selected check/);
  assert.doesNotMatch(`${result.stdout}${result.stderr}`, /browser.*launch|ECONNREFUSED/i);
});
