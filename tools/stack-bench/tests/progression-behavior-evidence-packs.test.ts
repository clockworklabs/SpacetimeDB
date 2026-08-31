import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition, type CompiledPackDefinition }
  from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition, type CompiledCriterion, type CompiledFeature }
  from '../src/composition/definition-compiler.js';
import { compileProgressionDefinitionFile, type CompiledProgressionNode }
  from '../src/progression/progression-definition.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const readPack = (name: string): CompiledPackDefinition =>
  compilePackDefinition(readJson(join(packRoot, name)), { source: name });
const packNames = [
  'progression-automatic-reorder-2.0.0.json',
  'progression-cart-recovery-2.0.0.json',
  'progression-personalized-recommendations-2.0.0.json',
];

function fragmentText(
  fragment: CompiledPackDefinition['task']['requirements'][number],
): string {
  return readFileSync(join(trackRoot, fragment.path), 'utf8');
}

function featureFor(pack: CompiledPackDefinition): CompiledFeature {
  const check = pack.checks[0];
  if (!check) throw new Error(`${pack.id} must have a check`);
  const source = check.source;
  const scenario = compileScenarioDefinition(readJson(join(trackRoot, source)), { source });
  const feature = scenario.features.find(candidate => candidate.id === check.feature);
  assert(feature, `${pack.id}.${check.id} must select a feature`);
  return feature;
}

function criterion(feature: CompiledFeature, id: string): CompiledCriterion {
  const selected = feature.criteria.find(item => item.id === id);
  assert(selected, `feature ${feature.id} must contain criterion ${id}`);
  return selected;
}

test('the three behavior packs have isolated, non-prescriptive contracts', () => {
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
    const text = `${fragmentText(requiredRequirement(pack))}\n${fragmentText(requiredContract(pack))}`;
    assert.doesNotMatch(text, /framework|ORM|database|websocket|table|query|endpoint/i);
  }
  assert.equal(new Set(packs.map(pack => requiredRequirement(pack).path)).size, packs.length);
  assert.equal(new Set(packs.map(pack => requiredContract(pack).path)).size, packs.length);
});

test('the three behavior packs declare exact dependencies and check ownership', () => {
  const packs = new Map(packNames.map(name => {
    const pack = readPack(name);
    return [pack.id, pack];
  }));

  const reorder = requiredPack(packs, 'ecommerce.progression.automatic-reorder');
  const recovery = requiredPack(packs, 'ecommerce.progression.cart-recovery');
  const recommendations = requiredPack(packs, 'ecommerce.progression.personalized-recommendations');
  assert.deepEqual(reorder.requiresPacks,
    ['ecommerce.l3.scheduled-restocks-features@1.1.1',
      'ecommerce.progression.staff-roles@1.0.0']);
  assert.deepEqual(recovery.requiresPacks,
    ['ecommerce.l3.cart-expiration-features@2.0.0']);
  assert.deepEqual(recommendations.requiresPacks,
    ['ecommerce.l2.recommendations@2.0.0']);

  assert.deepEqual(reorder.checks
    .map(check => [check.id, check.criteria]),
  [['threshold', ['502a', '502b']], ['access', ['502c']]]);
  assert.deepEqual(recovery.checks
    .map(check => [check.id, check.criteria]),
  [['available', ['503a']], ['partial', ['503b']]]);
  assert.deepEqual(recommendations.checks
    .map(check => [check.id, check.criteria]),
  [['ordering', ['403a']], ['isolation', ['403b']]]);

  for (const pack of packs.values()) {
    const feature = featureFor(pack);
    const criteria = new Set(feature.criteria.map(item => item.id));
    const selected = pack.checks.flatMap(check => {
      if (!check.criteria) throw new Error(`${pack.id}.${check.id} must select criteria`);
      return check.criteria;
    });
    assert.equal(new Set(selected).size, selected.length);
    assert.deepEqual(new Set(selected), criteria);
  }
});

test('automatic reorder proves staff access and rejects a customer replay', () => {
  const pack = readPack(requiredPackName(0));
  const feature = featureFor(pack);
  assert.equal(feature.setup.filter(step => step.do === 'dbSetStock'
    && step.item === 'Mirrorless Camera').reduce((total, step) =>
    total + (typeof step.quantity === 'number' ? step.quantity : 0), 0), 3);
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
  const pack = readPack(requiredPackName(1));
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
  const pack = readPack(requiredPackName(2));
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

test('the graph selects every behavior check and required dependency', () => {
  const definition = compileProgressionDefinitionFile(
    join(trackRoot, 'progression', 'ecommerce-2.0.1.json'), { trackRoot });
  const byId = new Map(definition.nodes.map(node => [node.id, node]));

  assert.deepEqual(requiredNode(byId, 'automatic-reorder').dependencies,
    ['scheduled-restocks', 'staff-roles']);
  assert.equal(requiredNode(byId, 'automatic-reorder').gradingChecks.length, 3);
  assert.equal(requiredNode(byId, 'cart-recovery').gradingChecks.length, 2);
  assert.equal(requiredNode(byId, 'personalized-recommendations').gradingChecks.length, 2);
});

function requiredPackName(index: number): string {
  const name = packNames[index];
  if (!name) throw new Error(`behavior pack name ${index} is required`);
  return name;
}

function requiredPack(
  packs: ReadonlyMap<string, CompiledPackDefinition>,
  packId: string,
): CompiledPackDefinition {
  const pack = packs.get(packId);
  if (!pack) throw new Error(`pack ${packId} is required`);
  return pack;
}

function requiredRequirement(
  pack: CompiledPackDefinition,
): CompiledPackDefinition['task']['requirements'][number] {
  const requirement = pack.task.requirements[0];
  if (!requirement) throw new Error(`${pack.id} must have a product requirement`);
  return requirement;
}

function requiredContract(
  pack: CompiledPackDefinition,
): CompiledPackDefinition['task']['contracts'][number] {
  const contract = pack.task.contracts[0];
  if (!contract) throw new Error(`${pack.id} must have a testing contract`);
  return contract;
}

function requiredNode(
  nodes: ReadonlyMap<string, CompiledProgressionNode>,
  nodeId: string,
): CompiledProgressionNode {
  const node = nodes.get(nodeId);
  if (!node) throw new Error(`progression node ${nodeId} is required`);
  return node;
}
