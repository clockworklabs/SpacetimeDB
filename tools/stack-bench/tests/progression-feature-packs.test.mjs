import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const readPack = name => compilePackDefinition(readJson(join(packRoot, name)), { source: name });

const splitNames = [
  'l2-stock-transfers-features-1.0.0.json',
  'l2-operational-views-features-1.0.0.json',
  'l2-order-cancellation-features-1.0.0.json',
  'l3-order-returns-features-1.1.0.json',
  'l2-price-history-features-1.0.0.json',
];

function selectedChecks(pack) {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
    });
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    const criteria = check.criteria === undefined
      ? feature.criteria
      : check.criteria.map(id => feature.criteria.find(criterion => criterion.id === id));
    return criteria.map(criterion => ({
      key: `${pack.stableId ?? pack.id}.${check.stableId ?? check.id}.${criterion.id}`,
      points: criterion.points ?? 1,
    }));
  }).sort((left, right) => left.key.localeCompare(right.key));
}

function fragmentText(fragment) {
  const source = readFileSync(join(trackRoot, fragment.path), 'utf8');
  const start = fragment.from ? source.indexOf(fragment.from) : 0;
  const end = fragment.until ? source.indexOf(fragment.until, start + 1) : source.length;
  assert(start >= 0 && end > start, `${fragment.id} must resolve to text`);
  return source.slice(start, end);
}

test('split L2 feature packs preserve every established feature point', () => {
  const split = [...splitNames, 'progression-cancellation-queue-specifications-1.0.0.json']
    .flatMap(name => selectedChecks(readPack(name)));
  const established = [
    'inventory-operations-features-1.2.0.json',
    'returns-pricing-features-1.1.0.json',
  ].flatMap(name => selectedChecks(readPack(name)))
    .sort((left, right) => left.key.localeCompare(right.key));
  const totalByFamily = checks => checks.reduce((totals, check) => {
    const family = check.key === 'ecommerce.returns-pricing.cancellation-and-return.3d'
      ? 'ecommerce.returns-pricing.cancellation-and-return.3a'
      : check.key;
    totals.set(family, (totals.get(family) ?? 0) + check.points);
    return totals;
  }, new Map());
  const splitTotals = totalByFamily(split);
  for (const [key, points] of totalByFamily(established)) {
    assert.equal(splitTotals.get(key), points, `${key} must preserve its established points`);
  }
  assert.equal(splitTotals.get('ecommerce.returns-pricing.cancellation-and-return.3e'), 1);
});

test('each split pack owns only its prompt and exact dependencies', () => {
  const packs = Object.fromEntries(splitNames.map(name => {
    const pack = readPack(name);
    assert.equal(pack.state, 'draft');
    assert.equal(pack.moduleType, 'feature');
    return [pack.id, pack];
  }));

  const cancellation = packs['ecommerce.l2.order-cancellation-features'];
  const returns = packs['ecommerce.l3.order-returns-features'];
  const pricing = packs['ecommerce.l2.price-history-features'];
  const transfers = packs['ecommerce.l2.stock-transfers-features'];
  const views = packs['ecommerce.l2.operational-views-features'];

  assert.deepEqual(cancellation.requiresPacks,
    ['ecommerce.feature.purchasing@1.2.0', 'ecommerce.feature.warehouse-admin@1.2.0']);
  assert.deepEqual(returns.requiresPacks,
    ['ecommerce.l3.order-delivery-features@1.1.0', 'ecommerce.feature.warehouse-admin@1.2.0']);
  assert.deepEqual(pricing.requiresPacks,
    ['ecommerce.feature.cart-checkout@1.3.0', 'ecommerce.feature.purchasing@1.2.0',
      'ecommerce.progression.catalog-management@1.0.0']);
  assert.deepEqual(transfers.requiresPacks, ['ecommerce.feature.warehouse-admin@1.2.0']);
  assert.deepEqual(views.requiresPacks,
    ['ecommerce.feature.cart-checkout@1.3.0', 'ecommerce.feature.purchasing@1.2.0',
      'ecommerce.feature.warehouse-admin@1.2.0']);

  assert.doesNotMatch(fragmentText(cancellation.task.requirements[0]), /return|price|Live operational views/i);
  assert.doesNotMatch(fragmentText(returns.task.requirements[0]), /cancel|Live operational views/i);
  assert.doesNotMatch(fragmentText(pricing.task.requirements[0]), /Cancelling and returning|Live operational views/);
  assert.doesNotMatch(fragmentText(transfers.task.requirements[0]), /Cancelling and returning|Live operational views/);
  assert.doesNotMatch(fragmentText(views.task.requirements[0]), /cancel|return|price/i);
  assert.doesNotMatch(fragmentText(views.task.requirements[0]),
    /warehouse utilisation|fulfilment queue depth/i);
});

