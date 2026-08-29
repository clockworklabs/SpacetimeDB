import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { compileRecipeFile } from '../src/composition/composition-compiler.js';
import { buildRecipeRelease } from '../dist/src/composition/recipe-release.js';
import { createBoundRecipeTaskRequest } from '../src/composition/recipe-selection.mjs';

const ecommerce = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const recipePath = name => join(ecommerce, 'composition', 'recipes', name);
const recipe = name => buildRecipeRelease(recipePath(name), { trackRoot: ecommerce });
const binding = name => ({ plan: compileRecipeFile(recipePath(name)), release: recipe(name) });

test('candidate recipes preserve the promoted score definitions', () => {
  const oldL1 = recipe('l1-modular-2.4.0.json');
  const newL1 = recipe('l1-modular-2.5.0.json');
  const oldL2 = recipe('l2-standard-1.5.0.json');
  const newL2 = recipe('l2-standard-1.6.0.json');

  for (const [before, after] of [[oldL1, newL1], [oldL2, newL2]]) {
    assert.equal(after.checkCatalog.length, before.checkCatalog.length);
    assert.equal(after.scoring.points, before.scoring.points);
    assert.deepEqual(after.scoring.weights, before.scoring.weights);
    assert.deepEqual(after.checkCatalog.map(check => check.stableKey),
      before.checkCatalog.map(check => check.stableKey));
  }
});

test('the catalogue readiness interface follows the catalogue feature', () => {
  const candidate = binding('l1-modular-2.5.0.json');
  const accounts = createBoundRecipeTaskRequest(candidate,
    { featureIds: ['ecommerce.feature.accounts'] });
  const catalogue = createBoundRecipeTaskRequest(candidate,
    { featureIds: ['ecommerce.feature.catalog'] });

  assert.doesNotMatch(accounts.task.contractText, /GET \/api\/items/);
  assert.match(catalogue.task.contractText, /GET \/api\/items/);
});

test('the neutral initial task does not prescribe data recovery', () => {
  const candidate = binding('l1-modular-2.5.0.json');
  const expectedSpecifications = candidate.release.components.packs
    .filter(pack => pack.moduleType === 'specification')
    .map(pack => `${pack.id}@${pack.version}`);
  const task = createBoundRecipeTaskRequest(candidate, { expectedSpecifications });

  assert.match(task.task.contractText, /GET \/api\/items/);
  assert.doesNotMatch(task.task.contractText,
    /reset|restored database|first install|durability|recovery/i);
  assert(task.selection.scoredChecks.some(check => check.treatment === 'expected'));
});
