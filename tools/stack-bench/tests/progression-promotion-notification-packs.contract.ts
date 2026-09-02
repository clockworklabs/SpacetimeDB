import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition, resolveTaskFragment, type CompiledPackDefinition }
  from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const readPack = (name: string) => compilePackDefinition(readJson(join(packRoot, name)), { source: name });
const packNames = [
  'progression-notification-preferences-1.0.0.json',
  'progression-promotion-rules-1.0.1.json',
  'progression-promotion-checkout-2.0.0.json',
  'progression-stock-alerts-1.0.1.json',
  'progression-delivery-notifications-1.0.1.json',
  'progression-promotion-reporting-2.0.0.json',
];

function fragmentText(fragment: CompiledPackDefinition['task']['requirements'][number]): string {
  return resolveTaskFragment(fragment, { trackRoot, source: fragment.id }).text;
}

test('promotion and notification packs have isolated upgrade contracts', () => {
  const packs = packNames.map(readPack);
  for (const pack of packs) {
    assert.equal(pack.moduleType, 'feature');
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.contracts.length, 1);
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      assert.deepEqual(fragment.modes, ['upgrade']);
      assert.equal(fragment.from, undefined);
      assert.equal(fragment.until, undefined);
    }
    const text = `${fragmentText(requiredFragment(pack.task.requirements[0], 'the task requirement'))}\n${fragmentText(requiredFragment(pack.task.contracts[0], 'the task contract'))}`;
    assert.doesNotMatch(text, /framework|ORM|database|websocket/i);
  }
  assert.equal(new Set(packs.map(pack => requiredFragment(pack.task.requirements[0], 'the task requirement').path)).size, packs.length);
  assert.equal(new Set(packs.map(pack => requiredFragment(pack.task.contracts[0], 'the task contract').path)).size, packs.length);
  assert.equal(new Set(packs.flatMap(pack => pack.checks.map(check => check.source))).size,
    packs.length);
});

test('promotion and notification packs declare exact dependencies and check ownership', () => {
  const packs = Object.fromEntries(packNames.map(name => {
    const pack = readPack(name);
    return [pack.id, pack];
  }));
  assert.deepEqual(requiredPack(packs, 'ecommerce.progression.notification-preferences').requiresPacks,
    ['ecommerce.feature.accounts@1.2.0']);
  assert.deepEqual(requiredPack(packs, 'ecommerce.progression.promotion-rules').requiresPacks,
    ['ecommerce.progression.staff-access@1.0.0', 'ecommerce.feature.catalog-items@1.0.0']);
  assert.deepEqual(requiredPack(packs, 'ecommerce.progression.promotion-checkout').requiresPacks,
    ['ecommerce.progression.promotion-rules@1.0.1', 'ecommerce.feature.checkout@2.0.0']);
  assert.deepEqual(requiredPack(packs, 'ecommerce.progression.stock-alerts').requiresPacks,
    ['ecommerce.progression.notification-preferences@1.0.0',
      'ecommerce.feature.warehouse-admin@1.2.1']);
  assert.deepEqual(requiredPack(packs, 'ecommerce.progression.delivery-notifications').requiresPacks,
    ['ecommerce.l3.order-delivery-features@1.1.1',
      'ecommerce.progression.notification-preferences@1.0.0']);
  assert.deepEqual(requiredPack(packs, 'ecommerce.progression.promotion-reporting').requiresPacks,
    ['ecommerce.progression.promotion-checkout@2.0.0']);

  assert.deepEqual(requiredPack(packs, 'ecommerce.progression.notification-preferences').checks
    .map(check => [check.id, check.criteria]),
  [['persistence', ['630a']], ['privacy', ['630b']]]);
  assert.deepEqual(requiredPack(packs, 'ecommerce.progression.promotion-rules').checks
    .map(check => [check.id, check.criteria]),
  [['values', ['620a']], ['access', ['620b']]]);
  assert.deepEqual(requiredPack(packs, 'ecommerce.progression.promotion-checkout').checks
    .map(check => [check.id, check.criteria]),
  [['active', ['621a']], ['expired', ['621b']], ['exhausted', ['621c']]]);
  assert.deepEqual(requiredPack(packs, 'ecommerce.progression.stock-alerts').checks
    .map(check => [check.id, check.criteria]),
  [['delivery', ['631a']], ['privacy', ['631b']]]);
  assert.deepEqual(requiredPack(packs, 'ecommerce.progression.delivery-notifications').checks
    .map(check => [check.id, check.criteria]),
  [['delivery', ['501a']], ['privacy', ['501b']]]);
  assert.deepEqual(requiredPack(packs, 'ecommerce.progression.promotion-reporting').checks
    .map(check => [check.id, check.criteria]),
  [['redemptions', ['622a']], ['revenue', ['622b']]]);

  for (const pack of Object.values(packs)) {
    const sources = new Set(pack.checks.map(check => check.source));
    assert.equal(sources.size, 1);
    const [source] = sources;
    assert(source, `${pack.id} must have a scenario source`);
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, source)), {
      source,
      expectedLevel: 5,
    });
    const firstCheck = pack.checks[0];
    assert(firstCheck, `${pack.id} must have a check`);
    assert.deepEqual(scenario.features.map(feature => feature.id), [firstCheck.feature]);
    const [feature] = scenario.features;
    assert(feature, `${pack.id} must select a feature`);
    const criteria = new Set(feature.criteria.map(criterion => criterion.id));
    for (const check of pack.checks) {
      for (const criterion of check.criteria ?? []) assert(criteria.has(criterion));
    }
  }
});

