import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { compileRecipeFile } from '../composition-compiler.mjs';
import { buildRecipeRelease } from '../recipe-release.mjs';
import { createModularRecipeTaskRequest, resolveModularRecipeSelection } from '../recipe-selection.mjs';

const TRACK = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const recipe = name => join(TRACK, 'composition', 'recipes', name);
const CURRENT = recipe('l2-standard-1.4.0.json');

const l2Prefixes = [
  'ecommerce.operations-access.',
  'ecommerce.inventory-operations.',
  'ecommerce.returns-pricing.',
];
const isL2 = check => l2Prefixes.some(prefix => check.stableKey.startsWith(prefix));

function check(plan, stableKey) {
  const found = plan.checks.find(candidate => candidate.stableKey === stableKey);
  assert(found, `missing ${stableKey}`);
  return found;
}

function steps(plan, stableKey) {
  const target = check(plan, stableKey);
  const execution = plan.execution.find(candidate => candidate.source === target.source);
  const group = execution.checkGroups.find(candidate =>
    (candidate.stablePackId ?? candidate.packId) === target.stablePackId
      && candidate.checkGroupId === target.checkGroupId
      && candidate.feature.criteria.some(criterion => criterion.id === target.criterionId));
  const criterion = group.feature.criteria.find(candidate => candidate.id === target.criterionId);
  return { feature: group.feature, criterion, steps: criterion.steps };
}

function visitSteps(actions, visit) {
  for (const action of actions) {
    visit(action);
    if (action.do === 'race') action.branches.forEach(branch => visitSteps(branch, visit));
  }
}

test('L2 1.4 carries the exact promoted L1 2.3 contract and the exact published L2 key set', () => {
  const base = compileRecipeFile(recipe('l1-modular-2.3.0.json'), { trackRoot: TRACK });
  const previous = compileRecipeFile(recipe('l2-standard-1.3.0.json'), { trackRoot: TRACK });
  const current = compileRecipeFile(CURRENT, { trackRoot: TRACK });

  assert.deepEqual(current.recipe.task.baseRecipe, {
    id: 'ecommerce.l1-modular', version: '2.3.0', path: 'recipes/l1-modular-2.3.0.json',
  });
  assert.deepEqual(current.checks.slice(0, base.checks.length), base.checks);

  const beforeL2 = previous.checks.filter(isL2);
  const afterL2 = current.checks.filter(isL2);
  assert.deepEqual(afterL2.map(item => [item.stableKey, item.points]).sort(),
    beforeL2.map(item => [item.stableKey, item.points]).sort());
  assert.deepEqual({ checks: current.checks.length, points: current.scoring.points,
    l1Checks: base.checks.length, l1Points: base.scoring.points,
    l2Checks: afterL2.length, l2Points: afterL2.reduce((sum, item) => sum + item.points, 0),
    zeroPointControls: current.checks.filter(item => item.points === 0).length }, {
    checks: 76, points: 117, l1Checks: 48, l1Points: 58,
    l2Checks: 28, l2Points: 59, zeroPointControls: 2,
  });
});

test('the six L2 modules are typed and retain the old scoring namespaces', () => {
  const release = buildRecipeRelease(CURRENT, { trackRoot: TRACK });
  const modules = release.components.packs.filter(module => module.id.includes('operations-access')
    || module.id.includes('inventory-operations') || module.id.includes('returns-pricing'));
  assert.equal(modules.length, 6);
  assert.deepEqual(modules.map(module => module.moduleType).sort(),
    ['feature', 'feature', 'feature', 'specification', 'specification', 'specification']);
  assert.deepEqual(new Set(modules.map(module => module.stableId)), new Set([
    'ecommerce.operations-access', 'ecommerce.inventory-operations', 'ecommerce.returns-pricing',
  ]));
  assert(release.checkCatalog.filter(isL2).every(item =>
    item.stableKey.startsWith('ecommerce.operations-access.')
      || item.stableKey.startsWith('ecommerce.inventory-operations.')
      || item.stableKey.startsWith('ecommerce.returns-pricing.')));
});

