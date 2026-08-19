import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compileRecipeFile } from '../src/composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';
import { buildRecipeRelease } from '../src/composition/recipe-release.mjs';
import { selectScenarioChecks } from '../src/composition/recipe-selection.mjs';
import { TRACKS_DIR } from '../src/composition/tracks.mjs';

const root = join(TRACKS_DIR, 'ecommerce');
const recipe = version => join(root, 'composition', 'recipes', `l1-modular-${version}.json`);
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));

test('L1 2.4 preserves the published score surface while replacing review semantics', () => {
  const previous = compileRecipeFile(recipe('2.3.0'), { trackRoot: root });
  const candidate = compileRecipeFile(recipe('2.4.0'), { trackRoot: root });
  const scores = plan => new Map(plan.checks.map(check => [check.stableKey, check.points]));

  assert.deepEqual(scores(candidate), scores(previous));
  assert.equal(candidate.checks.length, 48);
  assert.equal(candidate.checks.reduce((total, check) => total + check.points, 0), 58);

  const reviewChecks = candidate.checks.filter(check => check.criterionId.startsWith('6'));
  assert.deepEqual(reviewChecks.map(check => [check.criterionId, check.source]), [
    ['6a', 'scenarios/01-review-visibility-2.4.0.json'],
    ['6b', 'scenarios/01-review-uniqueness-2.4.0.json'],
    ['6c', 'scenarios/01-review-rating-live-2.4.0.json'],
  ]);
});

test('each review check has an independent executable scenario', () => {
  const release = buildRecipeRelease(recipe('2.4.0'), { trackRoot: root });
  const selectionRelease = { checks: release.checkCatalog };
  for (const check of release.checkCatalog.filter(item =>
    ['6a', '6b', '6c'].includes(item.criterionId))) {
    const spec = compileScenarioDefinition(readJson(join(root, check.source)), {
      source: check.source,
    });
    const selected = selectScenarioChecks(spec, selectionRelease, [check.stableKey]);
    assert.equal(selected.features.length, 1);
    assert.equal(selected.features[0].criteria.length, 1);
    assert.equal(selected.features[0].criteria[0].id, check.criterionId);
    assert(selected.features[0].setup.length > 0);
  }
});

test('each account check has an independent executable scenario', () => {
  const release = buildRecipeRelease(recipe('2.4.0'), { trackRoot: root });
  const selectionRelease = { checks: release.checkCatalog };
  const checks = release.checkCatalog.filter(check => String(check.featureId) === '1');
  assert.equal(checks.length, 5);
  assert.equal(new Set(checks.map(check => check.source)).size, 5);
  for (const check of checks) {
    const spec = compileScenarioDefinition(readJson(join(root, check.source)), {
      source: check.source,
    });
    const selected = selectScenarioChecks(spec, selectionRelease, [check.stableKey]);
    assert.equal(selected.features.length, 1, check.stableKey);
    assert.deepEqual(selected.features[0].criteria.map(criterion => criterion.id),
      [check.criterionId], check.stableKey);
  }
  const creation = compileScenarioDefinition(readJson(join(root,
    'scenarios', '01-account-create-2.4.0.json')));
  assert.equal(creation.features[0].setup.length, 0);
  assert.equal(creation.features[0].criteria[0].steps[0].do, 'signUp');
});

test('review uniqueness grades the invariant without choosing reject or update', () => {
  const candidate = compileRecipeFile(recipe('2.4.0'), { trackRoot: root });
  const requirement = candidate.recipe.task.requirements.find(fragment =>
    fragment.id === 'ecommerce.spec.transactional-integrity.reviews').text;
  assert.match(requirement, /may update the existing review or be refused/);
  assert.doesNotMatch(requirement, /submission updates it/);

  const uniqueness = compileScenarioDefinition(readJson(join(root,
    'scenarios', '01-review-uniqueness-2.4.0.json')));
  const steps = uniqueness.features[0].criteria[0].steps;
  assert(steps.some(step => step.do === 'reload'));
  assert.deepEqual(steps.at(-1), {
    do: 'expect', actor: 'author', testid: 'review-item', count: 1, within: 10000,
  });
  assert(!steps.some(step => step.absent || step.testid === 'review-error'));
});

