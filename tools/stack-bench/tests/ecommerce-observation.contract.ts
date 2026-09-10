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
  const source = read('03-scheduled-restock-apply.json');
  const due = source.features.find(feature => feature.id === 305)!.criteria[0]!.steps;
  const record = due.findIndex(step => step.do === 'recordNumber' && step.count === true);
  const submit = due.findIndex(step => step.testid === 'schedule-restock-submit');
  assert(record >= 0 && record < submit);
  assert(due.some(step => step.do === 'expectElementCount'
    && step.relativeTo === 'ledger-before' && step.plus === 1));
  const pack = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/composition/packs/l3-scheduled-restocks-features.json'), 'utf8'));
  const sources = pack.checks.map((check: { source: string }) => check.source);
  assert.equal(new Set(sources).size, 3, 'each restock check needs its own database reset boundary');
  for (const [index, name] of ['03-scheduled-restocks.json', '03-scheduled-restock-apply.json',
    '03-scheduled-restock-cancel.json'].entries()) {
    assert.equal(sources[index], `scenarios/${name}`);
    assert.deepEqual(read(name).features.flatMap(feature => feature.criteria).map(check => check.id),
      [['302a'], ['305a'], ['306a']][index]);
  }
  for (const name of ['03-scheduled-restocks.json', '03-scheduled-restock-apply.json',
    '03-scheduled-restock-cancel.json', '03-deferred-access.json',
    '03-deferred-durability.json', '03-deferred-integrity.json', '03-server-time.json']) {
    const scenario = JSON.stringify(read(name));
    assert(!/"testid":"pending-restock-item","contains"/.test(scenario));
  }
});

test('overdraw failure cannot change the next transfer or authorization probe state', () => {
  const isolated = read('02-transfer-overdraw.json');
  const shared = read('02-strengthened.json');
  assert.deepEqual(isolated.features.flatMap(feature => feature.criteria).map(check => check.id), ['2c']);
  assert(!shared.features.flatMap(feature => feature.criteria).some(check => check.id === '2c'));
  assert.deepEqual(isolated.features[0]!.setup, shared.features.find(feature => feature.id === 2)!.setup);
  const pack = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/composition/packs/spec-transactional-integrity.json'), 'utf8'));
  assert.equal(pack.checks.find((check: { id: string }) => check.id === 'stock-transfer-overdraw').source,
    'scenarios/02-transfer-overdraw.json');
});

test('support privacy creates its own persisted ticket after the durability probe', () => {
  const history = read('progression-support-history.json').features[0]!;
  const privacy = history.criteria.find(check => check.id === '612b')!.steps;
  const subject = privacy.find(step => step.testid === 'support-subject')!.text;
  assert(subject);
  assert.notEqual(subject, history.setup.find(step => step.testid === 'support-subject')!.text);
  const submit = privacy.findIndex(step => step.testid === 'support-submit');
  const reloaded = privacy.findIndex((step, index) => index > submit && step.do === 'reload');
  const positive = privacy.findIndex(step => step.do === 'expect' && step.testid === 'support-ticket'
    && step.contains === subject && step.absent !== true);
  assert(submit >= 0 && reloaded > submit && positive > reloaded);
  assert(privacy.slice(positive + 1).some(step => step.do === 'expectReceived' && step.contains === subject));
  assert(privacy.some(step => step.actor === 'other' && step.do === 'expect'
    && step.contains === subject && step.absent === true));
  assert(privacy.some(step => step.actor === 'other' && step.do === 'expectNotReceived'
    && step.contains === subject));
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

test('later-depth checks retain their own effects and independent controls', () => {
  const reorder = read('progression-automatic-reorder.json').features[0]!;
  assert.equal(reorder.setup.filter(step => step.testid === 'buy-now').length, 2);
  assert.equal(reorder.criteria.find(c => c.id === '502a')!.steps.some(step => step.testid === 'buy-now'), false);
  const duplicate = reorder.criteria.find(c => c.id === '502b')!.steps;
  assert.equal(duplicate[0]!.do, 'expectElementCount');
  assert.equal(duplicate[1]!.testid, 'buy-now');
  const payment = read('progression-core-business.json').features.find(f => f.id === 623)!;
  assert.equal(payment.setup.find(step => step.do === 'expectCallOutcomes')!.accepted, undefined);
  assert(payment.setup.some(step => step.do === 'freshClient'));
  assert(payment.setup.some(step => step.do === 'expectElementCount' && step.testid === 'order-item' && step.equals === 1));
  const checkout = read('03-deferred-integrity.json').features.find(f => f.id === 314)!.criteria[0]!.steps;
  assert(checkout.some(step => step.do === 'reload'));
  assert(checkout.some(step => step.testid === 'order-item' && step.equals === 1));
  const filters = read('progression-faceted-filters.json').features[0]!;
  assert.equal(filters.setup.filter(step => step.do === 'dbSetStock' && step.item === 'Coffee Grinder' && step.quantity === 0).length, 2);
  const steps = filters.criteria[0]!.steps;
  assert.equal(steps[0]!.contains, 'Coffee Grinder');
  assert.equal(steps[0]!.equals, 1);
  assert.equal(steps[1]!.testid, 'in-stock-filter');
  assert(steps.some(step => step.contains === 'Coffee Grinder' && step.absent === true));
  const boundary = read('progression-order-support-boundary.json').features[0]!.criteria[0]!.steps;
  const attack = boundary.find(step => step.do === 'callAction' && step.actor === 'other')!;
  assert.deepEqual((attack.input as { overrides: unknown }).overrides, {
    caseId: { actor: 'other', testid: 'support-ticket', contains: 'Other order case', attribute: 'data-entity-id' },
  });
  assert(boundary.some(step => step.do === 'expectActionOutcome' && step.actor === 'owner' && step.outcome === 'accepted'));
});

test('progression review eligibility proves the route and reads back persisted refusal', () => {
  const steps = read('progression-review-access.json').features[0]!.criteria[0]!.steps;
  assert.equal(steps[0]!.do, 'callAction');
  assert.equal(steps[0]!.actor, 'owner');
  assert.equal(steps[1]!.outcome, 'accepted');
  const refused = steps.find(step => step.do === 'expectActionOutcome' && step.actor === 'stranger')!;
  assert.equal(refused.outcome, 'application-refused');
  assert.equal(refused.routeProvenBy, 'owner');
  assert(steps.some(step => step.do === 'freshClient' && step.actor === 'stranger'));
  assert(steps.some(step => step.actor === 'stranger-fresh' && step.contains === 'never bought this' && step.absent === true));
});


test('countdown displays decrease from observed baselines without setup-time assumptions', () => {
  for (const [source, id] of [['03-reservations.json', '305a'], ['03-scheduled-restocks.json', '302a']]) {
    const criterion = read(source!).features.flatMap(f => f.criteria).find(c => c.id === id)!;
    const recordIndex = criterion.steps.findIndex(step => step.do === 'recordNumber' && step.as === 'initial-countdown');
    assert(recordIndex > 0);
    assert.equal(criterion.steps[recordIndex - 1]!.atLeast, 1);
    assert.equal(criterion.steps[recordIndex - 1]!.atMost, 90);
    assert.equal(criterion.steps[recordIndex + 1]!.ms, 1000);
    const bound = criterion.steps[recordIndex + 2]!;
    assert.equal(bound.relativeTo, 'initial-countdown');
    assert.equal(bound.plus, -1);
    assert.equal(bound.comparison, 'atMost');
    assert.equal(bound.within, 10000);
  }
});
