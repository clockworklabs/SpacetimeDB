import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { diffRecipeFiles, selectRecipeRelease, validatePackFile,
  validateRecipeFile, showRecipeFile } from '../commands/composition-cli.js';
import { hashDirectory } from '../src/evidence/provenance.js';
import { loadTrack } from '../src/composition/tracks.js';
import { compiledEntrypoint } from '../src/package-root.js';

const SOURCE = loadTrack('ecommerce').dir;
const CLI = compiledEntrypoint('commands', 'composition-cli.js');

interface MutableRecipeFile {
  state: string;
  fixture: { path: string; id: string; version: string };
  packs: Array<{ id: string; [key: string]: unknown }>;
  [key: string]: unknown;
}

test('pack and recipe validation resolve their full source context without writing', () => {
  const before = hashDirectory(SOURCE);
  const pack = validatePackFile(join(SOURCE, 'composition', 'packs', 'feature-accounts-1.1.0.json'),
    { trackRoot: SOURCE });
  assert.equal(pack.id, 'ecommerce.feature.accounts');
  assert(pack.criteria > 0);
  const recipe = validateRecipeFile(join(SOURCE, 'composition', 'recipes', 'sequential-l1-2.5.0.json'),
    { trackRoot: SOURCE });
  assert.equal(recipe.release.checkCatalog.length, 48);
  assert.equal(recipe.release.checkCatalog.reduce((total, check) => total + check.points, 0), 58);
  assert.deepEqual(hashDirectory(SOURCE), before);
});

test('the command surface runs without Docker or PATH access', () => {
  const recipe = join(SOURCE, 'composition', 'recipes', 'sequential-l1-2.5.0.json');
  const result = spawnSync(process.execPath,
    [CLI, 'recipe', 'validate', recipe, '--track-root', SOURCE, '--json'], {
      encoding: 'utf8', env: { ...process.env, PATH: '' },
    });
  assert.equal(result.status, 0, result.stderr);
  const output = JSON.parse(result.stdout);
  assert.equal(output.id, 'ecommerce.sequential-l1');
  assert.equal(output.checks, 48);
});

test('recipe show gives pack/check selections their own stable scope identity', () => {
  const release = validateRecipeFile(
    join(SOURCE, 'composition', 'recipes', 'sequential-l1-2.5.0.json'), { trackRoot: SOURCE }).release;
  const selected = selectRecipeRelease(release, { packIds: ['ecommerce.feature.accounts'] });
  assert.equal(selected.contentSha256, release.contentSha256);
  assert.equal(selected.selection.recipe.contentSha256, release.contentSha256);
  assert.equal(selected.selection.completeness, 'subset');
  assert.deepEqual(selected.selection.taskPacks, ['ecommerce.feature.accounts']);
  assert.match(selected.selection.sha256, /^[a-f0-9]{64}$/);
  assert(selected.checkCatalog.every(check => check.packId === 'ecommerce.feature.accounts'));
  assert.equal(selectRecipeRelease(release, { packIds: ['ecommerce.feature.accounts'] })
    .selection.sha256, selected.selection.sha256);
  const firstCheck = selected.checkCatalog[0];
  assert.ok(firstCheck);
  const key = firstCheck.stableKey;
  const one = selectRecipeRelease(release, { checkKeys: [key] });
  assert.deepEqual(one.checkCatalog.map(check => check.stableKey), [key]);
  const outside = release.checkCatalog.find(check => check.packId !== 'ecommerce.feature.accounts');
  assert.ok(outside);
  assert.throws(() => selectRecipeRelease(release, {
    packIds: ['ecommerce.feature.accounts'], checkKeys: [outside.stableKey],
  }), /unrequested pack/);
  const full = selectRecipeRelease(release);
  assert.equal(full.selection.completeness, 'full');
  assert(full.checkCatalog.every(check => check.points > 0));
  assert.throws(() => selectRecipeRelease(release, { packIds: ['missing'] }), /no pack/);

  const shown = showRecipeFile(join(SOURCE, 'composition', 'recipes', 'sequential-l1-2.5.0.json'), {
    trackRoot: SOURCE, packIds: ['ecommerce.feature.accounts'],
  });
  assert.notEqual(shown.builderTask.sha256, shown.task.composedSha256);
  assert.match(shown.builderTask.requirementText, /Accounts/);
  assert.doesNotMatch(shown.builderTask.requirementText, /Catalog/);
  assert.match(shown.builderTask.note, /Pack selection defines the requested task/);
});

test('recipe diff separates categories and names exact calibration work invalidated', () => {
  const temporary = mkdtempSync(join(tmpdir(), 'stack-bench-authoring-'));
  const root = join(temporary, 'ecommerce');
  try {
    cpSync(SOURCE, root, { recursive: true });
    const original = join(root, 'composition', 'recipes', 'sequential-l1-2.5.0.json');
    const changed = join(root, 'composition', 'recipes', 'l1-fixture-variant.json');
    const value = JSON.parse(readFileSync(original, 'utf8')) as MutableRecipeFile;
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
    const calibration = diff.calibrations[0];
    assert.ok(calibration);
    assert.equal(calibration.id, 'ecommerce.sequential-l1-calibration');
    assert.deepEqual(calibration.invalidated, [
      'recipe binding', 'fixture binding',
      'reference repetitions', 'mutation repetitions', 'null repetitions', 'promotion decision',
    ]);
  } finally { rmSync(temporary, { recursive: true, force: true }); }
});

test('recipe diff names changed task fragments', () => {
  const temporary = mkdtempSync(join(tmpdir(), 'stack-bench-task-diff-'));
  const root = join(temporary, 'ecommerce');
  try {
    cpSync(SOURCE, root, { recursive: true });
    const original = join(root, 'composition', 'recipes', 'sequential-l1-2.5.0.json');
    const changed = join(root, 'composition', 'recipes', 'l1-renamed-framing.json');
    const value = JSON.parse(readFileSync(original, 'utf8')) as MutableRecipeFile;
    const task = value.task as { framing: { requirements: Array<{ id: string; path: string }> } };
    task.framing.requirements[0]!.id = 'ecommerce.sequential-l1.renamed-framing';
    task.framing.requirements[0]!.path = 'prompts/modular/l1-features.md';
    writeFileSync(changed, `${JSON.stringify(value, null, 2)}\n`);
    const diff = diffRecipeFiles(original, changed, { trackRoot: root });
    assert.deepEqual(diff.taskFragments.requirements.removed, ['ecommerce.sequential-l1.framing']);
    assert.deepEqual(diff.taskFragments.requirements.added, ['ecommerce.sequential-l1.renamed-framing']);
    assert.equal(diff.taskFragments.composedTaskChanged, true);
    assert.equal(diff.categories.meaning, true);
  } finally { rmSync(temporary, { recursive: true, force: true }); }
});