test('live rating uses two reviewers and does not inherit the repeat-submission outcome', () => {
  const rating = compileScenarioDefinition(readJson(join(root,
    'scenarios', '01-review-rating-live-2.4.0.json')));
  const feature = rating.features[0];
  assert.deepEqual(feature.actors, ['author', 'other']);
  const submitters = [...feature.setup, ...feature.criteria[0].steps]
    .filter(step => step.do === 'click' && step.testid === 'review-submit')
    .map(step => step.actor);
  assert.deepEqual(submitters, ['author', 'other']);
  assert(feature.criteria[0].steps.some(step => step.do === 'expectNumber'
    && step.testid === 'review-average' && step.equals === 3));
});

test('corrected purchasing, cart, review, and warehouse checks are independently selectable', () => {
  const release = buildRecipeRelease(recipe('2.4.0'), { trackRoot: root });
  const selectionRelease = { checks: release.checkCatalog };
  const corrected = release.checkCatalog.filter(check =>
    ['3', '4', '6', '7'].includes(String(check.featureId)));
  assert.equal(corrected.length, 14);

  for (const check of corrected) {
    const spec = compileScenarioDefinition(readJson(join(root, check.source)), {
      source: check.source,
    });
    const selected = selectScenarioChecks(spec, selectionRelease, [check.stableKey]);
    assert.equal(selected.features.length, 1, check.stableKey);
    assert.deepEqual(selected.features[0].criteria.map(criterion => criterion.id),
      [check.criterionId], check.stableKey);
  }
});

test('corrected workflows do not grade a particular panel-closing interaction', () => {
  for (const name of ['01-buying-2.4.0.json', '01-cart-2.4.0.json',
    '01-review-visibility-2.4.0.json', '01-review-uniqueness-2.4.0.json',
    '01-review-rating-live-2.4.0.json', '01-warehouse-admin-2.4.0.json',
    '01-warehouse-stock-live-2.4.0.json']) {
    const spec = compileScenarioDefinition(readJson(join(root, 'scenarios', name)));
    const actions = spec.features.flatMap(feature => [
      ...feature.setup,
      ...feature.criteria.flatMap(criterion => criterion.steps),
    ]);
    assert(!actions.some(action => action.do === 'pressKey'), name);
  }

  const cart = compileScenarioDefinition(readJson(join(root, 'scenarios',
    '01-cart-2.4.0.json')));
  const checkout = cart.features[0].criteria.find(criterion => criterion.id === '4d').steps;
  assert(!checkout.some(step => step.do === 'signIn'),
    'checkout must not duplicate the separate multi-session cart check');
  assert(!checkout.some(step => step.do === 'reload'),
    'checkout persistence must not duplicate the separate cart-reload check');
  assert(checkout.some(step => step.do === 'expectNumber'
    && step.actor === 'checkout' && step.testid === 'cart-count' && step.equals === 0));

  const warehouse = compileScenarioDefinition(readJson(join(root, 'scenarios',
    '01-warehouse-stock-live-2.4.0.json')));
  assert(warehouse.features[0].setup.some(step => step.do === 'click'
    && step.actor === 'admin' && step.testid === 'admin-link'));
  assert.equal(warehouse.features[0].criteria.length, 1);
  assert.equal(warehouse.features[0].criteria[0].id, '7c');
});

test('every scored L1 2.4 check is independently selectable with its numeric prerequisites', () => {
  const release = buildRecipeRelease(recipe('2.4.0'), { trackRoot: root });
  const selectionRelease = { checks: release.checkCatalog };
  assert.equal(release.checkCatalog.length, 48);

  for (const check of release.checkCatalog) {
    const spec = compileScenarioDefinition(readJson(join(root, check.source)), {
      source: check.source,
    });
    const selected = selectScenarioChecks(spec, selectionRelease, [check.stableKey]);
    assert.equal(selected.features.length, 1, check.stableKey);
    const feature = selected.features[0];
    assert.deepEqual(feature.criteria.map(criterion => criterion.id), [check.criterionId],
      check.stableKey);
    const recorded = new Set();
    for (const step of [...feature.setup, ...feature.criteria[0].steps]) {
      if (step.relativeTo) {
        assert(recorded.has(step.relativeTo),
          `${check.stableKey} reads ${step.relativeTo} before recording it`);
      }
      if (step.as) recorded.add(step.as);
    }
  }
});

