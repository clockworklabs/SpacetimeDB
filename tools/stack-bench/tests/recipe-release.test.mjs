import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import {
  buildRecipeRelease,
  bundleRecipeRelease,
  gradeRecipeRelease,
  resolveGradeRecipeArtifactBinding,
  resolveGradeRecipeRelease,
  resolveLegacyRecipeRelease,
} from '../recipe-release.mjs';
import { loadTrack } from '../tracks.mjs';

const ECOMMERCE = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const L1_RECIPE = join(ECOMMERCE, 'composition', 'recipes', 'l1-standard-1.0.0.json');

function copyTrack() {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-release-'));
  const root = join(temp, 'ecommerce');
  cpSync(ECOMMERCE, root, { recursive: true });
  return { temp, root, recipe: join(root, 'composition', 'recipes', 'l1-standard-1.0.0.json') };
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
    join(ECOMMERCE, 'composition', 'recipes', 'l2-standard-1.1.0.json'),
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
  const binding = resolveLegacyRecipeRelease(track, 2);
  assert.equal(binding.alias, 'L2');
  assert.equal(binding.status, 'candidate');
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
    assert.throws(() => resolveLegacyRecipeRelease(copiedTrack, 1), /does not exactly match/);
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
