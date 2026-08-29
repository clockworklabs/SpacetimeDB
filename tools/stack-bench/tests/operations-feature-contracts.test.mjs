import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const readPack = name => compilePackDefinition(
  readJson(join(trackRoot, 'composition', 'packs', name)), { source: name });

function fragmentText(fragment) {
  const text = readFileSync(join(trackRoot, fragment.path), 'utf8');
  const start = fragment.from ? text.indexOf(fragment.from) : 0;
  const end = fragment.until ? text.indexOf(fragment.until, start + 1) : text.length;
  assert(start >= 0 && end > start, `${fragment.id} must select text`);
  return text.slice(start, end);
}

function selectedCriteria(pack) {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
      expectedLevel: 2,
    });
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    assert(feature, `${pack.id}.${check.id} must select a feature`);
    return check.criteria.map(id => {
      const criterion = feature.criteria.find(candidate => candidate.id === id);
      assert(criterion, `${pack.id}.${check.id} must select ${id}`);
      return { check, feature, criterion };
    });
  });
}

test('operations access owns only fulfilment and order access behavior', () => {
  const pack = readPack('operations-access-1.1.0.json');
  assert.equal(pack.state, 'draft');
  assert.deepEqual(pack.requiresPacks,
    ['ecommerce.feature.purchasing@1.0.0', 'ecommerce.feature.warehouse-admin@1.0.0']);
  assert.deepEqual(pack.checks.map(check => [check.id, check.criteria]), [
    ['fulfilment-live', ['1a']],
    ['fulfilment-warehouse', ['1b']],
    ['fulfilment-ship', ['1c']],
    ['fulfilment-access', ['1d']],
    ['operator-authorization-direct', ['201c']],
    ['order-owner-direct', ['204a']],
  ]);
  const prompt = fragmentText(pack.task.requirements[0]);
  assert.doesNotMatch(prompt, /transfer|price|return|operational view|framework|database/i);
  const contract = fragmentText(pack.task.contracts[0]);
  for (const value of ['staff-link', 'queue-depth', 'queue-item', 'queue-warehouse',
    'ship-submit', 'data-ship-input', 'data-cancel-input']) assert.match(contract, new RegExp(value));
  assert.equal(selectedCriteria(pack)
    .reduce((total, item) => total + item.criterion.points, 0), 10);
});

test('fulfilment checks use focused setup and the draft recipe runs every source', () => {
  const pack = readPack('operations-access-1.1.0.json');
  const selected = selectedCriteria(pack);
  const featureChecks = selected.filter(item => item.check.role === 'feature');
  assert(featureChecks.every(item => item.feature.criteria.length === 1));

  const recipe = readJson(join(trackRoot, 'composition', 'recipes', 'l2-standard-1.3.0.json'));
  const sources = new Set(recipe.execution.map(entry => entry.source));
  for (const check of pack.checks) assert(sources.has(check.source), `${check.source} must run`);
});

test('operational views ask for and grade the same four views', () => {
  const pack = readPack('l2-operational-views-features-1.0.0.json');
  assert.equal(pack.state, 'draft');
  assert.deepEqual(pack.requiresPacks, [
    'ecommerce.feature.cart-checkout@1.3.0',
    'ecommerce.feature.purchasing@1.2.0',
    'ecommerce.feature.warehouse-admin@1.2.0',
  ]);
  assert.deepEqual(pack.checks.map(check => [check.id, check.criteria]), [
    ['operational-views-low-stock', ['5a']],
    ['operational-views-category-totals', ['5b']],
    ['operational-views-recommendations', ['5c']],
    ['operational-views-best-sellers', ['5d']],
  ]);
  const prompt = fragmentText(pack.task.requirements[0]);
  assert.match(prompt, /Low stock/);
  assert.match(prompt, /Category totals/);
  assert.match(prompt, /Recommended for you/);
  assert.match(prompt, /best sellers/);
  assert.doesNotMatch(prompt, /warehouse utilisation|fulfilment queue depth/i);
  const selected = selectedCriteria(pack);
  assert.equal(selected.reduce((total, item) => total + item.criterion.points, 0), 9);
  assert(selected.every(item => item.feature.criteria.length === 1));
});

test('the operational views graph node has every intrinsic parent and exact grading group', () => {
  const definition = readJson(join(trackRoot, 'progression', 'ecommerce-1.0.0.json'));
  const node = definition.nodes.find(candidate => candidate.id === 'operational-views');
  assert.deepEqual(node.dependencies.map(dependency => dependency.id).sort(),
    ['cart-checkout', 'purchasing', 'warehouse-admin']);
  assert.deepEqual(node.gradingGroups, [
    'ecommerce.l2.operational-views-features@1.0.0#operational-views-best-sellers',
    'ecommerce.l2.operational-views-features@1.0.0#operational-views-category-totals',
    'ecommerce.l2.operational-views-features@1.0.0#operational-views-low-stock',
    'ecommerce.l2.operational-views-features@1.0.0#operational-views-recommendations',
  ]);
});
