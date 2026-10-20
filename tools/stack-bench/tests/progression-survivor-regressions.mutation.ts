import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition, type CompiledCriterion }
  from '../src/composition/definition-compiler.js';

const ROOT = STACK_BENCH_ROOT;
const readJson = (...parts: string[]): unknown =>
  JSON.parse(readFileSync(join(ROOT, ...parts), 'utf8'));

function criterion(source: unknown, id: string): CompiledCriterion {
  const scenario = compileScenarioDefinition(source);
  const selected = scenario.features.flatMap(feature => feature.criteria)
    .find(candidate => candidate.id === id);
  assert(selected, `scenario must contain criterion ${id}`);
  return selected;
}

test('scheduled-restock access targets the live restock through the named action', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios', '03-deferred-access.json');
  const compiled = compileScenarioDefinition(scenario);
  const feature = compiled.features[0];
  assert(feature, 'the deferred access scenario must contain a feature');
  assert.equal(feature.setup.some(step => step.do === 'click'
    && step.testid === 'pending-restock-cancel'), false,
  'setup must not leave an earlier DELETE for replayAs to select');

  const replay = criterion(scenario, '317a').steps.find(step => step.do === 'replayAs');
  assert(replay, 'criterion 317a must contain a replay action');
  assert(isRecord(replay.namedAction), 'the replay action must name its target action');
  assert.equal(replay.namedAction.id, 'cancelScheduledRestock');
  // The single scheduled job is selected without an undeclared item-name label.
  assert.deepEqual(replay.namedTarget, {
    testid: 'pending-restock-item',
    attribute: 'data-entity-id',
    valueType: 'string',
  });
});

test('stock-alert uniqueness waits for the second restock update before counting', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    'progression-stock-alerts.json');
  const steps = criterion(scenario, '631a').steps;
  const restockIndex = steps.findIndex(step => step.do === 'callAction'
    && step.action === 'restock');
  const secondRestock = steps[restockIndex];
  const finalCount = steps.at(-1);

  assert(secondRestock, 'criterion 631a must contain a second restock');
  assert.equal(steps[restockIndex + 1]?.do, 'expectActionOutcome');
  assert.equal(steps[restockIndex + 1]?.outcome, 'accepted');
  assert.equal(steps[restockIndex + 2]?.do, 'wait');
  assert.equal(steps[restockIndex + 2]?.ms, 10000);
  assert.equal(steps[restockIndex + 3]?.do, 'freshClient');
  assert(finalCount, 'criterion 631a must end with a count check');
  assert.equal(finalCount.do, 'expectElementCount');
  assert.equal(finalCount.equals, 1);
});

test('the duplicate-payment mutation remains visible through the payment view', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    'progression-core-business.json');
  const compiled = compileScenarioDefinition(scenario);
  const feature = compiled.features.find(candidate => candidate.id === 623);
  assert(feature, 'the core business scenario must contain feature 623');
  const callIndex = feature.setup.findIndex(step => step.do === 'callConcurrently');
  const concurrentCall = feature.setup[callIndex];
  const callOutcome = feature.setup[callIndex + 1];
  const ordersToggle = feature.setup.find(step => step.testid === 'orders-toggle');
  assert(concurrentCall && callOutcome && ordersToggle, 'feature 623 must contain its setup');
  assert.equal(concurrentCall.do, 'callConcurrently');
  assert.equal(callOutcome.do, 'expectCallOutcomes');
  assert.equal(ordersToggle.testid, 'orders-toggle');
  assert.equal(feature.setup[callIndex + 2]?.do, 'freshClient');
  assert.equal(ordersToggle.actor, 'owner-fresh');
  assert.equal(criterion(scenario, '623a').steps.some(step => step.do === 'callConcurrently'), false);
  assert.equal(criterion(scenario, '623b').steps.some(step => step.do === 'callConcurrently'), false);
});

test('cart expiration waits for the durable expiration state after restart', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    '03-deferred-durability.json');
  const steps = criterion(scenario, '316a').steps;
  const waitIndex = steps.findIndex(step => step.do === 'wait'
    && step.since === 'pending-316-accepted');
  const expiryWait = steps[waitIndex];
  assert(expiryWait && waitIndex > 0, 'expiration must be measured from the accepted reservation');
  assert.equal(expiryWait.ms, 310000);
  const beforeExpiry = steps.slice(0, waitIndex);
  assert(beforeExpiry.some(step => step.do === 'reload' && step.actor === 'customer'));
  assert(beforeExpiry.some(step => step.do === 'click' && step.testid === 'cart-toggle'
    && step.unlessVisible === 'cart-item'), 'opening an existing cart must not close it');
  const retainedIndex = beforeExpiry.findIndex(step => step.do === 'expectNumber'
    && step.testid === 'cart-count');
  assert(retainedIndex > 0, 'the restarted app must retain the reservation before expiry');
  assert.deepEqual(beforeExpiry[retainedIndex], {
    do: 'expectNumber', actor: 'customer', testid: 'cart-count', equals: 1, within: 1000,
  });
  for (const index of [retainedIndex - 1, retainedIndex + 1]) {
    assert.deepEqual(beforeExpiry[index], {
      do: 'expectElapsed', since: 'pending-316', atMost: 250000,
    }, 'the positive reservation check must finish before the expiration deadline');
  }
  assert.deepEqual(steps.slice(waitIndex + 1).map(step => step.do),
    ['reload', 'ensureSignedIn', 'click', 'expectNumber', 'expect', 'reload', 'expectNumber']);
  assert.equal(steps[waitIndex + 3]?.testid, 'cart-toggle');
  assert.deepEqual(steps[waitIndex + 4], {
    do: 'expectNumber', actor: 'customer', testid: 'cart-count', equals: 0, within: 10000,
  });
  assert.deepEqual(steps[waitIndex + 5], {
    do: 'expect', actor: 'customer', testid: 'cart-expired-notice', within: 10000,
  });
  assert.deepEqual(steps.at(-1), {
    do: 'expectNumber', actor: 'watcher', testid: 'item-stock',
    in: { testid: 'item-card', contains: 'Bluetooth Speaker' },
    relativeTo: 'before', plus: 0, within: 10000,
  });
});

test('staff role check reloads the saved value on each progression reference', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    'progression-staff-roles.json');
  const steps = criterion(scenario, '621a').steps;
  const reload = steps.find(step => step.do === 'reload');
  const savedRole = steps.at(-1);
  assert(reload && savedRole, 'criterion 621a must reload and check the saved role');
  assert.equal(reload.do, 'reload');
  // Rows are addressed by the account's own element id, not by row text.
  assert.deepEqual(savedRole, {
    do: 'expect', actor: 'admin-fresh', testid: 'staff-role-select',
    in: { testid: 'staff-role-account-staff' },
    value: 'inventory', within: 10000,
  });
});

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
