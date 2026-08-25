import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const readPack = name => compilePackDefinition(readJson(join(packRoot, name)), { source: name });

const packs = {
  transfers: readPack('l2-stock-transfers-features-1.0.0.json'),
  prices: readPack('l2-price-history-features-1.0.0.json'),
  returns: readPack('l3-order-returns-features-1.1.0.json'),
};

function fragmentText(fragment) {
  const source = readFileSync(join(trackRoot, fragment.path), 'utf8');
  const start = fragment.from ? source.indexOf(fragment.from) : 0;
  const end = fragment.until ? source.indexOf(fragment.until, start + 1) : source.length;
  assert(start >= 0 && end > start, `${fragment.id} must resolve to text`);
  return source.slice(start, end);
}

function selectedCriteria(pack) {
  return pack.checks.flatMap(check => {
    assert(check.criteria?.length > 0, `${pack.id}.${check.id} must select explicit criteria`);
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

test('inventory and order nodes use exact graph parent packs', () => {
  assert.deepEqual(packs.transfers.requiresPacks,
    ['ecommerce.feature.warehouse-admin@1.2.0']);
  assert.deepEqual(packs.prices.requiresPacks, [
    'ecommerce.feature.cart-checkout@1.3.0',
    'ecommerce.feature.purchasing@1.2.0',
    'ecommerce.progression.catalog-management@1.0.0',
  ]);
  assert.deepEqual(packs.returns.requiresPacks, [
    'ecommerce.l3.order-delivery-features@1.1.0',
    'ecommerce.feature.warehouse-admin@1.2.0',
  ]);

  const definition = readJson(join(trackRoot, 'progression', 'ecommerce-1.0.0.json'));
  const expectedParents = new Map([
    ['stock-transfers', ['warehouse-admin']],
    ['price-history', ['cart-checkout', 'catalog-management', 'purchasing']],
    ['order-returns', ['order-delivery', 'warehouse-admin']],
  ]);
  for (const [nodeId, parents] of expectedParents) {
    const node = definition.nodes.find(candidate => candidate.id === nodeId);
    assert.deepEqual(node.dependencies.map(item => item.id).sort(), parents);
  }
});

test('each node has a dedicated product prompt and testing interface', () => {
  const expectedPaths = new Map([
    [packs.transfers, ['prompts/modular/stock-transfers-1.0.0.md',
      'contracts/stock-transfers-1.0.md']],
    [packs.prices, ['prompts/modular/price-history-1.0.0.md',
      'contracts/price-history-1.0.md']],
    [packs.returns, ['prompts/modular/order-returns-1.0.0.md',
      'contracts/order-returns-1.0.md']],
  ]);
  for (const [pack, [promptPath, contractPath]] of expectedPaths) {
    assert.equal(pack.state, 'draft');
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.requirements[0].path, promptPath);
    assert.equal(pack.task.contracts.length, 1);
    assert.equal(pack.task.contracts[0].path, contractPath);
    assert.deepEqual(pack.task.requirements[0].modes, ['upgrade']);
    assert.deepEqual(pack.task.contracts[0].modes, ['upgrade']);
    assert.doesNotMatch(fragmentText(pack.task.requirements[0]),
      /framework|ORM|MongoDB|PostgreSQL|SpacetimeDB|endpoint|reducer/i);
  }

  assert.doesNotMatch(fragmentText(packs.transfers.task.requirements[0]),
    /price|return|cancel|delivery/i);
  assert.doesNotMatch(fragmentText(packs.prices.task.requirements[0]),
    /transfer|return|cancel|delivery/i);
  assert.doesNotMatch(fragmentText(packs.returns.task.requirements[0]),
    /transfer|cart|catalog|promotion/i);

  const transferContract = fragmentText(packs.transfers.task.contracts[0]);
  for (const hook of ['transfer-from', 'transfer-to', 'transfer-qty', 'transfer-submit',
    'data-transfer-input', 'order-error']) assert.match(transferContract, new RegExp(hook));
  const priceContract = fragmentText(packs.prices.task.contracts[0]);
  for (const hook of ['price-input', 'price-submit', 'data-price-input']) {
    assert.match(priceContract, new RegExp(hook));
  }
  const returnContract = fragmentText(packs.returns.task.contracts[0]);
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
  const boundary = returns.find(item => item.criterion.id === '3e').criterion;
  assert(boundary.steps.some(step => step.testid === 'return-item' && step.absent === true));
});

test('the graph owns every check group from these feature packs', () => {
  const definition = readJson(join(trackRoot, 'progression', 'ecommerce-1.0.0.json'));
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
  for (const [nodeId, pack] of [
    ['stock-transfers', packs.transfers],
    ['price-history', packs.prices],
    ['order-returns', packs.returns],
  ]) {
    const groups = new Set(nodes.get(nodeId).gradingGroups);
    for (const check of pack.checks) {
      assert(groups.has(`${pack.id}@${pack.version}#${check.id}`),
        `${nodeId} must grade ${check.id}`);
    }
  }
});