test('catalog, order, warehouse, and direct-action claims use exact evidence', () => {
  const core = compileScenarioDefinition(readJson(join(root, 'scenarios', '01-core-2.4.0.json')));
  const catalog = core.features.find(feature => feature.id === 2);
  const catalogValues = catalog.criteria.find(criterion => criterion.id === '2a').steps;
  assert(catalogValues.some(step => step.do === 'fill' && step.actor === 'inspector'
    && step.text === 'Air Purifier'));
  assert(catalogValues.filter(step => step.do.startsWith('expect'))
    .every(step => step.actor === 'inspector'),
  'catalog values must use a separate view so ranking defects cannot hide the inspected item');
  assert.equal(catalog.criteria.find(criterion => criterion.id === '2b').steps[0].do,
    'expectSequence');
  assert.equal(catalog.criteria.find(criterion => criterion.id === '2c').steps[1].do,
    'expectSequence');

  const buying = compileScenarioDefinition(readJson(join(root, 'scenarios',
    '01-buying-2.4.0.json')));
  assert.equal(buying.features[0].criteria.find(criterion => criterion.id === '3c')
    .steps.at(-1).equals, 64);

  const cart = compileScenarioDefinition(readJson(join(root, 'scenarios', '01-cart-2.4.0.json')));
  assert.equal(cart.features[0].criteria.find(criterion => criterion.id === '4d')
    .steps.at(-1).count, 1);

  const warehouse = compileScenarioDefinition(readJson(join(root, 'scenarios',
    '01-warehouse-admin-2.4.0.json')));
  assert(warehouse.features[0].criteria.find(criterion => criterion.id === '7b').steps
    .some(step => step.testid === 'admin-location-row' && step.count === 24));

  for (const [featureId, source] of [[101, '01-purchase-session-2.4.0.json'],
    [103, '01-admin-write-2.4.0.json']]) {
    const direct = compileScenarioDefinition(readJson(join(root, 'scenarios', source)));
    const actions = direct.features.find(feature => feature.id === featureId).criteria[0].steps;
    assert(actions.some(step => step.do === 'expectActionOutcome' && step.outcome === 'accepted'));
    assert(actions.some(step => step.do === 'expectActionOutcome' && step.outcome === 'refused'));
  }
  const price = compileScenarioDefinition(readJson(join(root, 'scenarios',
    '01-server-price-2.4.0.json'))).features[0].criteria[0].steps;
  assert(price.some(step => step.testid === 'order-total' && step.equals === 449));
});

test('invariant checks own their setup and server actions', () => {
  const reload = compileScenarioDefinition(readJson(join(root, 'scenarios',
    '01-account-state-reload-2.4.0.json'))).features[0];
  const reconnect = compileScenarioDefinition(readJson(join(root, 'scenarios',
    '01-account-state-reconnect-2.4.0.json'))).features[0];
  assert(reload.setup.some(step => step.testid === 'add-to-cart'));
  assert(reconnect.setup.some(step => step.testid === 'add-to-cart'));

  const accounting = compileScenarioDefinition(readJson(join(root, 'scenarios',
    '01-books-balance-2.4.0.json'))).features[0];
  assert(accounting.setup.some(step => step.as === 'revenue-before'));
  assert(accounting.setup.some(step => step.as === 'stand-before'));

  const cart = compileScenarioDefinition(readJson(join(root, 'scenarios',
    '01-cart-boundary-2.4.0.json'))).features[0];
  assert(cart.setup.some(step => step.testid === 'add-to-cart'));
  const invalidQuantity = cart.criteria.find(criterion => criterion.id === '109b').steps;
  assert(invalidQuantity.some(step => step.do === 'callAction'
    && step.action === 'cart-set-quantity' && step.namedAction.method === 'PATCH'));
  assert(invalidQuantity.some(step => step.do === 'expectActionOutcome'
    && step.outcome === 'refused'));
  assert.equal(invalidQuantity.filter(step => step.relativeTo).length, 2);
});
