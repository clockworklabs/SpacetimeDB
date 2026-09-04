import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import {
  buildRecipeRelease,
  bundleRecipeRelease,
  gradeRecipeRelease,
  requireRecipeRelease,
  resolveGradeRecipeArtifactBinding,
  resolveRecipeRelease,
  validateRecipeRequest,
} from '../src/composition/recipe-release.js';
import { compileTrackManifest } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { loadTrack, type Track } from '../src/composition/tracks.js';

const ECOMMERCE = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const L1_RECIPE = join(ECOMMERCE, 'composition', 'recipes', 'sequential-l1.json');
const L2_RECIPE = join(ECOMMERCE, 'composition', 'recipes', 'sequential-l2.json');

function copyTrack(): { temp: string; root: string; recipe: string } {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-release-'));
  const root = join(temp, 'ecommerce');
  cpSync(ECOMMERCE, root, { recursive: true });
  return { temp, root, recipe: join(root, 'composition', 'recipes', 'sequential-l1.json') };
}

function copiedTrack(track: Track, root: string): Track {
  const manifest: unknown = JSON.parse(readFileSync(join(root, 'track.json'), 'utf8'));
  const compiled = compileTrackManifest(manifest, { source: join(root, 'track.json') });
  return { ...track, dir: root, suites: compiled.suites };
}

test('recipe releases are deterministic and L2 binds the exact L1 content', () => {
  const l1a = buildRecipeRelease(L1_RECIPE, { trackRoot: ECOMMERCE });
  const l1b = buildRecipeRelease(L1_RECIPE, { trackRoot: ECOMMERCE });
  const l2 = buildRecipeRelease(L2_RECIPE, { trackRoot: ECOMMERCE });

  assert.deepEqual(l1a, l1b);
  assert.match(l1a.contentSha256, /^[a-f0-9]{64}$/);
  assert.equal(new Set(l1a.checkCatalog.map(check => check.stableKey)).size,
    l1a.checkCatalog.length);
  assert(l2.task.baseRecipe);
  assert.equal(l2.task.baseRecipe.contentSha256, l1a.contentSha256);
});

test('level selection uses stable IDs and may pin the compiled content', () => {
  const track = loadTrack('ecommerce');
  const selected = requireRecipeRelease(track, 1);
  const pinned = requireRecipeRelease(track, 1, {
    id: selected.release.id,
    contentSha256: selected.release.contentSha256,
  });

  assert.equal(selected.release.id, 'ecommerce.sequential-l1');
  assert.equal(pinned.release.contentSha256, selected.release.contentSha256);
  assert.throws(() => requireRecipeRelease(track, 1, {
    id: selected.release.id,
    contentSha256: '0'.repeat(64),
  }), /content changed/);
  assert.throws(() => validateRecipeRequest({ id: selected.release.id, unexpected: true }),
    /unknown field/);
});

test('a recipe binding emits only its selected grade artifact', () => {
  const track = loadTrack('ecommerce');
  const binding = requireRecipeRelease(track, 2);
  const execution = binding.execution.find(item => item.source === 'scenarios/02-features.json');
  assert(execution);

  const grade = gradeRecipeRelease(binding, execution.id);
  assert(grade);
  assert(grade.checks.length > 0);
  assert(grade.checks.every(check => check.executionId === execution.id));
  const artifact = resolveGradeRecipeArtifactBinding(track, 2,
    join(track.dir, 'scenarios', '02-features.json'));
  assert(artifact);
  assert.deepEqual(artifact.release, grade);
  const bundled = bundleRecipeRelease(binding);
  assert(bundled);
  assert.equal(bundled.selection.alias, 'L2');
});

test('a changed selected recipe cannot satisfy a pinned request', () => {
  const box = copyTrack();
  try {
    const track = copiedTrack(loadTrack('ecommerce'), box.root);
    const before = requireRecipeRelease(track, 1);
    const recipe = JSON.parse(readFileSync(box.recipe, 'utf8')) as {
      task: { framing: { requirements: Array<{ id: string }> } };
    };
    const requirement = recipe.task.framing.requirements[0];
    assert(requirement);
    requirement.id = `${requirement.id}.revised`;
    writeFileSync(box.recipe, `${JSON.stringify(recipe, null, 2)}\n`);

    assert.throws(() => resolveRecipeRelease(track, 1, {
      id: before.release.id,
      contentSha256: before.release.contentSha256,
    }), /content changed/);
  } finally {
    rmSync(box.temp, { recursive: true, force: true });
  }
});
