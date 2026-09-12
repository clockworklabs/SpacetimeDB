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

test('warehouse criteria enter the admin area once, including individually selected criteria', () => {
  for (const name of ['01-admin-write-staff.json', '01-warehouse-admin-staff.json']) {
    const selected = feature(name);
    assert.equal(selected.setup.filter(step => step.do === 'click'
      && step.actor === 'admin' && step.testid === 'admin-link').length, 1);
    for (const criterion of selected.criteria) {
      assert(!criterion.steps.some(step => step.do === 'click'
        && step.actor === 'admin' && step.testid === 'admin-link'));
      assert(criterion.steps.some(step => step.do.startsWith('expect')));
    }
  }
});

test('support reload checks restore the staff actor and still verify saved fields', () => {
  for (const [id, field, value] of [['611a', 'support-assignee', 'staff'],
    ['611b', 'support-priority', 'high']]) {
    const criterion = feature('progression-support-triage.json').criteria.find(item => item.id === id)!;
    const reload = criterion.steps.findIndex(step => step.do === 'reload');
    assert(reload >= 0);
    assert.deepEqual(criterion.steps[reload + 1],
      { do: 'ensureSignedIn', actor: 'staff', name: 'staff', password: 'stackbench-staff-2026',
        exact: true, readyTestid: 'current-user' });
    assert.deepEqual(criterion.steps[reload + 2],
      { do: 'click', actor: 'staff', testid: 'staff-link', ifAvailable: true, unlessVisible: 'support-assignee' });
    assert(criterion.steps.slice(reload + 3).some(step => step.do === 'expect'
      && step.testid === field && step.value === value));
  }
});

test('independent authorization probes restore only their required actor after reload', () => {
  const warehouse = feature('01-admin-write-staff.json').criteria.find(item => item.id === '103b')!;
  const replay = warehouse.steps.findIndex(step => step.do === 'callAction' && step.actor === 'staff');
  const reload = warehouse.steps.findIndex(step => step.do === 'reload');
  assert(reload >= 0 && replay > reload);
  assert.deepEqual(warehouse.steps[reload + 1],
    { do: 'ensureSignedIn', actor: 'staff', name: 'staff', password: 'stackbench-staff-2026',
      exact: true, readyTestid: 'current-user' });
  assert.equal(warehouse.steps[replay]!.from, 'admin');
  assert(warehouse.steps.slice(replay + 1).some(step => step.do === 'expectActionOutcome'
    && step.actor === 'staff' && step.outcome === 'refused'));
  assert(warehouse.steps.slice(replay + 1).some(step => step.do === 'expectNumber' && step.plus === 0));

  const privacy = feature('progression-managed-support-privacy.json').criteria[0]!.steps;
  const staffReload = privacy.findIndex(step => step.do === 'reload' && step.actor === 'staff');
  assert.deepEqual(privacy[staffReload + 1], warehouse.steps[reload + 1]);
  assert(privacy.slice(staffReload + 1).some(step => step.do === 'expectElementCount' && step.equals === 1));
  assert(privacy.some(step => step.do === 'expectReplayRejected' && step.actor === 'other'));

  const durability = feature('01-account-reload.json').criteria[0]!;
  assert(durability.steps.some(step => step.do === 'reload'));
  assert(!durability.steps.some(step => step.do === 'ensureSignedIn' || step.do === 'signIn'));
});

test('promotion and catalog management use the entries declared by their interfaces', () => {
  assert(feature('progression-promotion-rules.json').setup.some(step => step.do === 'click'
    && step.testid === 'staff-link'));
  assert.match(readFileSync(join(track, 'contracts/promotion-rules.md'), 'utf8'),
    /`staff-link`.*staff area/);
  assert(feature('progression-catalog-management.json').setup.some(step => step.do === 'click'
    && step.testid === 'admin-link'));
  assert.match(readFileSync(join(track, 'contracts/catalog-management.md'), 'utf8'),
    /`admin-link`.*administrator area containing the product controls/);
});

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
});

test('support history opens the saved history after submission confirmation', () => {
  const history = feature('progression-support-history.json');
  assert(history.setup.some(step => step.do === 'click' && step.testid === 'support-link'));
  const visible = history.criteria.find(criterion => criterion.id === '612c')!;
  assert.equal(visible.steps[0]!.do, 'reload');
  assert(visible.steps.some(step => step.do === 'click' && step.testid === 'support-link'
    && step.unlessVisible === 'support-ticket'));
  assert(visible.steps.some(step => step.do === 'expect' && step.testid === 'support-ticket'));
});

test('cart access assertions read refreshed state after direct writes', () => {
  const cart = feature('01-cart-boundary.json');
  for (const criterion of cart.criteria) {
    const result = criterion.steps.findIndex(step => step.do === 'expectActionOutcome');
    assert(result >= 0);
    assert.equal(criterion.steps[result + 1]!.do, 'reload');
    assert(criterion.steps.slice(result + 1).some(step => step.do === 'expectNumber'
      && step.testid === 'cart-quantity'));
  }
  const stranger = cart.criteria[0]!.steps;
  assert(stranger.some(step => step.do === 'reload' && step.actor === 'stranger'));
});
