import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition, resolveTaskFragment, type CompiledPackDefinition }
  from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition, type CompiledCriterion }
  from '../src/composition/definition-compiler.js';
import type { CompiledProgressionNode } from '../src/progression/progression-definition.js';
import { loadValidatedProgressionSource } from './helpers/progression-source.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const graphPath = join(trackRoot, 'progression', 'ecommerce-2.0.1.json');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const readPack = (name: string): CompiledPackDefinition =>
  compilePackDefinition(readJson(join(packRoot, name)), { source: name });

const managedPackName = 'progression-managed-support-1.0.0.json';
const orderPackName = 'progression-order-support-1.0.1.json';
const refundsPackName = 'progression-support-refunds-1.0.1.json';

function fragmentText(
  fragment: CompiledPackDefinition['task']['requirements'][number],
): string {
  return resolveTaskFragment(fragment, { trackRoot, source: fragment.id }).text;
}

function selectedCriteria(pack: CompiledPackDefinition): CompiledCriterion[] {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
    });
    assert.deepEqual(scenario.features.map(feature => feature.id), [check.feature]);
    const feature = scenario.features[0];
    assert(feature, `${check.source} must contain its selected feature`);
    assert.equal(feature.criteria.length, 1,
      `${check.source} must isolate one support behavior and its setup`);
    if (!check.criteria) throw new Error(`${pack.id}.${check.id} must select criteria`);
    return check.criteria.map(id => {
      const criterion = feature.criteria.find(candidate => candidate.id === id);
      assert(criterion, `${pack.id} must own ${id}`);
      return criterion;
    });
  });
}

test('advanced support packs have exact graph dependencies and focused ownership', () => {
  const managed = readPack(managedPackName);
  const order = readPack(orderPackName);
  const refunds = readPack(refundsPackName);
  assert.deepEqual(managed.requiresPacks, [
    'ecommerce.progression.support-history@1.0.0',
    'ecommerce.progression.support-triage@1.0.0',
  ]);
  assert.deepEqual(order.requiresPacks, [
    'ecommerce.progression.managed-support@1.0.0',
    'ecommerce.feature.purchasing@1.2.1',
  ]);
  assert.deepEqual(refunds.requiresPacks, [
    'ecommerce.progression.order-support@1.0.1',
    'ecommerce.l2.order-cancellation-features@1.0.1',
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
  const managed = readPack(managedPackName);
  const order = readPack(orderPackName);
  const refunds = readPack(refundsPackName);
  for (const pack of [managed, order, refunds]) {
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.contracts.length, 1);
    const requirement = pack.task.requirements[0];
    const contract = pack.task.contracts[0];
    assert(requirement, `${pack.id} must have a product requirement`);
    assert(contract, `${pack.id} must have a testing contract`);
    assert.deepEqual(requirement.modes, ['upgrade']);
    assert.deepEqual(contract.modes, ['upgrade']);
    const prompt = fragmentText(requirement);
    assert.doesNotMatch(prompt, /framework|ORM|database|websocket|endpoint|route|reducer|testid/i);
    assert.doesNotMatch(requirement.path, /business-features/);
    assert.doesNotMatch(contract.path, /business-hooks/);
  }

  assert.match(fragmentText(requiredRequirement(managed)), /shared support case/i);
  assert.match(fragmentText(requiredRequirement(order)), /one of their orders/i);
  assert.match(fragmentText(requiredRequirement(refunds)), /amount paid.*only once/is);
  assert.match(fragmentText(requiredContract(managed)), /support-reply-item/);
  assert.match(fragmentText(requiredContract(order)), /support-order-option/);
  assert.match(fragmentText(requiredContract(refunds)), /supportRefund/);
});

test('the full graph binds every advanced support check to its owner', () => {
  const { definition, gradingGroups } = loadValidatedProgressionSource(graphPath, trackRoot);
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
  const packByNode = new Map<string, CompiledPackDefinition>([
    ['managed-support', readPack(managedPackName)],
    ['order-support', readPack(orderPackName)],
    ['support-refunds', readPack(refundsPackName)],
  ]);
  for (const [nodeId, pack] of packByNode) {
    const node = requiredNode(nodes, nodeId);
    const graphDependencies = node.dependencies
      .flatMap(dependency => requiredNode(nodes, dependency).featureRefs).sort();
    assert.deepEqual([...pack.requiresPacks].sort(), graphDependencies,
      `${nodeId} pack dependencies must match its graph parents`);
  }
  assert.deepEqual(gradingGroups('managed-support'), [
    'ecommerce.progression.managed-support@1.0.0#shared-state',
    'ecommerce.progression.managed-support@1.0.0#privacy',
  ]);
  assert.deepEqual(gradingGroups('order-support'), [
    'ecommerce.progression.order-support@1.0.1#owned-order',
    'ecommerce.progression.order-support@1.0.1#ownership-boundary',
  ]);
  assert.deepEqual(gradingGroups('support-refunds'), [
    'ecommerce.progression.support-refunds@1.0.1#resolution',
    'ecommerce.progression.support-refunds@1.0.1#accounting',
    'ecommerce.progression.support-refunds@1.0.1#access',
  ]);
  const supportNodes = ['managed-support', 'order-support', 'support-refunds']
    .map(nodeId => requiredNode(nodes, nodeId));
  assert.deepEqual(supportNodes.map(node => node.gradingChecks
    .reduce((total, check) => total + check.points, 0)), [3, 5, 4]);
  assert.deepEqual(supportNodes.map(node => node.gradingChecks.length), [2, 2, 3]);
});

function requiredNode(
  nodes: ReadonlyMap<string, CompiledProgressionNode>,
  nodeId: string,
): CompiledProgressionNode {
  const node = nodes.get(nodeId);
  if (!node) throw new Error(`progression node ${nodeId} is required`);
  return node;
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