test('every L2 criterion owns each recorded value that it reads', () => {
  const plan = compileRecipeFile(CURRENT, { trackRoot: TRACK });
  for (const target of plan.checks.filter(isL2)) {
    const selected = steps(plan, target.stableKey);
    const available = new Set();
    visitSteps(selected.feature.setup, action => {
      if (action.do === 'recordNumber') available.add(action.as);
    });
    visitSteps(selected.criterion.steps, action => {
      if (action.do === 'expectNumber' && action.relativeTo !== undefined) {
        assert(available.has(action.relativeTo),
          `${target.stableKey} reads ${action.relativeTo} without recording it`);
      }
      if (action.do === 'recordNumber') available.add(action.as);
    });
  }
});

test('the low-stock boundary is isolated from the other operational views', () => {
  const plan = compileRecipeFile(CURRENT, { trackRoot: TRACK });
  const lowStock = check(plan, 'ecommerce.inventory-operations.operational-views.5a');
  const categoryTotals = check(plan, 'ecommerce.inventory-operations.operational-views.5b');
  assert.equal(lowStock.source, 'scenarios/02-low-stock-1.4.0.json');
  assert.equal(categoryTotals.source, 'scenarios/02-features.json');
  assert.equal(steps(plan, lowStock.stableKey).feature.criteria.length, 1);
});

test('corrected transfer checks assert source, destination, and exact total behavior', () => {
  const plan = compileRecipeFile(CURRENT, { trackRoot: TRACK });
  for (const key of [
    'ecommerce.inventory-operations.warehouse-transfer.2a',
    'ecommerce.inventory-operations.stock-conservation.202a',
  ]) {
    const actions = steps(plan, key).steps;
    const expectations = actions.filter(action => action.do === 'expectNumber');
    assert(expectations.some(action => action.testid === 'warehouse-total'
      && action.in?.contains === 'East' && action.plus < 0), `${key} must decrease East`);
    assert(expectations.some(action => action.testid === 'warehouse-total'
      && action.in?.contains === 'West' && action.plus > 0), `${key} must increase West`);
    assert(expectations.some(action => action.testid === 'item-stock' && action.plus === 0),
      `${key} must preserve the item total`);
  }

  const refused = steps(plan, 'ecommerce.inventory-operations.warehouse-transfer.2c').steps
    .filter(action => action.do === 'expectNumber');
  assert.deepEqual(refused.map(action => [action.testid, action.in?.contains, action.plus]), [
    ['warehouse-total', 'East', 0], ['warehouse-total', 'West', 0], ['item-stock', 'Headphones', 0],
  ]);

  const cancelled = check(plan, 'ecommerce.inventory-operations.stock-conservation.202b');
  assert.equal(cancelled.source, 'scenarios/02-self-contained-1.4.0.json');
  const cancellationSteps = steps(plan, cancelled.stableKey).steps;
  assert(cancellationSteps.some(action => action.do === 'recordNumber'
    && action.as === 'cancel-stock-before'));
  assert(cancellationSteps.some(action => action.do === 'expectNumber'
    && action.relativeTo === 'cancel-stock-before' && action.plus === -1));
  assert(cancellationSteps.some(action => action.do === 'expectNumber'
    && action.relativeTo === 'cancel-stock-before' && action.plus === 0));
});

