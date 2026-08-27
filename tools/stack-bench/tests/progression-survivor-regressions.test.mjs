import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';

const ROOT = join(import.meta.dirname, '..');
const readJson = (...parts) => JSON.parse(readFileSync(join(ROOT, ...parts), 'utf8'));

function criterion(source, id) {
  const scenario = compileScenarioDefinition(source);
  return scenario.features.flatMap(feature => feature.criteria)
    .find(candidate => candidate.id === id);
}

test('scheduled-restock access targets the live restock through the named action', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios', '03-deferred-access-1.0.0.json');
  assert.equal(scenario.features[0].setup.some(step => step.do === 'click'
    && step.testid === 'pending-restock-cancel'), false,
  'setup must not leave an earlier DELETE for replayAs to select');

  const replay = criterion(scenario, '317a').steps.find(step => step.do === 'replayAs');
  assert.equal(replay.namedAction.id, 'cancelScheduledRestock');
  assert.deepEqual(replay.namedTarget, {
    testid: 'pending-restock-item',
    contains: 'Webcam',
    attribute: 'data-entity-id',
    valueType: 'string',
  });
});

test('stock-alert uniqueness waits for the second restock update before counting', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    'progression-stock-alerts-1.0.0.json');
  const steps = criterion(scenario, '631a').steps;
  const secondRestock = steps.filter(step => step.do === 'click'
    && step.testid === 'restock-submit')[0];
  const finalCount = steps.at(-1);

  assert(secondRestock.settleMs >= 1000);
  assert.equal(finalCount.do, 'expectElementCount');
  assert.equal(finalCount.equals, 1);
});

test('the duplicate-payment mutation remains visible through the payment view', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    'progression-core-business-1.0.0.json');
  const steps = criterion(scenario, '623b').steps;
  assert.equal(steps[0].do, 'callConcurrently');
  assert.equal(steps[1].do, 'expectCallOutcomes');
  assert.equal(steps[2].testid, 'orders-toggle');

  const manifest = readJson('grader', 'mutations', 'spacetime-ecom-progression-1.0.0.json');
  const mutation = manifest.mutations.find(candidate =>
    candidate.id === 'checkout-records-duplicate-payments');
  assert(mutation);
  assert.match(mutation.edits[0].replace,
    /amount: order\.total \+ 0\.01, status: 'paid'/);
});

test('cart expiration waits for the durable expiration state after restart', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    '03-deferred-durability-1.0.0.json');
  const steps = criterion(scenario, '316a').steps;
  assert.equal(steps[0].do, 'reload');
  assert.equal(steps[1].testid, 'cart-expired-notice');
  assert.equal(steps[1].within, 220000);
  assert.equal(steps[2].testid, 'cart-count');
  assert.equal(steps[3].testid, 'item-stock');
});
