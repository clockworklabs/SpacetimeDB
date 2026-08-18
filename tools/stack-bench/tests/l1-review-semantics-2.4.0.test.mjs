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
    '01-review-rating-live-2.4.0.json', '01-warehouse-admin-2.4.0.json']) {
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
  assert(checkout.some(step => step.do === 'reload'));
  assert(checkout.some(step => step.do === 'expectNumber'
    && step.testid === 'cart-count' && step.equals === 0));

  const warehouse = compileScenarioDefinition(readJson(join(root, 'scenarios',
    '01-warehouse-admin-2.4.0.json')));
  const liveRestock = warehouse.features[0].criteria.find(criterion => criterion.id === '7c');
  assert.deepEqual(liveRestock.steps[0], {
    do: 'click', actor: 'admin', testid: 'admin-link',
  });
});
