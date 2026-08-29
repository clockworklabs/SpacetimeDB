import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { compileProgressionDefinitionFile } from '../dist/src/progression/progression-definition.js';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const graphPath = join(trackRoot, 'progression', 'ecommerce-1.0.0.json');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const readPack = name => compilePackDefinition(readJson(join(packRoot, name)), { source: name });

const packNames = [
  'progression-managed-support-1.0.0.json',
  'progression-order-support-1.0.0.json',
  'progression-support-refunds-1.0.0.json',
];

function fragmentText(fragment) {
  const source = readFileSync(join(trackRoot, fragment.path), 'utf8');
  const start = fragment.from ? source.indexOf(fragment.from) : 0;
  const end = fragment.until ? source.indexOf(fragment.until, start + 1) : source.length;
  assert(start >= 0 && end > start, `${fragment.id} must resolve to text`);
  return source.slice(start, end);
}

function selectedCriteria(pack) {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
    });
    assert.deepEqual(scenario.features.map(feature => feature.id), [check.feature]);
    const feature = scenario.features[0];
    assert.equal(feature.criteria.length, 1,
      `${check.source} must isolate one support behavior and its setup`);
    return check.criteria.map(id => {
      const criterion = feature.criteria.find(candidate => candidate.id === id);
      assert(criterion, `${pack.id} must own ${id}`);
      return criterion;
    });
  });
}

test('advanced support packs have exact graph dependencies and focused ownership', () => {
  const [managed, order, refunds] = packNames.map(readPack);
  assert.deepEqual(managed.requiresPacks, [
    'ecommerce.progression.support-history@1.0.0',
    'ecommerce.progression.support-triage@1.0.0',
  ]);
  assert.deepEqual(order.requiresPacks, [
    'ecommerce.progression.managed-support@1.0.0',
    'ecommerce.feature.purchasing@1.2.0',
  ]);
  assert.deepEqual(refunds.requiresPacks, [
    'ecommerce.progression.order-support@1.0.0',
    'ecommerce.l2.order-cancellation-features@1.0.0',
  ]);

  assert.deepEqual(managed.checks.map(check => [check.id, check.criteria]), [
    ['shared-state', ['613a']],
    ['privacy', ['613b']],
  ]);
  assert.deepEqual(order.checks.map(check => [check.id, check.criteria]), [
    ['owned-order', ['614a']],
    ['ownership-boundary', ['614b']],
  ]);
  assert.deepEqual(refunds.checks.map(check => [check.id, check.criteria]), [
    ['resolution', ['615a']],
    ['accounting', ['615b']],
    ['access', ['615c']],
  ]);
  assert.equal(new Set([...managed.checks, ...order.checks, ...refunds.checks]
    .map(check => check.source)).size, 7);
  assert.deepEqual([managed, order, refunds].map(pack =>
    selectedCriteria(pack).reduce((total, criterion) => total + criterion.points, 0)), [3, 5, 4]);
});

test('advanced support product prompts are dedicated and implementation neutral', () => {
  const [managed, order, refunds] = packNames.map(readPack);
  for (const pack of [managed, order, refunds]) {
    assert.equal(pack.state, 'draft');
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.contracts.length, 1);
    assert.deepEqual(pack.task.requirements[0].modes, ['upgrade']);
    assert.deepEqual(pack.task.contracts[0].modes, ['upgrade']);
    const prompt = fragmentText(pack.task.requirements[0]);
    assert.doesNotMatch(prompt, /framework|ORM|database|websocket|endpoint|route|reducer|testid/i);
    assert.doesNotMatch(pack.task.requirements[0].path, /business-features/);
    assert.doesNotMatch(pack.task.contracts[0].path, /business-hooks/);
  }

  assert.match(fragmentText(managed.task.requirements[0]), /shared support case/i);
  assert.match(fragmentText(order.task.requirements[0]), /one of their orders/i);
  assert.match(fragmentText(refunds.task.requirements[0]), /amount paid.*only once/is);
  assert.match(fragmentText(managed.task.contracts[0]), /support-reply-item/);
  assert.match(fragmentText(order.task.contracts[0]), /support-order-option/);
  assert.match(fragmentText(refunds.task.contracts[0]), /supportRefund/);
});

test('the full graph binds every advanced support check to its owner', () => {
  const sourceNodes = new Map(readJson(graphPath).nodes.map(node => [node.id, node]));
  const definition = compileProgressionDefinitionFile(graphPath, { trackRoot });
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
  const packByNode = new Map([
    ['managed-support', readPack(packNames[0])],
    ['order-support', readPack(packNames[1])],
    ['support-refunds', readPack(packNames[2])],
  ]);
  for (const [nodeId, pack] of packByNode) {
    const graphDependencies = sourceNodes.get(nodeId).dependencies
      .flatMap(dependency => sourceNodes.get(dependency.id).featureRefs).sort();
    assert.deepEqual([...pack.requiresPacks].sort(), graphDependencies,
      `${nodeId} pack dependencies must match its graph parents`);
  }
  assert.deepEqual(sourceNodes.get('managed-support').gradingGroups, [
    'ecommerce.progression.managed-support@1.0.0#shared-state',
    'ecommerce.progression.managed-support@1.0.0#privacy',
  ]);
  assert.deepEqual(sourceNodes.get('order-support').gradingGroups, [
    'ecommerce.progression.order-support@1.0.0#owned-order',
    'ecommerce.progression.order-support@1.0.0#ownership-boundary',
  ]);
  assert.deepEqual(sourceNodes.get('support-refunds').gradingGroups, [
    'ecommerce.progression.support-refunds@1.0.0#resolution',
    'ecommerce.progression.support-refunds@1.0.0#accounting',
    'ecommerce.progression.support-refunds@1.0.0#access',
  ]);
  assert.deepEqual([nodes.get('managed-support'), nodes.get('order-support'),
    nodes.get('support-refunds')].map(node => node.gradingChecks
    .reduce((total, check) => total + check.points, 0)), [3, 5, 4]);
  assert.deepEqual([nodes.get('managed-support'), nodes.get('order-support'),
    nodes.get('support-refunds')].map(node => node.gradingChecks.length), [2, 2, 3]);
});
