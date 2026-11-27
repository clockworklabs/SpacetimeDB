import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const track = join(STACK_BENCH_ROOT, 'tracks/ecommerce');
function feature(name: string) {
  const source = join(track, 'scenarios', name);
  return compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source }).features[0]!;
}

test('access checks observe protected content and retain direct restock authorization', () => {
  const staff = feature('progression-staff-access.json');
  for (const actor of ['staff', 'admin']) {
    assert(staff.criteria[0]!.steps.some(step => step.do === 'click' && step.actor === actor));
    assert(staff.criteria[0]!.steps.some(step => step.do === 'expect'
      && step.actor === actor && step.testid === 'staff-area'));
  }
  const customerBoundary = staff.criteria.find(criterion => criterion.id === '601b')!;
  assert(customerBoundary.steps.some(step => step.do === 'expect'
    && step.actor === 'authorized' && step.testid === 'staff-area' && step.absent !== true));
  for (const name of ['progression-staff-access.json', '02-fulfilment-access.json',
    '01-warehouse-admin-staff.json', '03-deferred-access.json']) {
    const steps = feature(name).criteria.flatMap(criterion => criterion.steps);
    assert(!steps.some(step => step.do === 'expect' && step.absent === true
      && ['staff-link', 'admin-link', 'admin-panel'].includes(String(step.testid))));
  }
  const restock = feature('03-deferred-access.json').criteria[0]!.steps;
  assert(restock.some(step => step.do === 'callAction'));
  assert(restock.some(step => step.do === 'expectActionOutcome' && step.outcome === 'refused'));
  assert(restock.some(step => step.do === 'replayAs'));
  assert(restock.some(step => step.do === 'expectReplayRejected'));
  for (const name of ['01-features.json', '01-warehouse-admin-staff.json']) {
    const source = join(track, 'scenarios', name);
    const warehouse = compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source })
      .features.find(item => item.id === 7)!;
    const boundary = warehouse.criteria.find(item => item.id === '7a')!;
    const authorized = name === '01-features.json' ? 'authorized' : 'admin';
    assert(boundary.steps.some(step => step.do === 'expect' && step.actor === authorized
      && step.testid === 'admin-item-row' && step.absent !== true));
    assert.equal([...warehouse.setup, ...boundary.steps].filter(step => step.do === 'click'
      && step.actor === authorized && step.testid === 'admin-link').length, 1);
    if (authorized !== 'admin') {
      assert(!boundary.steps.some(step => step.actor === 'admin'),
        'the positive control must not open the next criterion actor\'s panel');
      assert(warehouse.criteria.find(item => item.id === '7b')!.steps.some(step =>
        step.do === 'click' && step.actor === 'admin' && step.testid === 'admin-link'));
    }
  }
  // Deferred delivery is first observed through its positive completed order.
  const integrity = join(track, 'scenarios', '03-deferred-integrity.json');
  const delivery = compileScenarioDefinition(JSON.parse(readFileSync(integrity, 'utf8')), { source: integrity })
    .features.find(item => item.id === 312)!;
  const firstObservation = delivery.criteria[0]!.steps.find(step => step.do === 'expect')!;
  assert.equal(firstObservation.testid, 'completed-order-item');
  assert.equal(firstObservation.absent, undefined);
});
