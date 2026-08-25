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

const splitNames = [
  'l2-stock-transfers-features-1.0.0.json',
  'l2-operational-views-features-1.0.0.json',
  'l2-cancellation-returns-features-1.0.0.json',
  'l2-price-history-features-1.0.0.json',
];

function selectedChecks(pack) {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
      expectedLevel: 2,
    });
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    const criteria = check.criteria === undefined
      ? feature.criteria
      : check.criteria.map(id => feature.criteria.find(criterion => criterion.id === id));
    return criteria.map(criterion => ({
      key: `${pack.stableId ?? pack.id}.${check.stableId ?? check.id}.${criterion.id}`,
      points: criterion.points ?? 1,
    }));
  }).sort((left, right) => left.key.localeCompare(right.key));
}

function fragmentText(fragment) {
  const source = readFileSync(join(trackRoot, fragment.path), 'utf8');
  const start = fragment.from ? source.indexOf(fragment.from) : 0;
  const end = fragment.until ? source.indexOf(fragment.until, start + 1) : source.length;
  assert(start >= 0 && end > start, `${fragment.id} must resolve to text`);
  return source.slice(start, end);
}

test('split L2 feature packs preserve every established feature check and point', () => {
  const split = splitNames.flatMap(name => selectedChecks(readPack(name)));
  const established = [
    'inventory-operations-features-1.2.0.json',
    'returns-pricing-features-1.1.0.json',
  ].flatMap(name => selectedChecks(readPack(name)))
    .sort((left, right) => left.key.localeCompare(right.key));
  assert.deepEqual(split.sort((left, right) => left.key.localeCompare(right.key)), established);
});

test('each split pack owns only its prompt and exact dependencies', () => {
  const packs = Object.fromEntries(splitNames.map(name => {
    const pack = readPack(name);
    assert.equal(pack.state, 'draft');
    assert.equal(pack.moduleType, 'feature');
    return [pack.id, pack];
  }));

  const cancellation = packs['ecommerce.l2.cancellation-returns-features'];
  const pricing = packs['ecommerce.l2.price-history-features'];
  const transfers = packs['ecommerce.l2.stock-transfers-features'];
  const views = packs['ecommerce.l2.operational-views-features'];

  assert.deepEqual(cancellation.requiresPacks, ['ecommerce.operations-access-features@1.0.0']);
  assert.deepEqual(pricing.requiresPacks,
    ['ecommerce.feature.catalog@1.1.0', 'ecommerce.feature.purchasing@1.1.0']);
  assert.deepEqual(transfers.requiresPacks, ['ecommerce.feature.warehouse-admin@1.1.0']);
  assert.deepEqual(views.requiresPacks,
    ['ecommerce.feature.purchasing@1.1.0', 'ecommerce.feature.warehouse-admin@1.1.0']);

  assert.doesNotMatch(fragmentText(cancellation.task.requirements[0]), /### Prices|Live operational views/);
  assert.doesNotMatch(fragmentText(pricing.task.requirements[0]), /Cancelling and returning|Live operational views/);
  assert.doesNotMatch(fragmentText(transfers.task.requirements[0]), /Cancelling and returning|Live operational views/);
  assert.doesNotMatch(fragmentText(views.task.requirements[0]), /cancel|return|price/i);
});
