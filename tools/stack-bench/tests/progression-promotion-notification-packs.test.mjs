import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const readPack = name => compilePackDefinition(readJson(join(packRoot, name)), { source: name });
const packNames = [
  'progression-notification-preferences-1.0.0.json',
  'progression-promotion-rules-1.0.0.json',
  'progression-promotion-checkout-1.0.0.json',
  'progression-stock-alerts-1.0.0.json',
  'progression-delivery-notifications-1.0.0.json',
  'progression-promotion-reporting-1.0.0.json',
];

function fragmentText(fragment) {
  return readFileSync(join(trackRoot, fragment.path), 'utf8');
}

test('promotion and notification packs have isolated upgrade contracts', () => {
  const packs = packNames.map(readPack);
  for (const pack of packs) {
    assert.equal(pack.state, 'draft');
    assert.equal(pack.moduleType, 'feature');
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.contracts.length, 1);
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      assert.deepEqual(fragment.modes, ['upgrade']);
      assert.equal(fragment.from, undefined);
      assert.equal(fragment.until, undefined);
    }
    const text = `${fragmentText(pack.task.requirements[0])}\n${fragmentText(pack.task.contracts[0])}`;
    assert.doesNotMatch(text, /framework|ORM|database|websocket/i);
  }
  assert.equal(new Set(packs.map(pack => pack.task.requirements[0].path)).size, packs.length);
  assert.equal(new Set(packs.map(pack => pack.task.contracts[0].path)).size, packs.length);
  assert.equal(new Set(packs.flatMap(pack => pack.checks.map(check => check.source))).size,
    packs.length);
});

test('promotion and notification packs declare exact dependencies and check ownership', () => {
  const packs = Object.fromEntries(packNames.map(name => {
    const pack = readPack(name);
    return [pack.id, pack];
  }));
  assert.deepEqual(packs['ecommerce.progression.notification-preferences'].requiresPacks,
    ['ecommerce.feature.accounts@1.2.0']);
  assert.deepEqual(packs['ecommerce.progression.promotion-rules'].requiresPacks,
    ['ecommerce.progression.staff-access@1.0.0', 'ecommerce.feature.catalog@1.2.0']);
  assert.deepEqual(packs['ecommerce.progression.promotion-checkout'].requiresPacks,
    ['ecommerce.progression.promotion-rules@1.0.0', 'ecommerce.feature.cart-checkout@1.3.0']);
  assert.deepEqual(packs['ecommerce.progression.stock-alerts'].requiresPacks,
    ['ecommerce.progression.notification-preferences@1.0.0',
      'ecommerce.feature.warehouse-admin@1.2.0']);
  assert.deepEqual(packs['ecommerce.progression.delivery-notifications'].requiresPacks,
    ['ecommerce.l3.order-delivery-features@1.1.0',
      'ecommerce.progression.notification-preferences@1.0.0']);
  assert.deepEqual(packs['ecommerce.progression.promotion-reporting'].requiresPacks,
    ['ecommerce.progression.promotion-checkout@1.0.0',
      'ecommerce.l2.operational-views-features@1.0.0']);

  assert.deepEqual(packs['ecommerce.progression.notification-preferences'].checks
    .map(check => [check.id, check.criteria]),
  [['persistence', ['630a']], ['privacy', ['630b']]]);
  assert.deepEqual(packs['ecommerce.progression.promotion-rules'].checks
    .map(check => [check.id, check.criteria]),
  [['values', ['620a']], ['access', ['620b']]]);
  assert.deepEqual(packs['ecommerce.progression.promotion-checkout'].checks
    .map(check => [check.id, check.criteria]),
  [['active', ['621a']], ['expired', ['621b']], ['exhausted', ['621c']]]);
  assert.deepEqual(packs['ecommerce.progression.stock-alerts'].checks
    .map(check => [check.id, check.criteria]),
  [['delivery', ['631a']], ['privacy', ['631b']]]);
  assert.deepEqual(packs['ecommerce.progression.delivery-notifications'].checks
    .map(check => [check.id, check.criteria]),
  [['delivery', ['501a']], ['privacy', ['501b']]]);
  assert.deepEqual(packs['ecommerce.progression.promotion-reporting'].checks
    .map(check => [check.id, check.criteria]),
  [['redemptions', ['622a']], ['revenue', ['622b']]]);

  for (const pack of Object.values(packs)) {
    const sources = new Set(pack.checks.map(check => check.source));
    assert.equal(sources.size, 1);
    const [source] = sources;
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, source)), {
      source,
      expectedLevel: 5,
    });
    assert.deepEqual(scenario.features.map(feature => feature.id), [pack.checks[0].feature]);
    const criteria = new Set(scenario.features[0].criteria.map(criterion => criterion.id));
    for (const check of pack.checks) {
      for (const criterion of check.criteria) assert(criteria.has(criterion));
    }
  }
});

test('each prompt states only its feature behavior', () => {
  const byId = Object.fromEntries(packNames.map(name => {
    const pack = readPack(name);
    return [pack.id, fragmentText(pack.task.requirements[0])];
  }));
  assert.doesNotMatch(byId['ecommerce.progression.notification-preferences'],
    /promotion|delivery|restock/i);
  assert.doesNotMatch(byId['ecommerce.progression.promotion-rules'],
    /cart|checkout|report|revenue/i);
  assert.doesNotMatch(byId['ecommerce.progression.promotion-checkout'],
    /staff|report|revenue|notification/i);
  assert.doesNotMatch(byId['ecommerce.progression.stock-alerts'],
    /promotion|delivery|order notification/i);
  assert.doesNotMatch(byId['ecommerce.progression.delivery-notifications'],
    /stock|promotion|report/i);
  assert.doesNotMatch(byId['ecommerce.progression.promotion-reporting'],
    /notification|stock|create promotion/i);
});

test('promotion rules use values accepted by datetime-local inputs', () => {
  const pack = readPack('progression-promotion-rules-1.0.0.json');
  const source = pack.checks[0].source;
  const scenario = compileScenarioDefinition(readJson(join(trackRoot, source)), { source });
  const values = scenario.features[0].criteria[0].steps
    .filter(step => step.do === 'fill'
      && ['promotion-start', 'promotion-end'].includes(step.testid))
    .map(step => step.text);
  assert.deepEqual(values, ['2099-01-01T00:00', '2099-12-31T23:59']);
});
