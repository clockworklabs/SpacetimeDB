import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

function scenario(name: string) {
  const source = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce', 'scenarios', name);
  return compileScenarioDefinition(JSON.parse(readFileSync(source, 'utf8')), { source });
}

test('session-survival criteria observe the signed-in user without restoring the session', () => {
  // These IDs explicitly claim session survival. Do not infer that claim from wording
  // or apply this rule to independent data-retention checks that permit re-authentication.
  for (const [name, id] of [
    ['01-account-reload.json', '1e'], ['01-features.json', '1e'],
    ['01-invariants.json', '105a'], ['progression-account-state-reload.json', '105a'],
  ] as const) {
    const criterion = scenario(name).features.flatMap(feature => feature.criteria)
      .find(criterion => criterion.id === id);
    assert(criterion, `${name}: ${id} must exist`);
    const reload = criterion.steps.findIndex(step => step.do === 'reload');
    assert(reload >= 0, `${name}: ${id} must reload`);
    const after = criterion.steps.slice(reload + 1);
    const observation = after.findIndex(step => step.do === 'expect' && step.testid === 'current-user');
    assert(observation >= 0, `${name}: ${id} must observe the session`);
    assert.equal(after.slice(0, observation).some(step =>
      ['signIn', 'signUp', 'ensureSignedIn'].includes(step.do)), false, `${name}: ${id} must not restore it`);
  }
});

test('restock checks reopen and verify the admin destination after every admin reload', () => {
  let reloads = 0;
  for (const name of ['03-server-time.json', '03-deferred-durability.json']) {
    for (const feature of scenario(name).features) {
      const steps = [...feature.setup, ...feature.criteria.flatMap(criterion => criterion.steps)];
      for (const [index, step] of steps.entries()) {
        if (step.do !== 'reload' || step.actor !== 'admin') continue;
        reloads++;
        // A fresh page may first restore the admin session; the destination is then reopened
        // and verified before any observation.
        const next = steps[index + 1]!.do === 'ensureSignedIn' ? index + 2 : index + 1;
        assert.deepEqual(steps[next],
          { do: 'click', actor: 'admin', testid: 'admin-link', ifAvailable: true });
        // An optional in-area navigation hook may sit between the area and its controls.
        const after = steps[next + 1]!.testid === 'restocks-link' ? next + 2 : next + 1;
        assert.deepEqual(steps[after],
          { do: 'expect', actor: 'admin', testid: 'schedule-restock-submit' });
      }
    }
  }
  assert.equal(reloads, 3, 'cover both server-time reloads and the durability cleanup observation');
  const delivery = scenario('03-deferred-integrity.json').features.find(feature => feature.id === 312)!;
  assert.deepEqual(delivery.setup.at(-1),
    { do: 'click', actor: 'staff', testid: 'staff-link', ifAvailable: true });
  const firstObservation = delivery.criteria[0]!.steps.find(step => step.do === 'expect')!;
  assert.equal(firstObservation.testid, 'completed-order-item');
  assert.equal(firstObservation.absent, undefined);
});

test('support privacy confirms persisted owner writes without requiring live refresh', () => {
  const privacy = scenario('progression-managed-support-privacy.json').features[0]!.criteria[0]!;
  const tail = privacy.steps.slice(privacy.steps.findIndex(step => step.do === 'expectReplayRejected'));
  assert.equal(tail[0]!.do, 'expectReplayRejected');
  assert.deepEqual(tail[1], { do: 'reload', actor: 'staff', settleMs: 2000 });
  assert.deepEqual(tail[2],
    { do: 'ensureSignedIn', actor: 'staff', name: 'staff', password: 'stackbench-staff-2026',
      exact: true, readyTestid: 'current-user' });
  assert.deepEqual(tail[3],
    { do: 'click', actor: 'staff', testid: 'staff-link', ifAvailable: true });
  assert.deepEqual(tail[4],
    { do: 'expect', actor: 'staff', testid: 'support-ticket', contains: 'Private managed case {user:casemarker}' });
  assert.equal(tail[5]!.do, 'expectElementCount');
  assert.equal(tail[5]!.equals, 1, 'an unauthorized replay must not add a second reply');
  const live = scenario('progression-managed-support-shared.json').features[0]!.criteria
    .find(criterion => criterion.id === '613a')!;
  assert.equal(live.steps.some(step => step.do === 'reload'), false);
  assert.equal(live.steps.filter(step => step.do === 'expect'
    && step.testid === 'support-reply-item').length, 2, 'both open clients must still receive live replies');
});