test('each prompt states only its feature behavior', () => {
  const byId = Object.fromEntries(packNames.map(name => {
    const pack = readPack(name);
    return [pack.id, fragmentText(requiredFragment(pack.task.requirements[0], 'the task requirement'))];
  }));
  assert.doesNotMatch(requiredText(byId, 'ecommerce.progression.notification-preferences'),
    /promotion|delivery|restock/i);
  assert.doesNotMatch(requiredText(byId, 'ecommerce.progression.promotion-rules'),
    /cart|checkout|report|revenue/i);
  assert.doesNotMatch(requiredText(byId, 'ecommerce.progression.promotion-checkout'),
    /staff|report|revenue|notification/i);
  assert.doesNotMatch(requiredText(byId, 'ecommerce.progression.stock-alerts'),
    /promotion|delivery|order notification/i);
  assert.doesNotMatch(requiredText(byId, 'ecommerce.progression.delivery-notifications'),
    /stock|promotion|report/i);
  assert.doesNotMatch(requiredText(byId, 'ecommerce.progression.promotion-reporting'),
    /notification|stock|create promotion/i);
});

test('promotion rules use values accepted by datetime-local inputs', () => {
  const pack = readPack('progression-promotion-rules-1.0.1.json');
  const firstCheck = pack.checks[0];
  assert(firstCheck, 'promotion rules must have a check');
  const source = firstCheck.source;
  const scenario = compileScenarioDefinition(readJson(join(trackRoot, source)), { source });
  const [feature] = scenario.features;
  assert(feature, 'promotion rules must select a feature');
  const [criterion] = feature.criteria;
  assert(criterion, 'promotion rules must select a criterion');
  const values = criterion.steps
    .filter(step => step.do === 'fill'
      && typeof step.testid === 'string'
      && ['promotion-start', 'promotion-end'].includes(step.testid))
    .map(step => step.text);
  assert.deepEqual(values, ['2099-01-01T00:00', '2099-12-31T23:59']);
});

function requiredFragment(
  fragment: CompiledPackDefinition['task']['requirements'][number] | undefined,
  label: string,
): CompiledPackDefinition['task']['requirements'][number] {
  if (!fragment) throw new Error(`${label} is required`);
  return fragment;
}

function requiredPack(packs: Record<string, CompiledPackDefinition>, id: string): CompiledPackDefinition {
  const pack = packs[id];
  if (!pack) throw new Error(`pack ${id} is required`);
  return pack;
}

function requiredText(texts: Record<string, string>, id: string): string {
  const text = texts[id];
  if (!text) throw new Error(`prompt for ${id} is required`);
  return text;
}