test('dependency-owned checks use only interfaces supplied by their parents', () => {
  const warehouse = readPack('feature-warehouse-admin-1.2.0.json');
  assert.deepEqual(warehouse.requiresPacks,
    ['ecommerce.feature.catalog@1.2.0', 'ecommerce.progression.staff-access@1.0.0']);
  assert.deepEqual(warehouse.checks.map(check => [check.id, check.criteria]), [
    ['access-boundary', ['7a']], ['warehouse-view', ['7b']], ['warehouse-stock', ['7c']],
    ['admin-write', ['103a']],
  ]);
  for (const check of warehouse.checks) {
    const scenario = readJson(join(trackRoot, check.source));
    const setup = scenario.features.find(feature => feature.id === check.feature).setup;
    assert(setup.some(step => step.testid === 'staff-signin-submit'));
    assert.equal(setup.some(step => step.do === 'signIn' || step.do === 'signUp'), false);
  }

  const cancellation = readPack('l2-order-cancellation-features-1.0.0.json');
  assert(cancellation.checks.find(check => check.id === 'cancellation-core').source
    .endsWith('02-order-cancellation-core-1.0.0.json'));
  const cancellationScenario = readJson(join(trackRoot,
    cancellation.checks.find(check => check.id === 'cancellation-core').source));
  assert.equal(JSON.stringify(cancellationScenario).includes('queue-item'), false);

  const queue = readPack('progression-cancellation-queue-specifications-1.0.0.json');
  assert.deepEqual(queue.requiresPacks, []);
  assert.deepEqual(queue.task.requirements[0].requiresFeatures,
    ['ecommerce.l2.order-cancellation-features', 'ecommerce.operations-access-features']);
  assert.deepEqual(queue.checks.map(check => [check.id, check.criteria]), [
    ['queue-removal', ['3d']],
  ]);

  const staffScenario = readJson(join(trackRoot, 'scenarios',
    'progression-core-business-1.0.0.json'));
  const activitySetup = staffScenario.features.find(feature => feature.id === 624).setup;
  assert(activitySetup.some(step => step.testid === 'catalog-save'));
  assert.equal(activitySetup.some(step => step.testid === 'price-submit'), false);
});

test('catalog and faceted search expose only their own product work', () => {
  const catalog = readPack('feature-catalog-1.2.0.json');
  const search = readPack('progression-faceted-search-1.0.0.json');
  assert.equal(catalog.state, 'draft');
  assert.equal(search.state, 'draft');
  assert.deepEqual(catalog.requiresPacks, []);
  assert.deepEqual(search.requiresPacks, ['ecommerce.feature.catalog@1.1.0']);
  for (const fragment of [...catalog.task.requirements, ...catalog.task.contracts]) {
    assert.deepEqual(fragment.modes, ['fresh', 'upgrade']);
  }
  assert.deepEqual(catalog.checks.map(check => [check.id, check.criteria]), [
    ['values', ['2a']], ['ranking', ['2b']], ['search', ['2d']],
  ]);
  const catalogPrompt = fragmentText(catalog.task.requirements[0]);
  assert.match(catalogPrompt, /price.*total stock/i);
  assert.doesNotMatch(catalogPrompt, /review|rating|warehouse|framework|database/i);
  const searchPrompt = fragmentText(search.task.requirements[0]);
  assert.match(searchPrompt, /category, price range, and availability/i);
  assert.doesNotMatch(searchPrompt, /warehouse|admin|framework|database/i);
});

test('support nodes have isolated prompts, hooks, scenarios, and exact dependencies', () => {
  const intake = readPack('progression-support-intake-1.0.0.json');
  const triage = readPack('progression-support-triage-1.0.0.json');
  const history = readPack('progression-support-history-1.0.0.json');
  for (const pack of [intake, triage, history]) assert.equal(pack.state, 'draft');
  assert.deepEqual(intake.requiresPacks, []);
  for (const fragment of [...intake.task.requirements, ...intake.task.contracts]) {
    assert.deepEqual(fragment.modes, ['fresh', 'upgrade']);
  }
  assert.deepEqual(triage.requiresPacks,
    ['ecommerce.progression.staff-access@1.0.0',
      'ecommerce.progression.support-intake@1.0.0']);
  assert.deepEqual(history.requiresPacks,
    ['ecommerce.feature.accounts@1.1.0', 'ecommerce.progression.support-intake@1.0.0']);

  const intakeText = `${fragmentText(intake.task.requirements[0])}\n${fragmentText(intake.task.contracts[0])}`;
  const triageText = `${fragmentText(triage.task.requirements[0])}\n${fragmentText(triage.task.contracts[0])}`;
  const historyText = `${fragmentText(history.task.requirements[0])}\n${fragmentText(history.task.contracts[0])}`;
  assert.doesNotMatch(intakeText, /assign|priority|status|reply|order|refund/i);
  assert.doesNotMatch(triageText, /reply|order|refund|customer history/i);
  assert.doesNotMatch(historyText, /assign|priority|reply|order|refund/i);
  assert.doesNotMatch(`${intakeText}\n${triageText}\n${historyText}`,
    /framework|ORM|database|websocket/i);

  assert.deepEqual(intake.checks.map(check => check.id), ['ticket-create']);
  assert.deepEqual(triage.checks.map(check => [check.id, check.criteria]), [
    ['assignment', ['611a']], ['priority', ['611b']], ['status', ['611c']],
  ]);
  assert.deepEqual(history.checks.map(check => [check.id, check.criteria]), [
    ['persistence', ['612a']], ['privacy', ['612b']],
  ]);
  for (const pack of [intake, triage, history]) {
    for (const check of pack.checks) {
      const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
        source: check.source,
        expectedLevel: 5,
      });
      assert.deepEqual(scenario.features.map(feature => feature.id), [check.feature]);
      const feature = scenario.features[0];
      for (const criterion of check.criteria ?? feature.criteria.map(item => item.id)) {
        assert(feature.criteria.some(item => item.id === criterion),
          `${pack.id} must own ${criterion}`);
      }
    }
  }
});
