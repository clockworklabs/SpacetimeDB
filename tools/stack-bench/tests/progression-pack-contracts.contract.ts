import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { loadValidatedProgressionSource } from './helpers/progression-source.js';

const root = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const readPack = (name: string) => compilePackDefinition(
  readJson(join(root, 'composition', 'packs', name)), { source: name });
const readScenario = (name: string, expectedLevel?: number) => compileScenarioDefinition(
  readJson(join(root, 'scenarios', name)), { source: name, expectedLevel });

const dedicatedPacks: readonly [name: string, modes: readonly string[]][] = [
  ['feature-reviews-1.2.1.json', ['fresh', 'upgrade']],
  ['feature-warehouse-admin-1.2.1.json', ['fresh', 'upgrade']],
  ['feature-purchasing-1.2.1.json', ['fresh', 'upgrade']],
  ['feature-cart-2.0.0.json', ['fresh', 'upgrade']],
  ['feature-checkout-2.0.0.json', ['fresh', 'upgrade']],
  ['progression-catalog-management-1.0.2.json', ['upgrade']],
  ['progression-payment-records-2.0.0.json', ['upgrade']],
  ['progression-staff-activity-1.0.2.json', ['upgrade']],
  ['progression-recommendation-feedback-2.0.0.json', ['upgrade']],
  ['l2-order-cancellation-features-1.0.1.json', ['upgrade']],
  ['l3-reservations-features-2.0.0.json', ['upgrade']],
  ['l3-scheduled-restocks-features-1.1.1.json', ['upgrade']],
  ['l3-order-delivery-features-1.1.1.json', ['upgrade']],
  ['l3-cart-expiration-features-2.0.0.json', ['upgrade']],
];

test('progression packs own one implementation-neutral product and test contract', () => {
  const requirementPaths = new Set<string>();
  const contractPaths = new Set<string>();
  for (const [name, modes] of dedicatedPacks) {
    const pack = readPack(name);
    assert.equal(pack.task.requirements.length, 1, name);
    assert.equal(pack.task.contracts.length, 1, name);
    const requirement = pack.task.requirements[0];
    const contract = pack.task.contracts[0];
    assert(requirement && contract);
    for (const fragment of [requirement, contract]) {
      assert.deepEqual(fragment.modes, modes, name);
      assert.equal(fragment.from, undefined, name);
      assert.equal(fragment.until, undefined, name);
    }
    assert.equal(requirementPaths.has(requirement.path), false, requirement.path);
    assert.equal(contractPaths.has(contract.path), false, contract.path);
    requirementPaths.add(requirement.path);
    contractPaths.add(contract.path);
    assert.doesNotMatch(readFileSync(join(root, requirement.path), 'utf8'),
      /POST \/|reducer|framework|ORM|database|websocket/i, name);
  }
});

test('shopping checks preserve their focused actors and criteria', () => {
  const cases: readonly [pack: string, scenario: string, actors: readonly string[],
    criteria: readonly string[]][] = [
    ['feature-purchasing-1.2.1.json', 'progression-purchasing-1.0.0.json', ['buyer'], ['3c']],
    ['feature-cart-2.0.0.json', 'progression-cart-checkout-1.0.0.json',
      ['quantity', 'checkout'], ['4a']],
    ['feature-checkout-2.0.0.json', 'progression-cart-checkout-1.0.0.json',
      ['quantity', 'checkout'], ['4d']],
  ];
  for (const [packName, scenarioName, actors, criteria] of cases) {
    const check = readPack(packName).checks[0];
    const feature = readScenario(scenarioName, 1).features[0];
    assert(check && feature);
    assert.equal(check.source, `scenarios/${scenarioName}`);
    assert.deepEqual(check.criteria, criteria);
    assert.deepEqual(feature.actors, actors);
    assert(criteria.every(id => feature.criteria.some(candidate => candidate.id === id)));
  }

  const [quantity, checkout] = readScenario('progression-cart-checkout-1.0.0.json').features[0]?.criteria ?? [];
  assert(quantity && checkout);
  const product = (criterion: typeof quantity): string => {
    const add = criterion.steps.find(step => step.do === 'click' && step.testid === 'add-to-cart');
    assert(add && typeof add.in?.contains === 'string');
    return add.in.contains;
  };
  assert.equal(product(quantity), 'Headphones');
  assert.equal(product(checkout), 'Desk Lamp');
});

test('activity and cancellation checks remain bound to their dedicated scenarios', () => {
  const activity = readPack('progression-staff-activity-1.0.2.json').checks[0];
  assert(activity);
  const activityFeature = readScenario('progression-staff-activity-1.0.0.json', 5).features[0];
  assert(activityFeature);
  const activitySteps = [...activityFeature.setup,
    ...activityFeature.criteria.flatMap(criterion => criterion.steps)];
  assert(activitySteps.some(step => step.testid === 'catalog-save'));
  assert(activitySteps.some(step => step.testid === 'activity-time'));
  assert(!activitySteps.some(step => step.testid === 'price-submit'));

  const cancellation = readPack('l2-order-cancellation-features-1.0.1.json');
  assert.deepEqual(cancellation.checks.map(check => check.source), [
    'scenarios/02-order-cancellation-core-1.0.0.json',
    'scenarios/02-order-cancellation-history-1.0.0.json',
  ]);
  for (const check of cancellation.checks) {
    const scenario = readScenario(check.source.replace(/^scenarios\//, ''), 2);
    assert(scenario.features.some(feature => feature.id === check.feature));
  }
});

test('fulfilment and cancellation checks have separate authorization owners', () => {
  const feature = readPack('progression-fulfilment-queue-1.0.1.json');
  const access = readPack('progression-operations-access-specifications-1.0.0.json');
  assert.equal(feature.moduleType, 'feature');
  assert.deepEqual(feature.checks.map(check => check.criteria), [['1a'], ['1b'], ['1c'], ['1d']]);
  assert(!feature.checks.some(check => check.criteria?.includes('1e') === true));
  assert.equal(access.moduleType, 'specification');
  assert.deepEqual(access.checks.map(check => check.requiresFeatures), [
    ['ecommerce.l2.stock-transfers-features'],
    ['ecommerce.l2.price-history-features'],
    ['ecommerce.progression.fulfilment-queue'],
    ['ecommerce.l2.order-cancellation-features'],
  ]);

  const { definition, gradingGroups } = loadValidatedProgressionSource(
    join(root, 'progression', 'ecommerce-2.0.1.json'), root);
  const fulfilment = definition.nodes.find(node => node.id === 'fulfilment-queue');
  const cancellation = definition.nodes.find(node => node.id === 'order-cancellation');
  assert(fulfilment && cancellation);
  assert.deepEqual(fulfilment.featureRefs, ['ecommerce.progression.fulfilment-queue@1.0.1']);
  assert(gradingGroups(fulfilment.id).some(group => group.endsWith('#operator-authorization-direct')));
  assert(!gradingGroups(fulfilment.id).some(group => group.endsWith('#order-owner-direct')));
  assert(gradingGroups(cancellation.id).some(group => group.endsWith('#order-owner-direct')));
});
