import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const readPack = name => compilePackDefinition(
  readJson(join(trackRoot, 'composition', 'packs', name)), { source: name });

function fragmentText(fragment) {
  assert.equal(fragment.from, undefined, `${fragment.id} must use a whole file`);
  assert.equal(fragment.until, undefined, `${fragment.id} must use a whole file`);
  return readFileSync(join(trackRoot, fragment.path), 'utf8');
}

function selectedCriteria(pack) {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
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

test('the dependency catalog has exact whole-file instructions and focused checks', () => {
  const pack = readPack('feature-catalog-1.2.0.json');
  assert.equal(pack.state, 'draft');
  assert.deepEqual(pack.requiresPacks, []);
  const prompt = fragmentText(pack.task.requirements[0]);
  const contract = fragmentText(pack.task.contracts[0]);
  assert.match(prompt, /ten most-purchased items/i);
  assert.match(prompt, /without regard to case/i);
  assert.doesNotMatch(`${prompt}\n${contract}`, /framework|ORM|database|websocket/i);
  assert.deepEqual([...contract.matchAll(/`([^`]+)`/g)].map(match => match[1]), [
    'item-list', 'item-card', 'item-name', 'item-card', 'item-price', 'item-card',
    'item-stock', 'item-card', 'search-input', 'search-results', 'item-card',
  ]);
  assert.deepEqual(pack.checks.map(check => [check.id, check.criteria]), [
    ['values', ['2a']], ['ranking', ['2b']], ['search', ['2d']],
  ]);
  const selected = selectedCriteria(pack);
  assert.equal(selected.reduce((total, item) => total + item.criterion.points, 0), 3);
  assert(selected.every(item => item.feature.criteria.length === 1));
  assert(selected.every(item => item.feature.setup.length === 0));
});

test('faceted search covers every filter and stable pagination without prompt leakage', () => {
  const pack = readPack('progression-faceted-search-1.0.0.json');
  assert.equal(pack.state, 'draft');
  assert.deepEqual(pack.requiresPacks, ['ecommerce.feature.catalog@1.2.0']);
  assert.deepEqual(pack.capabilities, ['browser', 'direct-database-write']);
  const prompt = fragmentText(pack.task.requirements[0]);
  const contract = fragmentText(pack.task.contracts[0]);
  for (const text of ['category', 'minimum price', 'maximum price', 'availability',
    'six results per page']) assert.match(prompt, new RegExp(text, 'i'));
  for (const hook of ['category-filter', 'minimum-price', 'maximum-price', 'in-stock-filter',
    'search-results', 'item-card', 'search-next-page', 'search-previous-page']) {
    assert.match(contract, new RegExp(`\`${hook}\``));
  }
  assert.doesNotMatch(`${prompt}\n${contract}`, /framework|ORM|database|websocket/i);
  assert.deepEqual(pack.checks.map(check => [check.id, check.criteria]), [
    ['filters', ['401a']], ['pagination', ['402a']],
  ]);
  const selected = selectedCriteria(pack);
  assert.equal(selected.reduce((total, item) => total + item.criterion.points, 0), 6);
  assert(selected.every(item => item.feature.criteria.length === 1));

  const filters = selected.find(item => item.check.id === 'filters').feature;
  const setupTestIds = filters.setup.map(step => step.testid).filter(Boolean);
  assert.deepEqual(setupTestIds,
    ['category-filter', 'minimum-price', 'maximum-price', 'in-stock-filter']);
  const checks = filters.criteria[0].steps.filter(step => step.do === 'expect');
  for (const item of ['Coffee Grinder', 'Gaming Mouse', 'Desk Lamp', 'Espresso Machine',
    'Air Purifier']) assert(checks.some(step => step.contains === item));
});

test('catalog and faceted search graph nodes bind to the hardened draft packs', () => {
  const definition = readJson(join(trackRoot, 'progression', 'ecommerce-1.0.0.json'));
  const catalog = definition.nodes.find(node => node.id === 'catalog');
  const search = definition.nodes.find(node => node.id === 'faceted-search');
  assert.deepEqual(catalog.featureRefs, ['ecommerce.feature.catalog@1.2.0']);
  assert.deepEqual(catalog.gradingGroups, [
    'ecommerce.feature.catalog@1.2.0#values',
    'ecommerce.feature.catalog@1.2.0#ranking',
    'ecommerce.feature.catalog@1.2.0#search',
  ]);
  assert.deepEqual(search.featureRefs, ['ecommerce.progression.faceted-search@1.0.0']);
  assert.deepEqual(search.dependencies.map(dependency => dependency.id), ['catalog']);
});
