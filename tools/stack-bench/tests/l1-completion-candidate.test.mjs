import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { compileRecipeFile } from '../src/composition/composition-compiler.mjs';
import { buildRecipeRelease } from '../src/composition/recipe-release.mjs';

const recipePath = join(import.meta.dirname, '..', 'tracks', 'ecommerce', 'composition',
  'recipes', 'l1-modular-2.2.0.json');
const plan = compileRecipeFile(recipePath);
const release = buildRecipeRelease(recipePath);

function criterion(id) {
  for (const execution of plan.execution) {
    for (const group of execution.checkGroups) {
      const match = group.feature.criteria.find(candidate => candidate.id === id);
      if (match) return { execution, group, criterion: match };
    }
  }
  throw new Error(`missing criterion ${id}`);
}

test('the L1 completion candidate preserves every check and promotes only proven order counting', () => {
  assert.equal(plan.checks.length, 48);
  assert.equal(plan.scoring.points, 52);
  assert.equal(release.checkCatalog.filter(check => check.points === 0).length, 8);

  const orderCount = release.checkCatalog.find(check => check.criterionId === '201b');
  assert.equal(orderCount.stableKey,
    'ecommerce.spec.concurrency-safety.last-unit.201b');
  assert.equal(orderCount.points, 1);
  assert.equal(orderCount.executionId, 'last-unit');

  for (const id of ['201c', '202-control', '202a', '203a', '901a', '901b', '901d', '902a']) {
    assert.equal(release.checkCatalog.find(check => check.criterionId === id).points, 0,
      `${id} must remain evidence-gated`);
  }
});

test('the revenue check owns an admin baseline and an executable exact assertion', () => {
  const { group, criterion: revenue } = criterion('201c');
  assert.equal(group.feature.actors.includes('admin'), true);
  assert.deepEqual(group.feature.setup.slice(0, 3).map(step => step.do),
    ['signIn', 'click', 'recordNumber']);
  assert.equal(group.feature.setup[2].as, 'revenue-before-last-unit');
  assert.deepEqual(revenue.steps, [{
    do: 'expectNumber',
    actor: 'admin',
    testid: 'admin-revenue',
    relativeTo: 'revenue-before-last-unit',
    plus: 3897,
    within: 10000,
  }]);
});

test('the focused suite keeps the stock, order, and revenue consequences together', () => {
  const lastUnit = plan.execution.find(execution => execution.id === 'last-unit');
  assert.equal(lastUnit.source, 'scenarios/01-last-unit-2.2.0.json');
  assert.deepEqual(lastUnit.checkGroups[0].feature.criteria.map(item => item.id),
    ['201a', '201b', '201c']);
  assert.equal(plan.execution.find(execution => execution.id === 'restock-race')
    .checkGroups[0].feature.id, 202);
});

test('the restock candidate actually overlaps the purchases and restock', () => {
  const { criterion: restock } = criterion('202a');
  const overlap = restock.steps.find(step => step.do === 'race');
  assert(overlap);
  assert.deepEqual(overlap.branches.map(branch => branch.map(step => step.do)), [
    ['clickConcurrently'],
    ['fill', 'click'],
  ]);
});
