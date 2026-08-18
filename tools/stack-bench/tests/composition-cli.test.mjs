import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { diffRecipeFiles, selectRecipeRelease, validatePackFile,
  validateRecipeFile, showRecipeFile } from '../commands/composition-cli.mjs';
import { hashDirectory } from '../src/evidence/provenance.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';

const SOURCE = loadTrack('ecommerce').dir;
const CLI = join(import.meta.dirname, '..', 'commands', 'composition-cli.mjs');

test('pack and recipe validation resolve their full source context without writing', () => {
  const before = hashDirectory(SOURCE);
  const pack = validatePackFile(join(SOURCE, 'composition', 'packs', 'identity-access-1.0.0.json'),
    { trackRoot: SOURCE });
  assert.equal(pack.id, 'ecommerce.identity-access');
  assert(pack.criteria > 0);
  const recipe = validateRecipeFile(join(SOURCE, 'composition', 'recipes', 'l1-standard-1.0.0.json'),
    { trackRoot: SOURCE });
  assert.equal(recipe.release.checkCatalog.length, 48);
  assert.equal(recipe.release.checkCatalog.reduce((total, check) => total + check.points, 0), 51);
  assert.deepEqual(hashDirectory(SOURCE), before);
});

test('the command surface runs without Docker or PATH access', () => {
  const recipe = join(SOURCE, 'composition', 'recipes', 'smoke-1.0.0.json');
  const result = spawnSync(process.execPath,
    [CLI, 'recipe', 'validate', recipe, '--track-root', SOURCE, '--json'], {
      encoding: 'utf8', env: { ...process.env, PATH: '' },
    });
  assert.equal(result.status, 0, result.stderr);
  const output = JSON.parse(result.stdout);
  assert.equal(output.id, 'ecommerce.smoke');
  assert.equal(output.checks, 7);
});

test('recipe show gives pack/check selections their own stable scope identity', () => {
  const release = validateRecipeFile(
    join(SOURCE, 'composition', 'recipes', 'smoke-1.0.0.json'), { trackRoot: SOURCE }).release;
  const selected = selectRecipeRelease(release, { packIds: ['ecommerce.identity-access'] });
  assert.equal(selected.contentSha256, release.contentSha256);
  assert.equal(selected.selection.recipe.contentSha256, release.contentSha256);
  assert.equal(selected.selection.completeness, 'subset');
  assert.deepEqual(selected.selection.taskPacks, ['ecommerce.identity-access']);
  assert.match(selected.selection.sha256, /^[a-f0-9]{64}$/);
  assert(selected.checkCatalog.every(check => check.packId === 'ecommerce.identity-access'));
  assert.equal(selectRecipeRelease(release, { packIds: ['ecommerce.identity-access'] })
    .selection.sha256, selected.selection.sha256);
  const key = selected.checkCatalog[0].stableKey;
  const one = selectRecipeRelease(release, { checkKeys: [key] });
  assert.deepEqual(one.checkCatalog.map(check => check.stableKey), [key]);
  const outside = release.checkCatalog.find(check => check.packId !== 'ecommerce.identity-access');
  assert.throws(() => selectRecipeRelease(release, {
    packIds: ['ecommerce.identity-access'], checkKeys: [outside.stableKey],
  }), /unrequested pack/);
  assert.deepEqual(selectRecipeRelease(release).checkCatalog, release.checkCatalog);
  assert.equal(selectRecipeRelease(release).selection.completeness, 'full');
  assert.throws(() => selectRecipeRelease(release, { packIds: ['missing'] }), /no pack/);

  const shown = showRecipeFile(join(SOURCE, 'composition', 'recipes', 'l1-standard-1.0.0.json'), {
    trackRoot: SOURCE, packIds: ['ecommerce.identity-access'],
  });
  assert.notEqual(shown.builderTask.sha256, shown.task.composedSha256);
  assert.match(shown.builderTask.requirementText, /### Accounts/);
  assert.doesNotMatch(shown.builderTask.requirementText, /### Reviews/);
  assert.match(shown.builderTask.note, /Pack selection defines the requested task/);
});

test('recipe diff separates categories and names exact calibration work invalidated', () => {
  const temporary = mkdtempSync(join(tmpdir(), 'stack-bench-authoring-'));
  const root = join(temporary, 'ecommerce');
  try {
    cpSync(SOURCE, root, { recursive: true });
    const original = join(root, 'composition', 'recipes', 'l1-standard-1.0.0.json');
    const changed = join(root, 'composition', 'recipes', 'l1-fixture-variant.json');
    const value = JSON.parse(readFileSync(original, 'utf8'));
    value.state = 'draft';
    value.fixture = { path: '../fixtures/operations-1.0.0.json',
      id: 'ecommerce.operations', version: '1.0.0' };
    writeFileSync(changed, `${JSON.stringify(value, null, 2)}\n`);

    const diff = diffRecipeFiles(original, changed, { trackRoot: root });
    assert.deepEqual(diff.categories, {
      meaning: false,
      scoring: false,
      fixtures: true,
      execution: true,
      metadata: true,
    });
    assert.equal(diff.calibrations.length, 1);
    assert.equal(diff.calibrations[0].id, 'ecommerce.l1-standard-calibration');
    assert.deepEqual(diff.calibrations[0].invalidated, [
      'recipe binding', 'recipe qualification state', 'fixture binding',
      'reference repetitions', 'mutation repetitions', 'null repetitions', 'promotion decision',
    ]);
  } finally { rmSync(temporary, { recursive: true, force: true }); }
});

test('recipe diff names requirement and contract fragments changed by pack composition', () => {
  const temporary = mkdtempSync(join(tmpdir(), 'stack-bench-task-diff-'));
  const root = join(temporary, 'ecommerce');
  try {
    cpSync(SOURCE, root, { recursive: true });
    const original = join(root, 'composition', 'recipes', 'l1-standard-1.0.0.json');
    const changed = join(root, 'composition', 'recipes', 'l1-without-session.json');
    const value = JSON.parse(readFileSync(original, 'utf8'));
    value.packs = value.packs.filter(pack => pack.id !== 'ecommerce.session-durability');
    writeFileSync(changed, `${JSON.stringify(value, null, 2)}\n`);
    const diff = diffRecipeFiles(original, changed, { trackRoot: root });
    assert.deepEqual(diff.taskFragments.requirements.removed, ['ecommerce.l1.session-durability']);
    assert.deepEqual(diff.taskFragments.requirements.added, []);
    assert.equal(diff.taskFragments.composedTaskChanged, true);
    assert.equal(diff.categories.meaning, true);
  } finally { rmSync(temporary, { recursive: true, force: true }); }
});
