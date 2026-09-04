import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { checkCompositions } from '../commands/check-composition.js';
import { compileRecipeFile, compileRecipeSelectionFile }
  from '../src/composition/composition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const ecommerce = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const recipe = (name: string): string => join(ecommerce, 'composition', 'recipes', name);

test('the current ecommerce composition and sequential chain are valid', () => {
  const [report] = checkCompositions({ trackName: 'ecommerce' });
  assert.equal(report?.track, 'ecommerce');
  assert(report && report.packs > 0 && report.recipes === 4 && report.checks > 0);

  const l1 = compileRecipeFile(recipe('sequential-l1.json'), { trackRoot: ecommerce });
  const l2 = compileRecipeFile(recipe('sequential-l2.json'), { trackRoot: ecommerce });
  const l3 = compileRecipeFile(recipe('sequential-l3.json'), { trackRoot: ecommerce });
  assert.deepEqual([l1.recipe.sequence?.level, l2.recipe.sequence?.level,
    l3.recipe.sequence?.level], [1, 2, 3]);
  assert.deepEqual([l2.recipe.task.baseRecipe?.id, l3.recipe.task.baseRecipe?.id],
    [l1.recipe.id, l2.recipe.id]);
  assert(l2.checks.length > l1.checks.length);
  assert(l3.checks.length > l2.checks.length);

  const selection = compileRecipeSelectionFile(join(ecommerce, 'composition', 'sequential.json'), {
    trackRoot: ecommerce,
  });
  assert.deepEqual(selection.entries.map(entry => [entry.alias, entry.recipe.id]), [
    ['L1', 'ecommerce.sequential-l1'],
    ['L2', 'ecommerce.sequential-l2'],
    ['L3', 'ecommerce.sequential-l3'],
  ]);
});
