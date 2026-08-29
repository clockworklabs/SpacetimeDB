import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { compileProgressionDefinitionFile } from '../dist/src/progression/progression-definition.js';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const readPack = name => compilePackDefinition(readJson(join(packRoot, name)), { source: name });
const packNames = [
  'progression-automatic-reorder-1.0.0.json',
  'progression-cart-recovery-1.0.0.json',
  'progression-personalized-recommendations-1.0.0.json',
];

function fragmentText(fragment) {
  return readFileSync(join(trackRoot, fragment.path), 'utf8');
}

function featureFor(pack) {
  const source = pack.checks[0].source;
  const scenario = compileScenarioDefinition(readJson(join(trackRoot, source)), { source });
  return scenario.features.find(feature => feature.id === pack.checks[0].feature);
}

function criterion(feature, id) {
  return feature.criteria.find(item => item.id === id);
}

test('the three behavior packs have isolated, non-prescriptive contracts', () => {
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
    assert.doesNotMatch(text, /framework|ORM|database|websocket|table|query|endpoint/i);
  }
  assert.equal(new Set(packs.map(pack => pack.task.requirements[0].path)).size, packs.length);
  assert.equal(new Set(packs.map(pack => pack.task.contracts[0].path)).size, packs.length);
});

test('the three behavior packs declare exact dependencies and check ownership', () => {
  const packs = Object.fromEntries(packNames.map(name => {
    const pack = readPack(name);
    return [pack.id, pack];
  }));

  assert.deepEqual(packs['ecommerce.progression.automatic-reorder'].requiresPacks,
    ['ecommerce.l3.scheduled-restocks-features@1.1.0',
      'ecommerce.l2.operational-views-features@1.0.0',
      'ecommerce.progression.staff-roles@1.0.0']);
  assert.deepEqual(packs['ecommerce.progression.cart-recovery'].requiresPacks,
    ['ecommerce.l3.cart-expiration-features@1.1.0']);
  assert.deepEqual(packs['ecommerce.progression.personalized-recommendations'].requiresPacks,
    ['ecommerce.feature.purchasing@1.2.0',
      'ecommerce.l2.operational-views-features@1.0.0']);

  assert.deepEqual(packs['ecommerce.progression.automatic-reorder'].checks
    .map(check => [check.id, check.criteria]),
  [['threshold', ['502a', '502b']], ['access', ['502c']]]);
  assert.deepEqual(packs['ecommerce.progression.cart-recovery'].checks
    .map(check => [check.id, check.criteria]),
  [['available', ['503a']], ['partial', ['503b']]]);
  assert.deepEqual(packs['ecommerce.progression.personalized-recommendations'].checks
    .map(check => [check.id, check.criteria]),
  [['ordering', ['403a']], ['isolation', ['403b']]]);

  for (const pack of Object.values(packs)) {
    const feature = featureFor(pack);
    const criteria = new Set(feature.criteria.map(item => item.id));
    const selected = pack.checks.flatMap(check => check.criteria);
    assert.equal(new Set(selected).size, selected.length);
    assert.deepEqual(new Set(selected), criteria);
  }
});

test('automatic reorder proves staff access and rejects a customer replay', () => {
  const pack = readPack(packNames[0]);
  const feature = featureFor(pack);
  assert.equal(feature.setup.filter(step => step.do === 'dbSetStock'
    && step.item === 'Mirrorless Camera').reduce((total, step) => total + step.quantity, 0), 3);
  assert(feature.setup.some(step => step.do === 'signIn' && step.actor === 'staff'));
  assert(feature.setup.some(step => step.do === 'click' && step.actor === 'staff'
    && step.testid === 'reorder-submit'));
  const access = criterion(feature, '502c').steps;
  assert(access.some(step => step.do === 'expect' && step.actor === 'customer'
    && step.testid === 'reorder-link' && step.absent === true));
  assert(access.some(step => step.do === 'replayAs' && step.actor === 'customer'
    && step.from === 'staff'));
  assert(access.some(step => step.do === 'expectReplayRejected' && step.actor === 'customer'));
  assert(access.some(step => step.do === 'expectElementCount'
    && step.testid === 'reorder-rule-item' && step.equals === 1));
});

test('cart recovery proves complete and partial restoration', () => {
  const pack = readPack(packNames[1]);
  const feature = featureFor(pack);
  const available = criterion(feature, '503a').steps;
  assert(available.some(step => step.do === 'expect' && step.testid === 'cart-item'
    && step.contains === 'Keyboard'));
  assert(available.some(step => step.do === 'expect' && step.testid === 'cart-restore-warning'
    && step.absent === true));

  const partial = criterion(feature, '503b').steps;
  assert.equal(partial.filter(step => step.do === 'dbSetStock'
    && step.item === 'Mirrorless Camera' && step.quantity === 0).length, 2);
  assert(partial.some(step => step.do === 'expect' && step.testid === 'cart-item'
    && step.contains === 'Webcam'));
  assert(partial.some(step => step.do === 'expect' && step.testid === 'cart-item'
    && step.contains === 'Mirrorless Camera' && step.absent === true));
  assert(partial.some(step => step.do === 'expect' && step.testid === 'cart-restore-warning'
    && step.contains === 'Mirrorless Camera'));
});

test('personalized recommendations prove sales ordering, name ties, and isolation', () => {
  const pack = readPack(packNames[2]);
  const feature = featureFor(pack);
  assert.equal(feature.setup.filter(step => step.do === 'click'
    && step.actor === 'sales-helper' && step.contains === undefined
    && step.in?.contains === 'Desk Lamp').length, 3);
  const expectedNameOrder = ['Gaming Mouse', 'Laptop Stand', 'Webcam'];
  const ordering = criterion(feature, '403a').steps;
  assert(ordering.some(step => step.do === 'expectNumber'
    && step.actor === 'home-customer' && step.testid === 'recommendation-rank'
    && step.in?.contains === 'Desk Lamp' && step.equals === 1));
  assert(ordering.some(step => step.do === 'expectSequence'
    && step.actor === 'computing-customer'
    && JSON.stringify(step.equals) === JSON.stringify(expectedNameOrder)));
  assert(ordering.some(step => step.do === 'expect' && step.actor === 'audio-customer'
    && step.contains === 'Headphones'));

  const isolation = criterion(feature, '403b').steps;
  assert(isolation.some(step => step.do === 'expect' && step.actor === 'audio-customer'
    && step.contains === 'Headphones' && step.absent === true));
  assert(isolation.some(step => step.do === 'expectNumber'
    && step.actor === 'home-customer' && step.testid === 'recommendation-rank'
    && step.in?.contains === 'Desk Lamp' && step.equals === 1));
  assert(isolation.some(step => step.do === 'expectSequence'
    && step.actor === 'computing-customer'
    && JSON.stringify(step.equals) === JSON.stringify(expectedNameOrder)));
});

test('the graph selects every behavior check and the intrinsic staff dependency', () => {
  const definition = compileProgressionDefinitionFile(
    join(trackRoot, 'progression', 'ecommerce-1.0.0.json'), { trackRoot });
  const byId = new Map(definition.nodes.map(node => [node.id, node]));

  assert.deepEqual(byId.get('automatic-reorder').dependencies,
    ['operational-views', 'scheduled-restocks', 'staff-roles']);
  assert.equal(byId.get('automatic-reorder').gradingChecks.length, 3);
  assert.equal(byId.get('cart-recovery').gradingChecks.length, 2);
  assert.equal(byId.get('personalized-recommendations').gradingChecks.length, 2);
});
