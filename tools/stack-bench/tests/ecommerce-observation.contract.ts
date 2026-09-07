import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const read = (name: string) => compileScenarioDefinition(JSON.parse(readFileSync(
  join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios', name), 'utf8')));

test('catalog variants and pagination enter the declared observation surface', () => {
  const variants = read('progression-catalog-management.json').features[0]!.criteria
    .find(criterion => criterion.id === '622b')!;
  assert.equal(variants.steps[0]!.do, 'openItem');
  assert.equal(variants.steps[0]!.item, 'Travel Mug');
  assert.equal(variants.steps[0]!.unlessVisible, 'item-variant');
  const setup = read('progression-faceted-pagination.json').features[0]!.setup;
  assert.deepEqual(setup.map(step => [step.do, step.testid, step.text, step.enter]), [
    ['fill', 'search-input', 'Air', undefined], ['fill', 'search-input', '', true],
  ]);
});

test('restock observations use one pending row and a ledger delta without item-name labels', () => {
  const source = read('03-scheduled-restocks.json');
  const due = source.features.find(feature => feature.id === 305)!.criteria[0]!.steps;
  const record = due.findIndex(step => step.do === 'recordNumber' && step.count === true);
  const submit = due.findIndex(step => step.testid === 'schedule-restock-submit');
  assert(record >= 0 && record < submit);
  assert(due.some(step => step.do === 'expectElementCount'
    && step.relativeTo === 'ledger-before' && step.plus === 1));
  for (const name of ['03-scheduled-restocks.json', '03-deferred-access.json',
    '03-deferred-durability.json', '03-deferred-integrity.json', '03-server-time.json']) {
    const scenario = JSON.stringify(read(name));
    assert(!/"testid":"pending-restock-item","contains"/.test(scenario));
  }
});

test('stock alerts use ready fresh account views after accepted restocks', () => {
  for (const name of ['progression-stock-alerts.json', 'progression-stock-alert-delivery.json']) {
    const feature = read(name).features[0]!;
    for (const criterion of feature.criteria) {
      for (const [index, step] of criterion.steps.entries()) {
        if (step.testid !== 'notifications-toggle') continue;
        assert.equal(step.unlessVisible, 'notifications-panel');
        assert.equal(criterion.steps[index - 2]!.do, 'freshClient');
        assert.equal(criterion.steps[index - 1]!.do, 'signIn');
        assert.equal(criterion.steps[index + 1]!.attribute, 'aria-busy');
        assert.equal(criterion.steps[index + 1]!.value, 'false');
      }
    }
  }
  const delivery = read('progression-stock-alert-delivery.json').features[0]!;
  const steps = delivery.criteria[0]!.steps;
  const empty = steps.findIndex(step => step.do === 'expectElementCount' && step.equals === 0);
  const restock = steps.findIndex(step => step.do === 'callAction' && step.action === 'restock');
  const delivered = steps.findIndex(step => step.do === 'expect' && step.testid === 'stock-alert-delivery');
  assert(empty >= 0 && empty < restock && restock < delivered);
  assert.equal(steps[restock + 1]!.do, 'expectActionOutcome');
  assert.equal(steps[restock + 1]!.outcome, 'accepted');
  assert.equal(steps[restock + 2]!.do, 'wait');
  assert.equal(steps[restock + 2]!.ms, 10000);
});

test('cart validation sends only a negative quantity and restores actor identity after reloads', () => {
  const feature = read('01-cart-boundary.json').features[0]!;
  const criterion = feature.criteria.find(candidate => candidate.id === '109b')!;
  const calls = criterion.steps.filter(step => step.do === 'callAction');
  // Zero is a convention (remove the line or refuse); only a negative quantity can credit money.
  assert.deepEqual(calls.map(step => (step.namedAction as { args: number[] }).args[1]), [-3]);
  for (const criterion of feature.criteria) {
    criterion.steps.forEach((step, index) => {
      if (step.do !== 'reload') return;
      assert.equal(criterion.steps[index + 1]!.do, 'ensureSignedIn');
      assert.equal(criterion.steps[index + 1]!.actor, step.actor);
    });
  }
});