test('price, return, ranking, and authorization corrections prove both sides of the behavior', () => {
  const plan = compileRecipeFile(CURRENT, { trackRoot: TRACK });

  const history = steps(plan, 'ecommerce.returns-pricing.price-history.4a').steps;
  assert(history.some(action => action.do === 'recordNumber' && action.as === 'air-purifier-paid'));
  assert(history.some(action => action.do === 'expectNumber' && action.testid === 'item-price'
    && action.equals === 1));
  assert(history.some(action => action.do === 'expectNumber' && action.testid === 'order-total'
    && action.relativeTo === 'air-purifier-paid' && action.plus === 0));

  const returned = steps(plan, 'ecommerce.returns-pricing.cancellation-and-return.3c').steps;
  assert(returned.some(action => action.do === 'expectElementCount'
    && action.testid === 'order-item' && action.contains === 'Keyboard' && action.equals === 1));
  assert(returned.some(action => action.do === 'expect' && action.testid === 'order-item'
    && action.contains === 'eturned'));
  assert(!returned.some(action => action.do === 'expect' && action.testid === 'order-status'
    && /return/i.test(action.contains ?? '')));

  const warehouse = steps(plan, 'ecommerce.operations-access.fulfilment-queue.1b').steps;
  assert.deepEqual(warehouse[0], {
    do: 'dbSetStock', item: 'Desk Lamp', warehouse: 'West', quantity: 0, settleMs: 2000,
  });
  assert(warehouse.some(action => action.do === 'expect' && action.testid === 'queue-warehouse'
    && action.in?.testid === 'queue-item' && action.in.contains === 'Desk Lamp'
    && action.contains === 'East' && action.nonEmpty === undefined));

  const bestSellers = steps(plan, 'ecommerce.inventory-operations.operational-views.5d').steps;
  for (const item of ['Webcam', 'Coffee Grinder']) {
    const present = bestSellers.findIndex(action => action.do === 'expect'
      && action.testid === 'recommended-item' && action.contains === item && !action.absent);
    const absent = bestSellers.findIndex(action => action.do === 'expect'
      && action.testid === 'recommended-item' && action.contains === item && action.absent);
    assert(present >= 0 && absent > present, `${item} must be observed before exclusion is asserted`);
  }
  assert(bestSellers.some(action => action.do === 'expect'
    && action.testid === 'recommended-item' && action.contains === 'Desk Lamp' && !action.absent));

  for (const key of [
    'ecommerce.operations-access.fulfilment-queue.1e',
    'ecommerce.operations-access.operator-authorization.201a',
    'ecommerce.operations-access.operator-authorization.201b',
    'ecommerce.operations-access.operator-authorization.201c',
    'ecommerce.operations-access.order-owner.204a',
  ]) {
    const actions = steps(plan, key).steps;
    assert(actions.some(action => action.do === 'callAction'), `${key} must call the server directly`);
    assert(actions.some(action => action.do === 'expectActionOutcome' && action.outcome === 'refused'),
      `${key} must prove deliberate server refusal`);
    assert(!actions.some(action => action.do === 'replayAs'), `${key} must be cross-stack, not HTTP-only`);
  }
  assert(steps(plan, 'ecommerce.operations-access.operator-authorization.201b').steps
    .some(action => action.do === 'callAction' && action.input.attribute === 'data-price-input'));
});

test('expected specifications stay out of the prompt while feature-owned machine handles remain', () => {
  const plan = compileRecipeFile(CURRENT, { trackRoot: TRACK });
  const release = buildRecipeRelease(CURRENT, { trackRoot: TRACK });
  const requested = {
    featureIds: ['ecommerce.inventory-operations-features'],
    expectedSpecifications: [
      'ecommerce.operations-access-specifications@1.0.0',
      'ecommerce.inventory-operations-specifications@1.0.0',
      'ecommerce.returns-pricing-specifications@1.0.0',
    ],
  };
  const selection = resolveModularRecipeSelection(release, requested);
  const task = createModularRecipeTaskRequest({ release, plan }, requested);

  assert.deepEqual(selection.features, [
    'ecommerce.inventory-operations-features',
    'ecommerce.operations-access-features',
    'ecommerce.returns-pricing-features',
  ]);
  assert(selection.scoredChecks.some(item =>
    item.stableKey === 'ecommerce.operations-access.operator-authorization.201a'));
  assert(selection.scoredChecks.some(item =>
    item.stableKey === 'ecommerce.operations-access.operator-authorization.201b'));
  assert.doesNotMatch(task.task.requirementText, /Staff cannot change prices/);
  assert.doesNotMatch(task.task.requirementText, /staff-only authorization/);
  assert.doesNotMatch(task.task.requirementText, /An item's stock is always/);
  assert.doesNotMatch(task.task.requirementText, /Revenue always equals/);
  assert.match(task.task.contractText, /data-ship-input/);
  assert.match(task.task.contractText, /data-price-input/);
  assert.match(task.task.contractText, /data-transfer-input/);
  assert.doesNotMatch(task.task.contractText, /`returned-item`/);
});

test('a modular subset never exposes a specification check whose feature is absent', () => {
  const release = buildRecipeRelease(CURRENT, { trackRoot: TRACK });
  const selection = resolveModularRecipeSelection(release, {
    featureIds: ['ecommerce.operations-access-features'],
    expectedSpecifications: ['ecommerce.operations-access-specifications@1.0.0'],
  });
  const keys = selection.scoredChecks.map(item => item.stableKey);
  assert(keys.includes('ecommerce.operations-access.operator-authorization.201c'));
  assert(keys.includes('ecommerce.operations-access.order-owner.204a'));
  assert(!keys.includes('ecommerce.operations-access.operator-authorization.201a'),
    'transfer authorization requires inventory operations');
  assert(!keys.includes('ecommerce.operations-access.operator-authorization.201b'),
    'price authorization requires returns and pricing');
});