test('low-stock observations follow the optional in-area link the contract allows', () => {
  for (const [name, featureId] of [['02-low-stock.json', 5], ['02-features.json', 5]] as const) {
    const feature = scenario(name).features.find(feature => feature.id === featureId)!;
    const area = feature.setup.findIndex(step => step.do === 'click' && step.testid === 'admin-link');
    assert(area >= 0, `${name}: the admin area is opened in setup`);
    assert.deepEqual(feature.setup[area + 1],
      { do: 'click', actor: 'admin', testid: 'low-stock-link', ifAvailable: true });
  }
});

test('scheduled-restock setups follow the optional in-area link the contract allows', () => {
  for (const [name, featureIds] of [['03-scheduled-restocks.json', [302, 305, 306]], ['03-features.json', [302]]] as const) {
    for (const featureId of featureIds) {
      const feature = scenario(name).features.find(feature => feature.id === featureId)!;
      const area = feature.setup.findIndex(step => step.do === 'click' && step.testid === 'admin-link');
      assert(area >= 0, `${name}/${featureId}: the admin area is opened in setup`);
      assert.deepEqual(feature.setup[area + 1],
        { do: 'click', actor: 'admin', testid: 'restocks-link', ifAvailable: true });
    }
  }
});

test('the queue warehouse label is observed on a fresh staff page', () => {
  for (const name of ['02-queue-warehouse.json', '02-self-contained.json']) {
    const check = scenario(name).features.flatMap(feature => feature.criteria)
      .find(criterion => criterion.id === '1b')!;
    const observation = check.steps.findIndex(step => step.do === 'expect' && step.testid === 'queue-item');
    const before = check.steps.slice(0, observation);
    const reload = before.findLastIndex(step => step.do === 'reload' && step.actor === 'staff');
    assert(reload >= 0, `${name}: staff reloads before reading the queue`);
    assert.equal(before[reload + 1]!.do, 'ensureSignedIn');
    assert.deepEqual(before[reload + 2], { do: 'click', actor: 'staff', testid: 'staff-link', ifAvailable: true });
  }
});

test('the direct conservation race is observed on fresh pages', () => {
  const race = scenario('02-server-actions.json').features.flatMap(feature => feature.criteria)
    .find(criterion => criterion.id === '202d')!;
  for (const actor of ['admin', 'customer']) {
    const observation = race.steps.findIndex(step => step.do === 'expectNumber' && step.actor === actor);
    const before = race.steps.slice(0, observation);
    const reload = before.findLastIndex(step => step.do === 'reload' && step.actor === actor);
    assert(reload >= 0, `${actor} reloads before reading the conserved total`);
    assert.equal(before[reload + 1]!.do, 'ensureSignedIn');
    assert(before.slice(reload).every(step => step.actor === actor),
      `${actor}'s fresh page is not interleaved with the other actor`);
  }
  assert(race.steps.some(step => step.do === 'dbExpectStock' && step.warehouse === 'East'
    && step.atLeast === 74 && step.atMost === 75));
  assert(race.steps.some(step => step.do === 'dbExpectStock' && step.warehouse === 'West'
    && step.atLeast === 124 && step.atMost === 125));
  assert(race.steps.some(step => step.do === 'dbExpectStock' && step.equals === 199));
  assert(race.steps.some(step => step.do === 'expect' && step.testid === 'order-item' && step.count === 1));
});

test('L2 direct authorization refusals follow accepted routes and fresh observations', () => {
  for (const [file, ids] of [['02-server-actions.json', ['201c']], ['02-self-contained.json', ['1e']],
    ['02-strengthened.json', ['201a', '201b']]] as const) {
    for (const id of ids) {
      const check = scenario(file).features.flatMap(feature => feature.criteria).find(criterion => criterion.id === id)!;
      const refusal = check.steps.findIndex(step => step.do === 'expectActionOutcome' && step.outcome === 'refused');
      assert(refusal > 0);
      const proof = check.steps[refusal]!.routeProvenBy;
      assert(check.steps.slice(0, refusal).some(step => step.do === 'expectActionOutcome'
        && step.actor === proof && step.outcome === 'accepted'));
      assert(check.steps.slice(refusal + 1).some(step => step.do === 'reload'));
    }
  }
});

test('cancellation conservation proves the sale before its reversal', () => {
  for (const [file, ids] of [['02-features.json', ['3a']], ['02-self-contained.json', ['202b', '202c']]] as const) {
    for (const id of ids) {
      const check = scenario(file).features.flatMap(feature => feature.criteria).find(criterion => criterion.id === id)!;
      const cancel = check.steps.findIndex(step => step.do === 'click' && step.testid === 'cancel-order');
      assert(check.steps.slice(0, cancel).some(step => step.do === 'dbExpectStock' && step.plus === -1));
      assert(check.steps.slice(0, cancel).some(step => step.do === 'expect' && step.testid === 'order-item' && step.count === 1));
      assert(check.steps.slice(cancel + 1).some(step => step.do === 'dbExpectStock' && step.plus === 0));
    }
  }
});
