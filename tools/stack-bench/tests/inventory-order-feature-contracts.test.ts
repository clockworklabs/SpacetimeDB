import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition, type CompiledPackDefinition }
  from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition, type CompiledCriterion, type CompiledFeature,
  type CompiledScenarioDefinition } from '../src/composition/definition-compiler.js';
import type { CompiledProgressionNode } from '../src/progression/progression-definition.js';
import { loadValidatedProgressionSource } from './helpers/progression-source.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const readPack = (name: string): CompiledPackDefinition =>
  compilePackDefinition(readJson(join(packRoot, name)), { source: name });

const packs = {
  transfers: readPack('l2-stock-transfers-features-1.0.1.json'),
  prices: readPack('l2-price-history-features-2.0.1.json'),
  returns: readPack('l3-order-returns-features-1.1.1.json'),
};

function fragmentText(
  fragment: CompiledPackDefinition['task']['requirements'][number],
): string {
  const source = readFileSync(join(trackRoot, fragment.path), 'utf8');
  const start = fragment.from ? source.indexOf(fragment.from) : 0;
  const end = fragment.until ? source.indexOf(fragment.until, start + 1) : source.length;
  assert(start >= 0 && end > start, `${fragment.id} must resolve to text`);
  return source.slice(start, end);
}

function selectedCriteria(pack: CompiledPackDefinition): Array<{
  check: CompiledPackDefinition['checks'][number];
  criterion: CompiledCriterion;
  feature: CompiledFeature;
  scenario: CompiledScenarioDefinition;
}> {
  return pack.checks.flatMap(check => {
    if (!check.criteria || check.criteria.length === 0) {
      throw new Error(`${pack.id}.${check.id} must select explicit criteria`);
    }
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
    });
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    assert(feature, `${pack.id}.${check.id} must select one feature`);
    return check.criteria.map(id => {
      const criterion = feature.criteria.find(candidate => candidate.id === id);
      assert(criterion, `${pack.id}.${check.id} must select ${id}`);
      return { check, criterion, feature, scenario };
    });
  });
}

test('inventory and order nodes use the expected direct graph parents', () => {
  assert.deepEqual(packs.transfers.requiresPacks,
    ['ecommerce.feature.warehouse-admin@1.2.1']);
  assert.deepEqual(packs.prices.requiresPacks, [
    'ecommerce.progression.catalog-management@1.0.2',
  ]);
  assert.deepEqual(packs.returns.requiresPacks, [
    'ecommerce.l3.order-delivery-features@1.1.1',
    'ecommerce.feature.warehouse-admin@1.2.1',
  ]);

  const { definition } = loadValidatedProgressionSource(
    join(trackRoot, 'progression', 'ecommerce-2.0.1.json'), trackRoot);
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
  const expectedParents = new Map([
    ['stock-transfers', ['warehouse-admin']],
    ['price-history', ['catalog-management']],
    ['order-returns', ['order-delivery']],
  ]);
  for (const [nodeId, parents] of expectedParents) {
    const node = requiredNode(nodes, nodeId);
    assert.deepEqual([...node.dependencies].sort(), parents);
  }
});

test('each node has a dedicated product prompt and testing interface', () => {
  const expectedPaths = new Map<CompiledPackDefinition, readonly [string, string]>([
    [packs.transfers, ['prompts/modular/stock-transfers-1.0.0.md',
      'contracts/stock-transfers-1.0.md']],
    [packs.prices, ['prompts/modular/price-history-1.0.0.md',
      'contracts/price-history-2.0.md']],
    [packs.returns, ['prompts/modular/order-returns-1.0.0.md',
      'contracts/order-returns-1.0.md']],
  ]);
  for (const [pack, [promptPath, contractPath]] of expectedPaths) {
    assert.equal(pack.task.requirements.length, 1);
    const requirement = requiredRequirement(pack);
    const contract = requiredContract(pack);
    assert.equal(requirement.path, promptPath);
    assert.equal(pack.task.contracts.length, 1);
    assert.equal(contract.path, contractPath);
    assert.deepEqual(requirement.modes, ['upgrade']);
    assert.deepEqual(contract.modes, ['upgrade']);
    assert.doesNotMatch(fragmentText(requirement),
      /framework|ORM|MongoDB|PostgreSQL|SpacetimeDB|endpoint|reducer/i);
  }

  assert.doesNotMatch(fragmentText(requiredRequirement(packs.transfers)),
    /price|return|cancel|delivery/i);
  assert.doesNotMatch(fragmentText(requiredRequirement(packs.prices)),
    /transfer|return|cancel|delivery/i);
  assert.doesNotMatch(fragmentText(requiredRequirement(packs.returns)),
    /transfer|cart|catalog|promotion/i);

  const transferContract = fragmentText(requiredContract(packs.transfers));
  for (const hook of ['transfer-from', 'transfer-to', 'transfer-qty', 'transfer-submit',
    'data-transfer-input', 'order-error']) assert.match(transferContract, new RegExp(hook));
  const priceContract = fragmentText(requiredContract(packs.prices));
  for (const hook of ['price-input', 'price-submit', 'data-price-input']) {
    assert.match(priceContract, new RegExp(hook));
  }
  const returnContract = fragmentText(requiredContract(packs.returns));
  for (const hook of ['return-item', 'order-item', 'item-stock', 'admin-revenue']) {
    assert.match(returnContract, new RegExp(hook));
  }
});

test('checks preserve established points and isolate the missing return boundary', () => {
  const transfers = selectedCriteria(packs.transfers);
  const prices = selectedCriteria(packs.prices);
  const returns = selectedCriteria(packs.returns);
  assert.equal(transfers.reduce((total, item) => total + item.criterion.points, 0), 7);
  assert.equal(prices.reduce((total, item) => total + item.criterion.points, 0), 8);
  assert.deepEqual(returns.map(item => [item.criterion.id, item.criterion.points]),
    [['3c', 3], ['3e', 1]]);

  for (const item of [...prices, ...returns]) {
    if (item.check.source.startsWith('scenarios/progression-')) {
      assert.equal(item.scenario.features.length, 1,
        `${item.check.source} must contain only the selected feature`);
    }
  }
  const boundary = returns.find(item => item.criterion.id === '3e');
  assert(boundary, 'order returns must include criterion 3e');
  assert(boundary.criterion.steps
    .some(step => step.testid === 'return-item' && step.absent === true));
});

test('the graph owns every check group from these feature packs', () => {
  const { definition, gradingGroups } = loadValidatedProgressionSource(
    join(trackRoot, 'progression', 'ecommerce-2.0.1.json'), trackRoot);
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
  const packsByNode = new Map<string, CompiledPackDefinition>([
    ['stock-transfers', packs.transfers],
    ['price-history', packs.prices],
    ['order-returns', packs.returns],
  ]);
  for (const [nodeId, pack] of packsByNode) {
    const node = requiredNode(nodes, nodeId);
    const groups = new Set(gradingGroups(node.id));
    for (const check of pack.checks) {
      assert(groups.has(`${pack.id}@${pack.version}#${check.id}`),
        `${nodeId} must grade ${check.id}`);
    }
  }
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
